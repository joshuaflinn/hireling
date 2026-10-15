//! The diskops shell (MOR-637): a free-space guard and a sibling reaper.
//!
//! All *decisions* live in the library (`src/lib.rs`) as pure functions —
//! threshold, reapability, parsing — so they are unit-tested through the
//! production path. This binary is only the shell: it reads the environment,
//! runs `df` / `ps` / `git` / `du`, and maps the outcome to an exit code.
//!
//! Two subcommands, one per justfile recipe:
//!
//! - `guard --floor-gib <n>` — refuse (exit 1) when free space on the
//!   filesystem holding `CARGO_TARGET_DIR` (or the worktree) is below the
//!   floor, naming the condition, the number, and the remedy.
//! - `reap [--list|--prune] [--parent <dir>]` — list or remove reapable
//!   sibling `target-*` dirs and stale `hireling-*` worktrees, refusing any
//!   while a live build (cargo/rustc/npm/node) is running.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use diskops::{
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, ReapDecision, SiblingKind, classify_sibling,
    decide, format_gib, guard_outcome, is_live_build_process, parse_df, parse_floor_gib,
    skip_reason_text,
};

fn main() -> ExitCode {
    // No `std::process::exit` — the workspace denies `clippy::exit` outside
    // `main.rs`, and an `ExitCode` return carries the same signal.
    //
    // nosemgrep: `rust.lang.security.args` fires on any binary that reads
    // its own arguments (CWE-807). This binary only selects a subcommand and
    // a floor override from argv; it makes no security decision from it. The
    // floor is compared against a free-space measurement, and the reaper
    // refuses removal while a live build runs regardless of argv. Same
    // suppression the server binary carries at `src/main.rs:35`.
    let args: Vec<String> = std::env::args().skip(1).collect(); // nosemgrep
    match dispatch(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("diskops: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    let Some((subcommand, rest)) = args.split_first() else {
        return Err("no subcommand (expected guard or reap)".to_owned());
    };
    match subcommand.as_str() {
        "guard" => guard(rest),
        "reap" => reap(rest),
        other => Err(format!(
            "unknown subcommand '{other}' (expected guard or reap)"
        )),
    }
}

/// The disk floor: refuse `ci-local` before the expensive legs when free
/// space cannot absorb one more build.
fn guard(args: &[String]) -> Result<ExitCode, String> {
    let mut floor_gib = DEFAULT_FLOOR_GIB;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(value) = arg.strip_prefix("--floor-gib=") {
            floor_gib = parse_floor_gib(value)?;
        } else if arg == "--floor-gib" {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "guard: --floor-gib needs a number".to_owned())?;
            floor_gib = parse_floor_gib(value)?;
        } else {
            return Err(format!("guard: unknown option '{arg}'"));
        }
        index += 1;
    }

    let floor_bytes = floor_gib
        .checked_mul(GIB)
        .ok_or_else(|| format!("floor {floor_gib}G overflows a byte count"))?;

    // The filesystem that matters is the one holding the build output:
    // CARGO_TARGET_DIR when a seat points it elsewhere, else the worktree.
    // A CARGO_TARGET_DIR that does not yet exist (the detached-gate recipe
    // points at `../target-gate-<n>` before `just ci-local` creates it)
    // still names a filesystem — walk to the nearest existing ancestor.
    let target = std::env::var_os("CARGO_TARGET_DIR");
    let path: PathBuf = match target {
        Some(dir) => nearest_existing_ancestor(Path::new(&dir)),
        None => std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?,
    };

    let FreeSpace { available, mount } = df_avail(&path)?;

    match guard_outcome(available, floor_bytes, floor_gib, &mount) {
        GuardOutcome::Refuse { message } => {
            eprintln!("{message}");
            Ok(ExitCode::FAILURE)
        }
        GuardOutcome::Proceed { message } => {
            println!("{message}");
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn parse_reap_args(args: &[String]) -> Result<(bool, Option<PathBuf>), String> {
    let mut prune = false;
    let mut parent: Option<PathBuf> = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--prune" {
            prune = true;
        } else if arg == "--list" {
            prune = false;
        } else if let Some(value) = arg.strip_prefix("--parent=") {
            parent = Some(PathBuf::from(value));
        } else if arg == "--parent" {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "reap: --parent needs a directory".to_owned())?;
            parent = Some(PathBuf::from(value));
        } else {
            return Err(format!("reap: unknown option '{arg}'"));
        }
        index += 1;
    }
    Ok((prune, parent))
}

/// The sibling reaper. Dry-run by default; `--prune` actually removes.
fn reap(args: &[String]) -> Result<ExitCode, String> {
    let (prune, parent) = parse_reap_args(args)?;

    let parent_path = match parent {
        Some(path) => path,
        None => std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "the current directory has no parent".to_owned())?,
    };

    // One live-build read for the whole sweep — it is a global condition,
    // and only worth paying when a target-* sibling actually exists.
    let mut live_build: Option<bool> = None;
    let mut plan: Vec<(String, PathBuf, ReapDecision)> = Vec::new();
    let entries = std::fs::read_dir(&parent_path)
        .map_err(|error| format!("cannot read {}: {error}", parent_path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("cannot read an entry in {}: {error}", parent_path.display())
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot stat {}: {error}", entry.path().display()))?;
        if !file_type.is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            return Err("a sibling name is not valid UTF-8".to_owned());
        };
        let path = parent_path.join(&name);
        let kind = classify_sibling(&name);
        let is_ancestor = if kind == SiblingKind::Hireling {
            git_head_is_ancestor(&path)
        } else {
            false
        };
        let is_clean = if kind == SiblingKind::Hireling {
            git_tree_is_clean(&path)
        } else {
            false
        };
        let live = match kind {
            SiblingKind::Target | SiblingKind::Hireling => {
                *live_build.get_or_insert_with(live_build_present)
            }
            SiblingKind::Mor | SiblingKind::Reference | SiblingKind::Other => false,
        };
        let decision = decide(&name, is_ancestor, is_clean, live);
        plan.push((name, path, decision));
    }

    plan.sort_by(|a, b| a.0.cmp(&b.0));

    if prune {
        println!("reap: removing");
    } else {
        println!("reap: dry-run — nothing removed (pass --prune to remove)");
    }

    let mut reclaimed: u64 = 0;
    let mut unmeasured: u64 = 0;
    let mut count: u64 = 0;
    for (name, path, decision) in &plan {
        match decision {
            ReapDecision::Reap => {
                let size = dir_size_bytes(path);
                if let Some(bytes) = size {
                    reclaimed += bytes;
                } else {
                    unmeasured += 1;
                }
                let size_text = size.map_or_else(|| "?".to_owned(), format_gib);
                if prune {
                    remove_dir(path, name)?;
                    println!("removed {name} ({size_text})");
                } else {
                    println!("would remove {name} ({size_text})");
                }
                count += 1;
            }
            ReapDecision::Skip { reason } => {
                println!("skip {name} ({})", skip_reason_text(*reason));
            }
        }
    }

    let total = if unmeasured == 0 {
        format_gib(reclaimed)
    } else {
        format!(
            "{}+ ({} dirs unmeasured)",
            format_gib(reclaimed),
            unmeasured
        )
    };
    if prune {
        println!("reclaimed {total} ({count} dirs removed)");
    } else {
        println!("would reclaim {total} ({count} dirs)");
    }
    Ok(ExitCode::SUCCESS)
}

/// `df` can only report a filesystem for a path that exists. A
/// `CARGO_TARGET_DIR` that does not yet exist would make `df` exit
/// non-zero, so walk up to the nearest existing ancestor — the filesystem
/// is the same either way.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        if !current.pop() {
            return PathBuf::from("/");
        }
    }
}

/// Run `df -P -B1 <path>` and hand the data row to the library parser.
fn df_avail(path: &Path) -> Result<FreeSpace, String> {
    let output = Command::new("df")
        .args(["-P", "-B1"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot run df: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "df failed on {}: {}",
            path.to_string_lossy(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let Ok(stdout) = String::from_utf8(output.stdout) else {
        return Err("df output is not valid UTF-8".to_owned());
    };
    parse_df(&stdout)
}

/// Whether a build process (cargo/rustc/npm/node) outside this invocation's
/// ancestry is running. Fail-closed: an unreadable `ps` reads as "live".
fn live_build_present() -> bool {
    let excluded = ancestor_pids();
    let Ok(output) = Command::new("ps").args(["-eo", "pid=,cmd="]).output() else {
        return true;
    };
    if !output.status.success() {
        return true;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().any(|line| {
        let pid = line
            .split_whitespace()
            .next()
            .and_then(|token| token.parse::<u32>().ok());
        if let Some(pid) = pid
            && excluded.contains(&pid)
        {
            return false;
        }
        is_live_build_process(line)
    })
}

/// This process and every ancestor, so a reaper invoked through `cargo run`
/// never mistakes its own waiting wrapper for a live build.
fn ancestor_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    let mut current = std::process::id();
    pids.push(current);
    while let Some(parent) = parent_pid(current) {
        if parent <= 1 {
            break;
        }
        pids.push(parent);
        current = parent;
    }
    pids
}

/// The parent PID from `/proc/<pid>/stat` (the 4th field).
fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    rest.split_whitespace().nth(1)?.parse::<u32>().ok()
}

/// Whether `HEAD` is an ancestor of `origin/main` in the worktree at `path`.
/// Fail-closed: any git error — a missing `origin/main`, a directory that is
/// not a repository — reads as `false`, so an unverifiable worktree is never
/// reaped.
fn git_head_is_ancestor(path: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["merge-base", "--is-ancestor", "HEAD", "origin/main"])
        .output()
    else {
        return false;
    };
    matches!(output.status.code(), Some(0))
}

/// Whether the worktree at `path` has no uncommitted changes. Fail-closed:
/// any git error reads as `false` — an unreadable tree is treated as dirty
/// and skipped, never deleted.
fn git_tree_is_clean(path: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["status", "--porcelain"])
        .output()
    else {
        return false;
    };
    output.status.success() && output.stdout.is_empty()
}

fn git_worktree_remove(path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["worktree", "remove"])
        .arg(path)
        .output()
        .map_err(|error| {
            format!(
                "cannot run git worktree remove on {}: {error}",
                path.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "git worktree remove {} failed: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// The byte size of a directory via `du -s --block-size=1`, or `None` when
/// `du` is unavailable or fails — the report degrades to `?` rather than
/// aborting a dry-run on an unmeasurable dir.
fn dir_size_bytes(path: &Path) -> Option<u64> {
    let output = Command::new("du")
        .args(["-s", "--block-size=1"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.split_whitespace().next()?.parse::<u64>().ok()
}

/// Remove one reapable sibling: `git worktree remove` (never `--force`) for
/// worktrees, a plain recursive delete for derived build dirs. Re-checks the
/// kind so a bug that lets a non-reapable name through refuses here too.
fn remove_dir(path: &Path, name: &str) -> Result<(), String> {
    match classify_sibling(name) {
        SiblingKind::Target => std::fs::remove_dir_all(path)
            .map_err(|error| format!("cannot remove {}: {error}", path.display())),
        SiblingKind::Hireling => git_worktree_remove(path),
        SiblingKind::Mor | SiblingKind::Reference | SiblingKind::Other => Err(format!(
            "refusing to remove {} — not a reapable kind",
            path.display()
        )),
    }
}
