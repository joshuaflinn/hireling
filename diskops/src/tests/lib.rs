//! Tests for [`super`].
//!
//! Loaded by `src/lib.rs` via `#[path = "tests/lib.rs"] mod tests;`, which
//! makes this a *child* of that module — so `use super::*` reaches private
//! items exactly as an inline `mod tests` would. No visibility inflation.
//!
//! Note there is no `#[expect(clippy::unwrap_used, ...)]` header here. The
//! root `clippy.toml` already exempts unwrap/expect/panic/dbg in tests, so an
//! expectation would never fire and would itself become an
//! unfulfilled-expectation error under `-D warnings`.

use super::{
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, ReapDecision, SiblingKind, SkipReason, below_floor,
    classify_sibling, decide, format_gib, is_cargo_like, parse_df, parse_floor_gib,
};

const FLOOR_BYTES: u64 = DEFAULT_FLOOR_GIB * GIB;

#[test]
fn floor_minus_one_is_below() {
    assert!(
        below_floor(FLOOR_BYTES - 1, FLOOR_BYTES),
        "one byte below the floor must refuse"
    );
}

#[test]
fn floor_exactly_is_not_below() {
    assert!(
        !below_floor(FLOOR_BYTES, FLOOR_BYTES),
        "exactly at the floor must pass"
    );
}

#[test]
fn above_floor_is_not_below() {
    assert!(
        !below_floor(FLOOR_BYTES + 1, FLOOR_BYTES),
        "one byte above the floor must pass"
    );
}

#[test]
fn classify_names_each_kind() {
    assert_eq!(
        classify_sibling("target-gate-486b"),
        SiblingKind::Target,
        "target-* is a derived build dir"
    );
    assert_eq!(
        classify_sibling("hireling-488-stash-web"),
        SiblingKind::Hireling,
        "hireling-* is a hand-cut worktree"
    );
    assert_eq!(
        classify_sibling("MOR-482"),
        SiblingKind::Mor,
        "MOR-<n> is harness-owned"
    );
    assert_eq!(
        classify_sibling("hireling"),
        SiblingKind::Reference,
        "the bare hireling name is the shared checkout"
    );
    assert_eq!(
        classify_sibling("mor28-workspace-archive"),
        SiblingKind::Other,
        "a lowercase mor* name is not the harness namespace"
    );
}

#[test]
fn decide_reaps_merged_clean_worktree() {
    assert_eq!(
        decide("hireling-488-stash-web", true, true, false),
        ReapDecision::Reap,
        "a clean worktree merged into origin/main is reapable"
    );
}

#[test]
fn decide_skips_dirty_worktree() {
    assert_eq!(
        decide("hireling-488-stash-web", true, false, false),
        ReapDecision::Skip {
            reason: SkipReason::DirtyTree
        },
        "uncommitted changes are another seat's work — never touch them"
    );
}

#[test]
fn decide_skips_unmerged_worktree() {
    assert_eq!(
        decide("hireling-488-stash-web", false, true, false),
        ReapDecision::Skip {
            reason: SkipReason::NotMerged
        },
        "a branch not merged into origin/main still holds work"
    );
}

#[test]
fn decide_reaps_idle_target_dir() {
    assert_eq!(
        decide("target-gate-486b", false, false, false),
        ReapDecision::Reap,
        "an idle target-* dir is safe to delete"
    );
}

#[test]
fn decide_skips_target_dir_with_live_build() {
    assert_eq!(
        decide("target-gate-486b", false, false, true),
        ReapDecision::Skip {
            reason: SkipReason::LiveBuild
        },
        "a live build may be writing the dir — refuse"
    );
}

#[test]
fn decide_skips_mor_sibling() {
    assert_eq!(
        decide("MOR-482", true, true, false),
        ReapDecision::Skip {
            reason: SkipReason::HarnessOwned
        },
        "a harness-owned workspace is never reapable, even clean and merged"
    );
}

#[test]
fn decide_skips_reference_checkout() {
    assert_eq!(
        decide("hireling", true, true, false),
        ReapDecision::Skip {
            reason: SkipReason::ReferenceCheckout
        },
        "the shared hireling checkout is reference only"
    );
}

#[test]
fn decide_skips_unknown_name() {
    assert_eq!(
        decide("mor28-workspace-archive", true, true, false),
        ReapDecision::Skip {
            reason: SkipReason::NotReapable
        },
        "a name outside the reaper's set is ignored"
    );
}

#[test]
fn is_cargo_like_matches_build_processes() {
    assert!(
        is_cargo_like("cargo run --package diskops -- reap"),
        "a cargo invocation is a build"
    );
    assert!(
        is_cargo_like("/home/flinn/.cargo/bin/rustc --edition 2024"),
        "a full-path rustc invocation is a build"
    );
    assert!(
        !is_cargo_like("just reap"),
        "the reaper's own just wrapper is not a build"
    );
}

#[test]
fn parse_df_reads_available_and_mount() {
    let output = "Filesystem         1-blocks        Used   Available Capacity Mounted on\n\
                  /dev/vda1     123746762752 82080034816 41649950720      67% /\n";
    let free = parse_df(output).expect("a real df data row parses");
    assert_eq!(
        free,
        FreeSpace {
            available: 41_649_950_720,
            mount: "/".to_owned()
        },
        "the Available column and mount point are the 4th and 6th fields"
    );
}

#[test]
fn parse_df_reads_mount_with_spaces() {
    let output = "Filesystem 1-blocks Used Available Capacity Mounted on\n\
                  tmpfs       1024      0      1024       0% /run/user/1000\n";
    let free = parse_df(output).expect("a mount with a path parses");
    assert_eq!(free.mount, "/run/user/1000");
}

#[test]
fn parse_df_rejects_empty_output() {
    assert!(
        parse_df("").is_err(),
        "no data row means no free-space reading"
    );
}

#[test]
fn parse_df_rejects_non_numeric_available() {
    let output = "Filesystem 1-blocks Used Available Capacity Mounted on\n\
                  /dev/vda1       nope  nope      nope     67% /\n";
    assert!(
        parse_df(output).is_err(),
        "a non-numeric Available column must fail closed"
    );
}

#[test]
fn parse_floor_gib_accepts_integer() {
    assert_eq!(
        parse_floor_gib("10").expect("a plain integer parses"),
        10,
        "the floor is read in gibibytes"
    );
}

#[test]
fn parse_floor_gib_accepts_surrounding_whitespace() {
    assert_eq!(
        parse_floor_gib("  10  ").expect("whitespace trims"),
        10,
        "leading and trailing whitespace is not a floor"
    );
}

#[test]
fn parse_floor_gib_rejects_empty() {
    assert!(
        parse_floor_gib("").is_err(),
        "an empty floor cannot mean zero free space required"
    );
}

#[test]
fn parse_floor_gib_rejects_non_numeric() {
    assert!(
        parse_floor_gib("ten").is_err(),
        "a word is not a byte count"
    );
}

#[test]
fn format_gib_rounds_to_one_decimal() {
    assert_eq!(
        format_gib(41_649_950_720),
        "38.8G",
        "38.789 GiB rounds to 38.8G"
    );
    assert_eq!(format_gib(429_496_729), "0.4G", "0.3999 GiB rounds to 0.4G");
    assert_eq!(format_gib(GIB), "1.0G", "one gibibyte is 1.0G");
}
