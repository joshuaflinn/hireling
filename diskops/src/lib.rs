//! Decision logic for the disk floor (MOR-637).
//!
//! Everything here is pure: no I/O, no `std::env`, no `std::fs`, no
//! `std::process`. The binary in `src/main.rs` reads the environment and
//! runs `df`, and feeds the result back through these functions — so the
//! threshold and refusal rules are unit-tested through the production path
//! instead of living inline in a recipe.

use std::path::{Path, PathBuf};

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
pub fn below_floor(free_bytes: u64, floor_bytes: u64) -> bool {
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
/// the number measured, and a remedy that exists at this head: free space,
/// or the `disk-floor-gb` just variable. The threshold test asserts the
/// message text.
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
                "error: {} free on {}, need {}G — free space, or lower the floor: just disk-floor-gb=<n>",
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

/// The anchor a `PathBuf::pop()`-exhausted walk resolves to. An absolute
/// path never exhausts — the root exists — so this is the defensive branch:
/// a relative path that walks to the empty path means the current directory
/// (`.`), whose filesystem is the one a relative `CARGO_TARGET_DIR` lives
/// on. Anchoring a relative path at `/` would report the root filesystem
/// instead of the worktree's for a single-component target dir.
#[must_use]
pub fn exhausted_walk_anchor(path_is_absolute: bool) -> &'static str {
    if path_is_absolute { "/" } else { "." }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
