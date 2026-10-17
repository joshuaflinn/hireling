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
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, LiveStatus, ProcessObservation, ReapDecision,
    RemovalAction, SiblingKind, apply_removals, classify_sibling, decide, format_gib,
    guard_outcome, nearest_existing_ancestor, parse_df, parse_floor_gib, process_liveness,
    removal_action, skip_reason_text,
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
    reap_into(args, &mut std::io::stdout())
}

/// The `reap` path with its stdout handed in. `just reap` has no other
/// caller, so this is what lets a test drive the whole wiring — parse,
/// resolve the parent, read `/proc`, `build_plan`, `decide`, report and
/// remove — and read the plan text back, instead of calling `build_plan`
/// and re-deriving the decision the production path already made.
/// `remove_dir` is still the real removal, so `--prune` really removes;
/// the caller owns `--parent`, and nothing here can sweep the real sibling
/// set. `reap` is the only non-test caller.
fn reap_into(args: &[String], sink: &mut impl std::io::Write) -> Result<ExitCode, String> {
    let (prune, parent) = parse_reap_args(args)?;
    let parent_path = reap_parent(parent.as_deref())?;
    let observations = read_processes()?;
    let plan = build_plan(&parent_path, &observations)?;
    report_and_remove(&plan, prune, sink)
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
/// worktrees, the clean-tree test.
fn build_plan(
    parent: &Path,
    observations: &[ProcessObservation],
) -> Result<Vec<(String, PathBuf, SiblingKind, ReapDecision)>, String> {
    let entries = std::fs::read_dir(parent)
        .map_err(|error| format!("cannot read {}: {error}", parent.display()))?;
    let mut plan: Vec<(String, PathBuf, SiblingKind, ReapDecision)> = Vec::new();
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
        let liveness = match kind {
            SiblingKind::Target | SiblingKind::Hireling => observations
                .iter()
                .fold(LiveStatus::Idle, |acc, observation| {
                    acc.merge(process_liveness(observation, &path))
                }),
            SiblingKind::Mor | SiblingKind::Reference | SiblingKind::Other => LiveStatus::Idle,
        };
        let is_clean = if kind == SiblingKind::Hireling {
            git_tree_is_clean(&path)
        } else {
            false
        };
        let decision = decide(&name, is_clean, liveness);
        plan.push((name, path, kind, decision));
    }
    plan.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(plan)
}

/// One error string for a report write that failed. `String` has no
/// `From<io::Error>`, so each `writeln!` in `report_and_remove` names this
/// rather than `?`. The report text is what the caller reads back, so a
/// failed write is a reap failure, not something to swallow.
fn write_error(error: std::io::Error) -> String {
    #![allow(
        clippy::needless_pass_by_value,
        reason = "an io::Error is consumed into one String"
    )]
    format!("cannot write the reap report: {error}")
}

/// Print the plan and, in `--prune` mode, carry out the removals. Never
/// `git worktree remove --force` — a dirty tree is the owner's.
fn report_and_remove(
    plan: &[(String, PathBuf, SiblingKind, ReapDecision)],
    prune: bool,
    sink: &mut impl std::io::Write,
) -> Result<ExitCode, String> {
    if prune {
        writeln!(sink, "reap: removing").map_err(write_error)?;
    } else {
        writeln!(
            sink,
            "reap: dry-run — nothing removed (pass --prune to remove)"
        )
        .map_err(write_error)?;
    }

    let mut reclaimed_bytes: u64 = 0;
    let mut unmeasured: u64 = 0;
    let mut candidates: u64 = 0;
    let mut reapable: Vec<(String, PathBuf, SiblingKind)> = Vec::new();
    for (name, path, kind, decision) in plan {
        match decision {
            ReapDecision::Reap => {
                candidates += 1;
                reapable.push((name.clone(), path.clone(), *kind));
                if !prune {
                    let size = dir_size_bytes(path);
                    match size {
                        Some(bytes) => reclaimed_bytes += bytes,
                        None => unmeasured += 1,
                    }
                    let size_text = size.map_or_else(|| "?".to_owned(), format_gib);
                    writeln!(sink, "would remove {name} ({size_text})").map_err(write_error)?;
                }
            }
            ReapDecision::Skip { reason } => {
                writeln!(sink, "skip {name} ({})", skip_reason_text(*reason))
                    .map_err(write_error)?;
            }
        }
    }

    if prune {
        // Collect removals instead of `?`-propagating: one failed candidate
        // must not abandon the sweep.
        let applied = apply_removals(&reapable, |path, name, kind| {
            let size = dir_size_bytes(path);
            let size_text = size.map_or_else(|| "?".to_owned(), format_gib);
            match remove_dir(path, name, kind) {
                Ok(()) => {
                    writeln!(sink, "removed {name} ({size_text})").map_err(write_error)?;
                    match size {
                        Some(bytes) => reclaimed_bytes += bytes,
                        None => unmeasured += 1,
                    }
                    Ok(())
                }
                Err(message) => Err(message),
            }
        });
        for message in &applied.failures {
            writeln!(sink, "diskops: {message}").map_err(write_error)?;
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
        if applied.failures.is_empty() {
            writeln!(sink, "reclaimed {total} ({} dirs removed)", applied.removed)
                .map_err(write_error)?;
            Ok(ExitCode::SUCCESS)
        } else {
            writeln!(
                sink,
                "reclaimed {total} ({} dirs removed, {} failed)",
                applied.removed,
                applied.failures.len()
            )
            .map_err(write_error)?;
            Ok(ExitCode::FAILURE)
        }
    } else {
        let total = if unmeasured == 0 {
            format_gib(reclaimed_bytes)
        } else {
            format!(
                "{}+ ({} dirs unmeasured)",
                format_gib(reclaimed_bytes),
                unmeasured
            )
        };
        writeln!(sink, "would reclaim {total} ({candidates} dirs)").map_err(write_error)?;
        Ok(ExitCode::SUCCESS)
    }
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

/// Remove a candidate, branching on the kind `build_plan` already computed:
/// `remove_dir_all` for `target-*`, `git worktree remove` (never `--force`)
/// for `hireling-*`. The kind, not the name, is the source of truth here —
/// the delete path must not re-derive it.
fn remove_dir(path: &Path, name: &str, kind: SiblingKind) -> Result<(), String> {
    match removal_action(kind, name)? {
        RemovalAction::GitWorktreeRemove => git_worktree_remove(path),
        RemovalAction::RemoveDirAll => {
            std::fs::remove_dir_all(path).map_err(|error| format!("cannot remove {name}: {error}"))
        }
    }
}

/// Whether the tree at `worktree` is clean — the AGENTS.md clean-tree test,
/// and the whole of it since gh#354 retired the ancestry conjunct.
/// Fail-closed: an unreadable status reads as dirty.
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

/// The worktree that owns `worktree`'s admin data: the main checkout for a
/// linked worktree, or `worktree` itself when it is the main one.
///
/// `git worktree remove` is a command *of the owning repo*, so it has to run
/// with `cwd` inside that repo. The delete path cannot inherit the sweep's
/// cwd, because the sweep may be running from any sibling of the candidate:
/// with `cwd` at another checkout, `git worktree remove <abs path>` answers
/// `fatal: '<abs path>' is not a working tree` even though the argument is
/// absolute and the worktree is real. Measured both ways in a throwaway repo
/// (`.git` file holds an absolute gitdir, so this is the owning-repo lookup
/// failing, not path resolution):
///
/// - cwd = the main checkout, arg = absolute sibling path -> `not a working tree`, nothing removed
/// - cwd = the sibling itself, same absolute arg -> removed
///
/// The reaper's own cwd is the project directory, whose repo owns none of the
/// siblings, so without this resolution every `hireling-*` removal fails.
/// `git worktree list --porcelain` lists the main worktree first, so that is
/// the entry taken.
fn worktree_owner(worktree: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(worktree)
        .output()
        .map_err(|error| {
            format!(
                "cannot run git worktree list in {}: {error}",
                worktree.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "git worktree list in {} failed: {}",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let owner = stdout
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
        .ok_or_else(|| {
            format!(
                "no `worktree` line in git worktree list --porcelain from {}",
                worktree.display()
            )
        })?;
    Ok(PathBuf::from(owner))
}

/// Remove a hand-cut worktree via `git worktree remove`, never `--force`.
/// Run from the owning repo — see [`worktree_owner`] — because that is the
/// only cwd from which the candidate is a registered worktree.
fn git_worktree_remove(path: &Path) -> Result<(), String> {
    let owner = worktree_owner(path)?;
    let output = Command::new("git")
        .args(["worktree", "remove"])
        .arg(path)
        .current_dir(&owner)
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
/// for [`process_liveness`]. The reaper's own ancestors are excluded so a
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
        let (env_target_dir, env_readable) = read_environ_target_dir(pid);
        observations.push(ProcessObservation {
            cmdline,
            argv,
            cwd,
            env_target_dir,
            env_readable,
        });
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

/// The `CARGO_TARGET_DIR` a process was started with, from its
/// `/proc/<pid>/environ`, plus whether the environment was readable at all.
/// The detached-gate recipe sets `CARGO_TARGET_DIR="$PWD/../target-gate-<n>"`
/// on the process environment (never on argv), so this is the one place the
/// repo's `target-*` producer names its directory.
fn read_environ_target_dir(pid: u32) -> (Option<PathBuf>, bool) {
    let Ok(bytes) = std::fs::read(format!("/proc/{pid}/environ")) else {
        return (None, false);
    };
    let environ = String::from_utf8_lossy(&bytes);
    for entry in environ.split('\0') {
        if let Some(value) = entry.strip_prefix("CARGO_TARGET_DIR=") {
            if value.is_empty() {
                return (None, true);
            }
            return (Some(PathBuf::from(value)), true);
        }
    }
    (None, true)
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

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
