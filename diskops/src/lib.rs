//! Decision logic for the disk floor and the sibling reaper (MOR-637).
//!
//! Everything here is pure: no I/O, no `std::env`, no `std::fs`, no
//! `std::process`. The binary in `src/main.rs` reads the environment, runs
//! `df` / `ps` / `git` / `du`, and feeds the results back through these
//! functions — so the threshold and reapability rules are unit-tested
//! through the production path instead of living inline in a recipe.

/// One gibibyte — the unit the floor and every reported figure share.
pub const GIB: u64 = 1024 * 1024 * 1024;

/// The default floor in gibibytes, when neither `disk-floor-gb` (just) nor
/// `DISK_FLOOR_GB` (env) overrides it.
///
/// Why 10: a full `target-gate-*` build measured 2.2G in-tree and up to 3.7G
/// behind an external `CARGO_TARGET_DIR`, so 10G is roughly three builds of
/// headroom on a bench that also holds checkouts, `node_modules`, and the
/// shared Postgres volume. CI overrides lower (`DISK_FLOOR_GB=3` in
/// gate.yml): a hosted runner starts near 14G free and cannot spare ten.
pub const DEFAULT_FLOOR_GIB: u64 = 10;

/// Whether `free_bytes` is below the floor — the guard's refusal condition.
#[must_use]
pub fn below_floor(free_bytes: u64, floor_bytes: u64) -> bool {
    free_bytes < floor_bytes
}

/// What a sibling directory name is, for the reaper's purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiblingKind {
    /// `target-*` — derived build output, safe to delete when idle.
    Target,
    /// `hireling-*` — a hand-cut worktree, reapable on the staleness test.
    Hireling,
    /// `MOR-<n>` — a harness-owned workspace, never touched.
    Mor,
    /// The shared `hireling` reference checkout.
    Reference,
    /// Any other name — not ours.
    Other,
}

/// Classify a sibling directory name.
#[must_use]
pub fn classify_sibling(name: &str) -> SiblingKind {
    if name == "hireling" {
        SiblingKind::Reference
    } else if name.starts_with("MOR-") {
        SiblingKind::Mor
    } else if name.starts_with("target-") {
        SiblingKind::Target
    } else if name.starts_with("hireling-") {
        SiblingKind::Hireling
    } else {
        SiblingKind::Other
    }
}

/// Why a sibling is left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A harness-owned `MOR-<n>` workspace — clean and merged can still be live.
    HarnessOwned,
    /// The shared `hireling` reference checkout.
    ReferenceCheckout,
    /// A live cargo/rustc build may be writing the derived dir.
    LiveBuild,
    /// Uncommitted changes — another seat's work.
    DirtyTree,
    /// Branch not an ancestor of `origin/main`.
    NotMerged,
    /// Not a name the reaper owns.
    NotReapable,
}

/// The reaper's verdict for one sibling directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapDecision {
    /// Remove it.
    Reap,
    /// Leave it alone, for `reason`.
    Skip { reason: SkipReason },
}

/// Decide whether a sibling is reapable.
///
/// `is_ancestor` reports whether the worktree's HEAD is an ancestor of
/// `origin/main`; `is_clean` whether its tree is clean; `live_build` whether
/// a cargo/rustc process outside this invocation is running. Only
/// `hireling-*` names consult the git state; only `target-*` names consult
/// the live-build state.
#[must_use]
pub fn decide(name: &str, is_ancestor: bool, is_clean: bool, live_build: bool) -> ReapDecision {
    match classify_sibling(name) {
        SiblingKind::Target => {
            if live_build {
                ReapDecision::Skip {
                    reason: SkipReason::LiveBuild,
                }
            } else {
                ReapDecision::Reap
            }
        }
        SiblingKind::Hireling => {
            if !is_clean {
                ReapDecision::Skip {
                    reason: SkipReason::DirtyTree,
                }
            } else if !is_ancestor {
                ReapDecision::Skip {
                    reason: SkipReason::NotMerged,
                }
            } else {
                ReapDecision::Reap
            }
        }
        SiblingKind::Mor => ReapDecision::Skip {
            reason: SkipReason::HarnessOwned,
        },
        SiblingKind::Reference => ReapDecision::Skip {
            reason: SkipReason::ReferenceCheckout,
        },
        SiblingKind::Other => ReapDecision::Skip {
            reason: SkipReason::NotReapable,
        },
    }
}

/// The human line for a skip reason, printed by `just reap`.
#[must_use]
pub fn skip_reason_text(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::HarnessOwned => "harness-owned MOR-<n> sibling",
        SkipReason::ReferenceCheckout => "shared reference checkout",
        SkipReason::LiveBuild => "live cargo/rustc build may be writing it",
        SkipReason::DirtyTree => "dirty tree — another seat's work",
        SkipReason::NotMerged => "branch not an ancestor of origin/main",
        SkipReason::NotReapable => "not a reapable name",
    }
}

/// Whether a process command line indicates a cargo/rustc build.
///
/// A substring match, deliberately at least as broad as the manual
/// `ps -eo pid,cmd | grep -E 'cargo|rustc'` sweep it replaces. Broad is the
/// safe direction: a false positive only makes the reaper refuse, never
/// delete something a build is writing.
#[must_use]
pub fn is_cargo_like(cmdline: &str) -> bool {
    cmdline.contains("cargo") || cmdline.contains("rustc")
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

/// Format a byte count as a one-decimal gibibyte figure (`"38.8G"`).
#[must_use]
pub fn format_gib(bytes: u64) -> String {
    // Tenths of a gibibyte, rounded half up, computed in u128 so a byte
    // count near `u64::MAX` cannot overflow the multiplication.
    let tenths = (u128::from(bytes) * 10 + u128::from(GIB) / 2) / u128::from(GIB);
    format!("{}.{}G", tenths / 10, tenths % 10)
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
