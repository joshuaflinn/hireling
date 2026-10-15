//! The diskops shell: a free-space guard (`just disk-guard`) and a reaper
//! for leaked build dirs (`just reap`).
//!
//! All *decisions* live in the library (`src/lib.rs`) as pure functions —
//! threshold, parsing, the refuse/proceed outcome, sibling classification,
//! and per-sibling liveness attribution — so they are unit-tested through
//! the production path. This binary is only the shell: it reads the
//! environment, runs `df`/`git`/`du`, reads `/proc`, and maps outcomes to
//! exit codes and removals.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use diskops::{
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, ProcessObservation, ReapDecision, SiblingKind,
    classify_sibling, decide, format_gib, guard_outcome, nearest_existing_ancestor, parse_df,
    parse_floor_gib, process_blocks_dir, skip_reason_text,
};

fn main() -> ExitCode {
    // No `std::process::exit` — the workspace denies `clippy::exit` outside
    // `main.rs`, and an `ExitCode` return carries the same signal.
    //
    // nosemgrep: `rust.lang.security.args` fires on any binary that reads
    // its own arguments (CWE-807). This binary only selects a subcommand
    // and its options from argv; it makes no security decision from them.
    // The floor is compared against a free-space measurement and the reaper
    // never deletes a directory outside the sweep parent, both regardless of
    // argv. Same suppression the server binary carries at `src/main.rs:35`.
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
        Some(dir) => nearest_existing_ancestor(Path::new(&dir), &|p| p.exists()),
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

/// The reaper: sweep the parent directory for leaked `target-*` dirs and
/// stale `hireling-*` worktrees, dry-run by default, `--prune` to remove.
fn reap(args: &[String]) -> Result<ExitCode, String> {
    let (prune, parent) = parse_reap_args(args)?;
    let parent_path = reap_parent(parent.as_deref())?;
    let observations = read_processes()?;
    let plan = build_plan(&parent_path, &observations)?;
    report_and_remove(&plan, prune)
}

/// Parse `reap` options: `--prune` (remove) vs `--list`/default (dry-run),
/// and `--parent <dir>` (override the sweep parent, for tests).
fn parse_reap_args(args: &[String]) -> Result<(bool, Option<PathBuf>), String> {
    let mut prune = false;
    let mut parent: Option<PathBuf> = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(value) = arg.strip_prefix("--parent=") {
            parent = Some(PathBuf::from(value));
        } else if arg == "--parent" {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "reap: --parent needs a directory".to_owned())?;
            parent = Some(PathBuf::from(value));
        } else if arg == "--prune" {
            prune = true;
        } else if arg == "--list" {
            prune = false;
        } else {
            return Err(format!("reap: unknown option '{arg}'"));
        }
        index += 1;
    }
    Ok((prune, parent))
}

/// Resolve the sweep parent: `--parent`, else the parent of the current
/// directory (the justfile and the siblings share the checkout's parent).
/// Canonicalized so candidate paths compare cleanly against the resolved
/// `/proc/<pid>/cwd` values.
fn reap_parent(parent: Option<&Path>) -> Result<PathBuf, String> {
    let path: PathBuf = match parent {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "the current directory has no parent to sweep".to_owned())?,
    };
    std::fs::canonicalize(&path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))
}

/// Build the per-candidate plan: classify each directory in the parent and
/// decide its fate from liveness (per candidate) and, for hand-cut
/// worktrees, the staleness test.
fn build_plan(
    parent: &Path,
    observations: &[ProcessObservation],
) -> Result<Vec<(String, PathBuf, ReapDecision)>, String> {
    let entries = std::fs::read_dir(parent)
        .map_err(|error| format!("cannot read {}: {error}", parent.display()))?;
    let mut plan: Vec<(String, PathBuf, ReapDecision)> = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("cannot read an entry in {}: {error}", parent.display()))?;
        if !entry
            .file_type()
            .map_err(|error| format!("cannot stat {}: {error}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let path = parent.join(&name);
        let kind = classify_sibling(&name);
        let live = match kind {
            SiblingKind::Target | SiblingKind::Hireling => observations
                .iter()
                .any(|observation| process_blocks_dir(observation, &path)),
            SiblingKind::Mor | SiblingKind::Reference | SiblingKind::Other => false,
        };
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
        let decision = decide(&name, is_ancestor, is_clean, live);
        plan.push((name, path, decision));
    }
    plan.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(plan)
}

/// Print the plan and, in `--prune` mode, carry out the removals. Never
/// `git worktree remove --force` — a dirty tree is the owner's.
fn report_and_remove(
    plan: &[(String, PathBuf, ReapDecision)],
    prune: bool,
) -> Result<ExitCode, String> {
    if prune {
        println!("reap: removing");
    } else {
        println!("reap: dry-run — nothing removed (pass --prune to remove)");
    }
    let mut reclaimed_bytes: u64 = 0;
    let mut unmeasured: u64 = 0;
    let mut count: u64 = 0;
    for (name, path, decision) in plan {
        match decision {
            ReapDecision::Reap => {
                let size = dir_size_bytes(path);
                match size {
                    Some(bytes) => reclaimed_bytes += bytes,
                    None => unmeasured += 1,
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
        format_gib(reclaimed_bytes)
    } else {
        format!(
            "{}+ ({} dirs unmeasured)",
            format_gib(reclaimed_bytes),
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

/// The size of a directory as `du -s --block-size=1` reports it, or `None`
/// when `du` cannot measure it (gone, or unreadable).
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

/// Remove a candidate: `remove_dir_all` for `target-*`, `git worktree
/// remove` (never `--force`) for `hireling-*`.
fn remove_dir(path: &Path, name: &str) -> Result<(), String> {
    if name.starts_with("hireling-") {
        return git_worktree_remove(path);
    }
    std::fs::remove_dir_all(path).map_err(|error| format!("cannot remove {name}: {error}"))
}

/// Whether the branch at `worktree` HEAD is an ancestor of `origin/main` —
/// the AGENTS.md staleness test's first half.
fn git_head_is_ancestor(worktree: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .args(["merge-base", "--is-ancestor", "HEAD", "origin/main"])
        .current_dir(worktree)
        .output()
    else {
        return false;
    };
    output.status.success()
}

/// Whether the tree at `worktree` is clean — the AGENTS.md staleness test's
/// second half. Fail-closed: an unreadable status reads as dirty.
fn git_tree_is_clean(worktree: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree)
        .output()
    else {
        return false;
    };
    output.status.success() && output.stdout.is_empty()
}

/// Remove a hand-cut worktree via `git worktree remove`, never `--force`.
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

/// Read every observable process from `/proc`, as a list of observations
/// for [`process_blocks_dir`]. The reaper's own ancestors are excluded so a
/// `cargo run` wrapper never attributes its own waiting parent to a
/// candidate. Fail-closed: an unreadable `/proc` is an error, so `--prune`
/// can never delete on an unknown liveness state.
fn read_processes() -> Result<Vec<ProcessObservation>, String> {
    let excluded = ancestor_pids();
    let entries =
        std::fs::read_dir("/proc").map_err(|error| format!("cannot read /proc: {error}"))?;
    let mut observations: Vec<ProcessObservation> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read a /proc entry: {error}"))?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if excluded.contains(&pid) {
            continue;
        }
        let Some(argv) = read_argv(pid) else {
            continue;
        };
        if argv.is_empty() {
            continue;
        }
        let cmdline = argv.join(" ");
        let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
        observations.push(ProcessObservation { cmdline, argv, cwd });
    }
    Ok(observations)
}

/// The NUL-separated argv of `/proc/<pid>/cmdline`, split into elements.
/// `None` when the file cannot be read — a process whose argv we cannot
/// observe is skipped, never treated as a build.
fn read_argv(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let argv: Vec<String> = text
        .split('\0')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    Some(argv)
}

/// The pids of every ancestor of the current process, walking `/proc/<pid>/stat`.
fn ancestor_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    let mut current = std::process::id();
    while let Some(parent) = parent_pid(current) {
        if parent == 0 || parent == current {
            break;
        }
        pids.push(parent);
        current = parent;
    }
    pids
}

/// The parent pid of `pid`, from the fourth field of `/proc/<pid>/stat`.
fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    rest.split_whitespace().nth(1)?.parse::<u32>().ok()
}
