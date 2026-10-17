//! Decision logic for the disk guard and the disk reaper (MOR-637 / MOR-649).
//!
//! Everything here is pure: no I/O, no `std::env`, no `std::fs`, no
//! `std::process`. The binary in `src/main.rs` reads the environment,
//! `/proc`, and `df`/`git` output, and feeds the results back through these
//! functions — so the floor threshold, the refusal rules, and the reaper's
//! per-sibling liveness attribution are unit-tested through the production
//! path instead of living inline in a recipe.

use std::path::{Component, Path, PathBuf};

/// One gibibyte — the unit the floor and every reported figure share.
pub const GIB: u64 = 1024 * 1024 * 1024;

/// The default floor in gibibytes, when neither `disk-floor-gb` (just) nor
/// `DISK_FLOOR_GB` (env) overrides it.
///
/// Why 10: a full `target-gate-*` build measured 2.2G in-tree and up to 3.7G
/// behind an external `CARGO_TARGET_DIR`, so 10G is roughly three builds of
/// headroom on a bench that also holds checkouts, `node_modules`, and the
/// shared Postgres volume. CI runs the same default: a hosted runner logged
/// `disk-guard ok: 75.4G free on /` with the gate cache already restored, so
/// no override is needed.
pub const DEFAULT_FLOOR_GIB: u64 = 10;

/// Whether `free_bytes` is below the floor — the guard's refusal condition.
#[must_use]
fn below_floor(free_bytes: u64, floor_bytes: u64) -> bool {
    free_bytes < floor_bytes
}

/// One filesystem's free space as `df -P -B1 <path>` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeSpace {
    /// Available bytes — the `Available` column.
    pub available: u64,
    /// The mount point — the `Mounted on` column.
    pub mount: String,
}

/// Parse the data row of `df -P -B1 <path>` output.
///
/// # Errors
///
/// Returns an error when the output has no data row, when the `Available`
/// column is not a byte count, or when no mount point is present.
pub fn parse_df(output: &str) -> Result<FreeSpace, String> {
    let data_row = output
        .lines()
        .next_back()
        .ok_or_else(|| "df output is empty — cannot read free space".to_owned())?;
    let fields: Vec<&str> = data_row.split_whitespace().collect();
    let available = fields
        .get(3)
        .ok_or_else(|| "df output has no Available column".to_owned())?
        .parse::<u64>()
        .map_err(|error| {
            format!("df Available column is not a byte count ({error}): {data_row}")
        })?;
    let mount = fields
        .get(5..)
        .ok_or_else(|| "df output has no mount point".to_owned())?
        .join(" ");
    if mount.is_empty() {
        return Err("df output has an empty mount point".to_owned());
    }
    Ok(FreeSpace { available, mount })
}

/// Parse a floor in gibibytes — a plain non-negative integer (`"10"`).
///
/// # Errors
///
/// Returns an error when the input is empty or not a base-10 integer.
pub fn parse_floor_gib(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("floor is empty".to_owned());
    }
    trimmed
        .parse::<u64>()
        .map_err(|error| format!("floor '{trimmed}' is not a whole number of gibibytes ({error})"))
}

/// Format a byte count as a one-decimal gibibyte figure (`"38.7G"`),
/// truncated, never rounded — a free-space report must not overstate.
#[must_use]
pub fn format_gib(bytes: u64) -> String {
    // Tenths of a gibibyte, truncated, computed in u128 so a byte count
    // near `u64::MAX` cannot overflow the multiplication. Truncation means
    // a figure just below the floor can never display as the floor it is
    // refusing against (half-up rounding did — gh#301 round 1 F1).
    let tenths = u128::from(bytes) * 10 / u128::from(GIB);
    format!("{}.{}G", tenths / 10, tenths % 10)
}

/// The disk guard's verdict for one reading, mapped to an exit code by the
/// shell: [`GuardOutcome::Refuse`] exits non-zero, [`GuardOutcome::Proceed`]
/// exits zero. The exact human line rides along so the message is asserted
/// on both sides of the threshold, not inferred from `below_floor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardOutcome {
    /// Free space is below the floor — the shell must refuse.
    Refuse { message: String },
    /// Free space is at or above the floor — the shell must proceed.
    Proceed { message: String },
}

/// Decide the guard's outcome from a free-space reading.
///
/// The refusal message names the condition (free space below the floor),
/// the number measured, and the remedies that exist at this head: `just
/// reap` (reclaim leaked build dirs) and the `disk-floor-gb` just variable
/// (lower the floor). The threshold test asserts the message text.
#[must_use]
pub fn guard_outcome(
    free_bytes: u64,
    floor_bytes: u64,
    floor_gib: u64,
    mount: &str,
) -> GuardOutcome {
    if below_floor(free_bytes, floor_bytes) {
        GuardOutcome::Refuse {
            message: format!(
                "error: {} free on {}, need {}G — run 'just reap' to reclaim leaked build dirs, or lower the floor: just disk-floor-gb=<n>",
                format_gib(free_bytes),
                mount,
                floor_gib
            ),
        }
    } else {
        GuardOutcome::Proceed {
            message: format!(
                "disk-guard ok: {} free on {}, floor {}G",
                format_gib(free_bytes),
                mount,
                floor_gib
            ),
        }
    }
}

/// What a directory in the sweep parent is, decided on its name alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiblingKind {
    /// `target-*` — a leaked external `CARGO_TARGET_DIR` (the detached-gate
    /// recipe names `target-gate-<n>`). Reapable once no live build writes
    /// inside it.
    Target,
    /// `hireling-*` — a hand-cut worktree. Reapable on the AGENTS.md
    /// clean-tree test alone: a clean tree means nothing in it can be lost,
    /// since `git worktree remove` leaves the branch ref in place.
    Hireling,
    /// `MOR-<n>` — a harness-provisioned workspace. Never reaped, however
    /// stale it looks: a clean `MOR-<n>` that is an ancestor of `main` can
    /// still be a live harness-owned workspace.
    Mor,
    /// `hireling` — the shared reference checkout. Never reaped.
    Reference,
    /// Anything else — not a reapable sibling.
    Other,
}

/// Classify a sibling directory name.
#[must_use]
pub fn classify_sibling(name: &str) -> SiblingKind {
    if name == "hireling" {
        SiblingKind::Reference
    } else if name.starts_with("MOR-") {
        SiblingKind::Mor
    } else if name.starts_with("hireling-") {
        SiblingKind::Hireling
    } else if name.starts_with("target-") {
        SiblingKind::Target
    } else {
        SiblingKind::Other
    }
}

/// Why a candidate is skipped, in operator-facing words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// `MOR-<n>` — harness-owned, never reaped.
    HarnessOwned,
    /// The shared `hireling` checkout — never reaped.
    ReferenceCheckout,
    /// A live build is writing inside the candidate.
    LiveBuild,
    /// A build-token process could not be observed — its working directory
    /// or environment is unreadable — so it reads as live, fail-closed.
    Unobservable,
    /// A hand-cut worktree with uncommitted work.
    DirtyTree,
    /// A name outside the reapable set.
    NotReapable,
}

/// The reaper's decision for one candidate directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReapDecision {
    /// Remove the directory.
    Reap,
    /// Leave it, for `reason`.
    Skip { reason: SkipReason },
}

/// The liveness signal for one candidate — three states, so an unobservable
/// process is reported as such rather than as a confirmed live build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStatus {
    /// Nothing attributes a live build to the candidate.
    Idle,
    /// A build-token process is attributed inside the candidate.
    Live,
    /// A build-token process could not be observed — its working directory
    /// or environment is unreadable — so it reads as live, fail-closed.
    Unobservable,
}

impl LiveStatus {
    /// Combine two observations' statuses for one candidate: a confirmed
    /// live build outranks an unobservable one, which outranks idle.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Live, _) | (_, Self::Live) => Self::Live,
            (Self::Unobservable, _) | (_, Self::Unobservable) => Self::Unobservable,
            _ => Self::Idle,
        }
    }
}

/// Decide a candidate's fate. The hard exclusions are decided on the name
/// *before* liveness or staleness is consulted, so they hold regardless of
/// `liveness` or `is_clean`: `MOR-<n>` and the shared `hireling` checkout are
/// skipped even when a build is writing inside them.
///
/// A hand-cut `hireling-*` sibling is reapable on the clean-tree test alone:
/// `git worktree remove` leaves the local branch ref in place, so its commits
/// stay reachable whatever the merge state. The ancestry conjunct this used
/// to require is retired (gh#354) precisely because `main` squash-merges and a
/// merged branch is normally *not* an ancestor of `origin/main`.
#[must_use]
pub fn decide(name: &str, is_clean: bool, liveness: LiveStatus) -> ReapDecision {
    let kind = classify_sibling(name);
    match kind {
        SiblingKind::Mor => ReapDecision::Skip {
            reason: SkipReason::HarnessOwned,
        },
        SiblingKind::Reference => ReapDecision::Skip {
            reason: SkipReason::ReferenceCheckout,
        },
        SiblingKind::Other => ReapDecision::Skip {
            reason: SkipReason::NotReapable,
        },
        SiblingKind::Target | SiblingKind::Hireling => {
            match liveness {
                LiveStatus::Live => {
                    return ReapDecision::Skip {
                        reason: SkipReason::LiveBuild,
                    };
                }
                LiveStatus::Unobservable => {
                    return ReapDecision::Skip {
                        reason: SkipReason::Unobservable,
                    };
                }
                LiveStatus::Idle => {}
            }
            if kind == SiblingKind::Hireling && !is_clean {
                return ReapDecision::Skip {
                    reason: SkipReason::DirtyTree,
                };
            }
            ReapDecision::Reap
        }
    }
}

/// The operator-facing reason text for a skip.
#[must_use]
pub fn skip_reason_text(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::HarnessOwned => "harness-owned MOR-<n> workspace",
        SkipReason::ReferenceCheckout => "the shared hireling checkout",
        SkipReason::LiveBuild => "a live build is writing inside it",
        SkipReason::Unobservable => {
            "a build-token process whose working directory or environment could not be read"
        }
        SkipReason::DirtyTree => "dirty worktree",
        SkipReason::NotReapable => "not a reapable sibling",
    }
}

/// One observed process, as the reaper shell reads it from `/proc`.
///
/// - `cmdline`: the argv elements joined with spaces — the string the build
///   token match runs over.
/// - `argv`: the NUL-separated argv elements, for the path-under-argv check.
/// - `cwd`: the resolved working directory (`read_link /proc/<pid>/cwd`),
///   or `None` when it cannot be read — which the liveness check treats as
///   live, never idle (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessObservation {
    /// The argv joined with spaces.
    pub cmdline: String,
    /// The argv elements.
    pub argv: Vec<String>,
    /// The resolved working directory, or `None` when unreadable.
    pub cwd: Option<PathBuf>,
    /// The `CARGO_TARGET_DIR` named in the process environment, or `None`
    /// when it names none or the environment could not be read.
    pub env_target_dir: Option<PathBuf>,
    /// Whether `/proc/<pid>/environ` was readable. A build-token process
    /// whose environment is unreadable reads as live — the detached-gate
    /// recipe names its target dir there, so an unreadable environment
    /// cannot be ruled out.
    pub env_readable: bool,
}

/// Whether `cmdline` names a build tool — the pre-filter before attribution.
///
/// Substring matching over the joined argv is deliberate: `node` and `npm`
/// also appear in the install *paths* of permanent tooling (Paperclip's
/// embedded Postgres runs at `.../installs/npm/.../@embedded-postgres/...`
/// for the whole system uptime), so this matcher over-matches by design.
/// The over-match is harmless because it only selects processes worth
/// attributing — whether any one of them blocks a candidate is decided by
/// [`process_liveness`] on the process's cwd, not by this match alone.
#[must_use]
pub fn is_live_build_process(cmdline: &str) -> bool {
    ["cargo", "rustc", "npm", "node"]
        .iter()
        .any(|token| cmdline.contains(token))
}

/// Whether `child` is `parent` itself or a descendant of it, compared
/// component-wise — `/a/foobar` is not within `/a/foo`. Both sides must be
/// absolute, resolved paths: the candidate is canonicalized by the shell and
/// the cwd comes from `read_link /proc/<pid>/cwd`, which is already
/// resolved.
#[must_use]
pub fn path_is_within(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

/// Whether any argv element is an absolute path under `candidate`.
///
/// This is the argv half of the liveness check: `cargo build --target-dir
/// <candidate>` writes into the candidate while its cwd is elsewhere, and
/// the cwd check alone would miss it. Only absolute argv elements are
/// compared, and the comparison is lexical — a relative or `..`-carrying
/// argv path is *not* resolved against the process cwd. Note the direction
/// of that residual: a missed argv match on a live build means the reaper
/// deletes a directory a build is writing — the over-delete direction. It
/// is accepted only because the cwd and environment checks are the primary
/// signals, and this repo's recipes name no target dir on argv at all
/// (`--target-dir` appears on no recipe; F1).
#[must_use]
pub fn argv_names_path(argv: &[String], candidate: &Path) -> bool {
    argv.iter().any(|argument| {
        let path = Path::new(argument);
        path.is_absolute() && path_is_within(path, candidate)
    })
}

/// Lexically normalize `path` — resolve `.` and `..` components without
/// touching the filesystem — so an unresolved `CARGO_TARGET_DIR` compares
/// cleanly against the canonicalized candidate. The detached-gate recipe
/// writes `CARGO_TARGET_DIR="$PWD/../target-gate-<n>"`, which still carries
/// a `..` when cargo reads it; a lexical `starts_with` against the
/// canonicalized candidate would miss it, and the environment is how the
/// repo's only `target-*` producer names its directory.
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => match parts.last().map(std::ffi::OsString::as_os_str) {
                Some(prev) if prev == ".." => parts.push(std::ffi::OsString::from("..")),
                Some(prev) if prev == "/" => {}
                Some(_) => {
                    parts.pop();
                }
                None => parts.push(std::ffi::OsString::from("..")),
            },
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                parts.push(component.as_os_str().to_owned());
            }
        }
    }
    parts.into_iter().collect()
}

/// Resolve a `CARGO_TARGET_DIR` value to a normalized absolute path.
///
/// An absolute value normalizes directly. A relative value is resolved
/// against the process's observed cwd before normalizing — compared against
/// a canonicalized candidate, a relative value never matches otherwise.
/// When the cwd is unreadable and the value is relative, the path cannot be
/// resolved and this returns `None`; a build-token process in that state
/// reads unobservable, never idle.
#[must_use]
fn resolve_env_target_dir(dir: &Path, cwd: Option<&Path>) -> Option<PathBuf> {
    if dir.is_absolute() {
        Some(normalize_path(dir))
    } else {
        cwd.map(|cwd| normalize_path(&cwd.join(dir)))
    }
}

/// How one observed process bears on reaping `candidate`.
///
/// Liveness is decided per candidate directory, not per process-table line:
/// a process whose working directory is outside the candidate does not block
/// it, even when its command line matches the build token set — the `npm`/
/// `node` tokens appear in the install paths of permanent tooling, and a
/// global match made the first reaper refuse forever (gh#301 round 2 F1).
///
/// The environment is consulted before the token pre-filter, because it is
/// per-candidate and exact: a process whose `CARGO_TARGET_DIR` names the
/// candidate blocks it regardless of its command line. The gate recipe's
/// wrapper (`just`, `python3`, `semgrep`, `sh`) holds the variable while no
/// `cargo`/`rustc`/`npm`/`node` token appears on argv, so the token match
/// alone would attribute nothing. The pre-filter exists only to stop a
/// `node`-in-a-path process from blocking globally.
///
/// A process is [`LiveStatus::Live`] against the candidate when one of:
///
/// - its environment names a `CARGO_TARGET_DIR` under the candidate (the
///   [`ProcessObservation::env_target_dir`] field), resolved against its
///   observed cwd when the value is relative — the detached-gate recipe is
///   the repo's only `target-*` producer and names the directory here, not
///   on argv; or
/// - its cwd is the candidate or a descendant of it — the real vitest reads
///   `<parent>/hireling-488-stash-web/web`, inside the sibling; or
/// - its argv names an absolute path under the candidate (see
///   [`argv_names_path`]).
///
/// It is [`LiveStatus::Unobservable`] when its cwd or environment cannot be
/// read — fail-closed: an unobservable process reads as live, never idle.
#[must_use]
pub fn process_liveness(observation: &ProcessObservation, candidate: &Path) -> LiveStatus {
    if let Some(dir) = &observation.env_target_dir
        && let Some(resolved) = resolve_env_target_dir(dir, observation.cwd.as_deref())
        && path_is_within(&resolved, candidate)
    {
        return LiveStatus::Live;
    }
    if !is_live_build_process(&observation.cmdline) {
        return LiveStatus::Idle;
    }
    let Some(cwd) = &observation.cwd else {
        return LiveStatus::Unobservable;
    };
    if path_is_within(cwd, candidate) {
        return LiveStatus::Live;
    }
    if argv_names_path(&observation.argv, candidate) {
        return LiveStatus::Live;
    }
    match &observation.env_target_dir {
        None if !observation.env_readable => LiveStatus::Unobservable,
        Some(_) | None => LiveStatus::Idle,
    }
}

/// Walk `path` up to the nearest existing ancestor. The existence check is
/// injected — the shell passes `Path::exists`, tests pass a fixed answer —
/// so the walk is unit-tested without touching a filesystem.
///
/// A `CARGO_TARGET_DIR` that does not yet exist would make `df` exit
/// non-zero (the detached-gate recipe points at `../target-gate-<n>` before
/// `just ci-local` creates it), so the shell walks up to the nearest
/// existing ancestor — the filesystem is the same either way.
#[must_use]
pub fn nearest_existing_ancestor(path: &Path, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if exists(&current) {
            return current;
        }
        if !current.pop() {
            return PathBuf::from(exhausted_walk_anchor(path.is_absolute()));
        }
    }
}

/// The result of applying removals to every reapable candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalsOutcome {
    /// How many candidates were removed.
    pub removed: u64,
    /// The error messages for candidates that failed, in sweep order.
    pub failures: Vec<String>,
}

/// Apply a removal to each reapable candidate, continuing past failures.
///
/// One failed candidate must not abandon the sweep: a failed removal would
/// leave every later candidate untouched on a full disk — exactly when the
/// reaper is reached for. The removal action itself is injected by the
/// shell (`std::fs::remove_dir_all` or `git worktree remove`); this
/// function owns the control flow — collect, don't propagate — and the exit
/// signal.
pub fn apply_removals(
    entries: &[(String, PathBuf, SiblingKind)],
    mut remove: impl FnMut(&Path, &str, SiblingKind) -> Result<(), String>,
) -> RemovalsOutcome {
    let mut removed = 0;
    let mut failures = Vec::new();
    for (name, path, kind) in entries {
        match remove(path, name, *kind) {
            Ok(()) => removed += 1,
            Err(message) => failures.push(message),
        }
    }
    RemovalsOutcome { removed, failures }
}

/// The removal the delete path performs for one candidate, decided on its
/// kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalAction {
    /// `remove_dir_all` — a leaked `target-*` dir.
    RemoveDirAll,
    /// `git worktree remove` (never `--force`) — a hand-cut worktree.
    GitWorktreeRemove,
}

/// Decide the removal for a candidate from its kind. The exclusion is a
/// property of the delete path, not only of [`decide`]: `MOR-<n>`, the
/// shared checkout, and unknown names are refused outright here, so the one
/// function that deletes things can never treat a harness-owned workspace
/// as a removal candidate however it was reached.
///
/// # Errors
///
/// Returns `refusing to remove <name>: not a reapable kind` for
/// [`SiblingKind::Mor`], [`SiblingKind::Reference`], and
/// [`SiblingKind::Other`].
pub fn removal_action(kind: SiblingKind, name: &str) -> Result<RemovalAction, String> {
    match kind {
        SiblingKind::Target => Ok(RemovalAction::RemoveDirAll),
        SiblingKind::Hireling => Ok(RemovalAction::GitWorktreeRemove),
        SiblingKind::Mor | SiblingKind::Reference | SiblingKind::Other => {
            Err(format!("refusing to remove {name}: not a reapable kind"))
        }
    }
}

/// The anchor a `PathBuf::pop()`-exhausted walk resolves to. An absolute
/// path never exhausts — the root exists — so this is the defensive branch:
/// a relative path that walks to the empty path means the current directory
/// (`.`), whose filesystem is the one a relative `CARGO_TARGET_DIR` lives
/// on. Anchoring a relative path at `/` would report the root filesystem
/// instead of the worktree's for a single-component target dir.
#[must_use]
fn exhausted_walk_anchor(path_is_absolute: bool) -> &'static str {
    if path_is_absolute { "/" } else { "." }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
