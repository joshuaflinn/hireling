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

use std::path::{Path, PathBuf};

use super::{
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, ProcessObservation, ReapDecision, SiblingKind,
    SkipReason, argv_names_path, below_floor, classify_sibling, decide, exhausted_walk_anchor,
    format_gib, guard_outcome, is_live_build_process, nearest_existing_ancestor, parse_df,
    parse_floor_gib, path_is_within, process_blocks_dir, skip_reason_text,
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
fn guard_outcome_refuses_below_floor() {
    let outcome = guard_outcome(FLOOR_BYTES - 1, FLOOR_BYTES, DEFAULT_FLOOR_GIB, "/");
    assert!(
        matches!(&outcome, GuardOutcome::Refuse { message }
            if message.contains("9.9G free")
                && message.contains("need 10G")
                && message.contains("run 'just reap'")
                && message.contains("just disk-floor-gb=")),
        "one byte below the floor must refuse, reporting free space below the floor and naming both remedies (just reap, just disk-floor-gb): {outcome:?}"
    );
}

#[test]
fn guard_outcome_proceeds_at_floor() {
    let outcome = guard_outcome(FLOOR_BYTES, FLOOR_BYTES, DEFAULT_FLOOR_GIB, "/");
    assert!(
        matches!(&outcome, GuardOutcome::Proceed { message }
            if message.starts_with("disk-guard ok:") && message.contains("floor 10G")),
        "exactly at the floor must pass: {outcome:?}"
    );
}

#[test]
fn exhausted_walk_anchor_keeps_relative_paths_on_the_cwd() {
    assert_eq!(
        exhausted_walk_anchor(false),
        ".",
        "a relative CARGO_TARGET_DIR walks to the cwd's filesystem, not the root"
    );
    assert_eq!(
        exhausted_walk_anchor(true),
        "/",
        "an absolute path that exhausts (defensive — the root exists) anchors at /"
    );
}

#[test]
fn nearest_existing_ancestor_anchors_exhausted_relative_walks_on_the_cwd() {
    let nothing = &|_: &Path| false;
    assert_eq!(
        nearest_existing_ancestor(Path::new("target-gate-123"), nothing),
        PathBuf::from("."),
        "a single-component relative target dir walks to the cwd's filesystem, not the root"
    );
    assert_eq!(
        nearest_existing_ancestor(Path::new("a/b/c"), nothing),
        PathBuf::from("."),
        "a multi-component relative path that does not exist walks to the cwd"
    );
    assert_eq!(
        nearest_existing_ancestor(Path::new("/nope/deeper"), nothing),
        PathBuf::from("/"),
        "an absolute path that exhausts anchors at the root (defensive)"
    );
}

#[test]
fn nearest_existing_ancestor_stops_at_the_first_existing_ancestor() {
    let exists = &|p: &Path| p == Path::new("a");
    assert_eq!(
        nearest_existing_ancestor(Path::new("a/b/c"), exists),
        PathBuf::from("a"),
        "the walk stops at the first ancestor the predicate says exists"
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
fn format_gib_truncates_to_one_decimal() {
    assert_eq!(
        format_gib(41_649_950_720),
        "38.7G",
        "38.789 GiB truncates to 38.7G, never rounding up"
    );
    assert_eq!(
        format_gib(429_496_729),
        "0.3G",
        "0.3999 GiB truncates to 0.3G"
    );
    assert_eq!(format_gib(GIB), "1.0G", "one gibibyte is 1.0G");
}

#[test]
fn classify_sibling_names_the_reapable_and_excluded_sets() {
    assert_eq!(
        classify_sibling("hireling"),
        SiblingKind::Reference,
        "the shared checkout is the reference, never reaped"
    );
    assert_eq!(
        classify_sibling("MOR-482"),
        SiblingKind::Mor,
        "a harness workspace is MOR-owned"
    );
    assert_eq!(
        classify_sibling("hireling-299-reap"),
        SiblingKind::Hireling,
        "a hand-cut worktree is a hireling sibling"
    );
    assert_eq!(
        classify_sibling("target-gate-42"),
        SiblingKind::Target,
        "a leaked external CARGO_TARGET_DIR is a target sibling"
    );
    assert_eq!(
        classify_sibling("docs"),
        SiblingKind::Other,
        "an unrelated directory is not reapable"
    );
    assert_eq!(
        classify_sibling("target"),
        SiblingKind::Other,
        "a bare 'target' is not a target-* sibling — the in-tree target dir is never at the sweep parent"
    );
}

#[test]
fn decide_skips_mor_regardless_of_liveness() {
    assert_eq!(
        decide("MOR-482", true, true, true),
        ReapDecision::Skip {
            reason: SkipReason::HarnessOwned
        },
        "a harness workspace is skipped even when a build is writing inside it (AC4)"
    );
}

#[test]
fn decide_skips_the_shared_checkout_regardless_of_liveness() {
    assert_eq!(
        decide("hireling", false, false, true),
        ReapDecision::Skip {
            reason: SkipReason::ReferenceCheckout
        },
        "the shared hireling checkout is skipped even when a build is writing inside it (AC4)"
    );
}

#[test]
fn decide_skips_a_dirty_worktree() {
    assert_eq!(
        decide("hireling-old", true, false, false),
        ReapDecision::Skip {
            reason: SkipReason::DirtyTree
        },
        "a clean-ancestor but dirty worktree is skipped (AC4)"
    );
}

#[test]
fn decide_skips_an_unmerged_worktree() {
    assert_eq!(
        decide("hireling-old", false, true, false),
        ReapDecision::Skip {
            reason: SkipReason::NotMerged
        },
        "a clean worktree whose branch is not an ancestor of origin/main is skipped"
    );
}

#[test]
fn decide_reaps_a_clean_merged_worktree() {
    assert_eq!(
        decide("hireling-old", true, true, false),
        ReapDecision::Reap,
        "a clean, merged, idle hand-cut worktree passes the staleness test and is reaped"
    );
}

#[test]
fn decide_reaps_an_idle_target() {
    assert_eq!(
        decide("target-gate-42", false, false, false),
        ReapDecision::Reap,
        "an idle target dir is reaped"
    );
}

#[test]
fn decide_skips_a_target_with_a_live_build() {
    assert_eq!(
        decide("target-gate-42", false, false, true),
        ReapDecision::Skip {
            reason: SkipReason::LiveBuild
        },
        "a target dir with a live build inside is skipped"
    );
}

#[test]
fn decide_skips_unknown_names() {
    assert_eq!(
        decide("docs", true, true, false),
        ReapDecision::Skip {
            reason: SkipReason::NotReapable
        },
        "an unrelated directory is never reaped"
    );
}

#[test]
fn skip_reason_text_covers_every_reason() {
    for reason in [
        SkipReason::HarnessOwned,
        SkipReason::ReferenceCheckout,
        SkipReason::LiveBuild,
        SkipReason::DirtyTree,
        SkipReason::NotMerged,
        SkipReason::NotReapable,
    ] {
        assert!(
            !skip_reason_text(reason).is_empty(),
            "every skip reason has operator-facing text: {reason:?}"
        );
    }
}

#[test]
fn is_live_build_process_matches_build_tokens_only() {
    assert!(
        is_live_build_process("cargo build --release"),
        "cargo is a build token"
    );
    assert!(
        is_live_build_process("node /path/to/vitest"),
        "node is a build token"
    );
    assert!(
        is_live_build_process("npm --prefix web ci"),
        "npm is a build token"
    );
    assert!(
        !is_live_build_process("git status"),
        "git is not a build token"
    );
}

#[test]
fn process_blocks_dir_attributes_inside_cwd_as_live() {
    let inside = ProcessObservation {
        cmdline: "node /home/flinn/.nvm/versions/node/v20/bin/vitest".to_owned(),
        argv: vec![
            "node".to_owned(),
            "/home/flinn/.nvm/versions/node/v20/bin/vitest".to_owned(),
        ],
        cwd: Some(PathBuf::from(
            "/home/flinn/projects/hireling-488-stash-web/web",
        )),
    };
    let candidate = Path::new("/home/flinn/projects/hireling-488-stash-web");
    assert!(
        process_blocks_dir(&inside, candidate),
        "a vitest whose cwd is inside the sibling blocks it (AC2 inside)"
    );
}

#[test]
fn process_blocks_dir_attributes_outside_cwd_as_idle() {
    let outside = ProcessObservation {
        cmdline: "/home/flinn/.paperclip/installs/npm/.../node_modules/@embedded-postgres/linux-x64/native/bin/postgres -D /home/flinn/.paperclip/instances/default/db".to_owned(),
        argv: vec![
            "/home/flinn/.paperclip/installs/npm/.../node_modules/@embedded-postgres/linux-x64/native/bin/postgres".to_owned(),
            "-D".to_owned(),
            "/home/flinn/.paperclip/instances/default/db".to_owned(),
        ],
        cwd: Some(PathBuf::from("/home/flinn/.paperclip/instances/default/db")),
    };
    let candidate = Path::new("/home/flinn/projects/hireling-488-stash-web");
    assert!(
        !process_blocks_dir(&outside, candidate),
        "embedded Postgres matches the npm/node tokens in its install path but its cwd is outside the sibling, so it does not block (AC2 outside)"
    );
}

#[test]
fn process_blocks_dir_blocks_when_cwd_is_unreadable() {
    let unreadable = ProcessObservation {
        cmdline: "cargo build".to_owned(),
        argv: vec!["cargo".to_owned(), "build".to_owned()],
        cwd: None,
    };
    assert!(
        process_blocks_dir(&unreadable, Path::new("/parent/target-gate-7")),
        "an unreadable cwd for a build-token process reads as live, never idle (fail-closed)"
    );
}

#[test]
fn process_blocks_dir_blocks_when_argv_names_the_candidate() {
    let building_elsewhere = ProcessObservation {
        cmdline: "cargo build --target-dir /parent/target-gate-42".to_owned(),
        argv: vec![
            "cargo".to_owned(),
            "build".to_owned(),
            "--target-dir".to_owned(),
            "/parent/target-gate-42".to_owned(),
        ],
        cwd: Some(PathBuf::from("/parent/hireling-gate-42")),
    };
    assert!(
        process_blocks_dir(&building_elsewhere, Path::new("/parent/target-gate-42")),
        "a cargo whose argv names the candidate blocks it even with a cwd elsewhere"
    );
    assert!(
        !process_blocks_dir(&building_elsewhere, Path::new("/parent/target-gate-99")),
        "the same cargo does not block a different, unrelated target dir"
    );
}

#[test]
fn process_blocks_dir_ignores_non_build_tokens_even_when_inside() {
    let git_inside = ProcessObservation {
        cmdline: "git status".to_owned(),
        argv: vec!["git".to_owned(), "status".to_owned()],
        cwd: Some(PathBuf::from("/parent/target-gate-7")),
    };
    assert!(
        !process_blocks_dir(&git_inside, Path::new("/parent/target-gate-7")),
        "a non-build process with its cwd inside the candidate does not block it"
    );
}

#[test]
fn path_is_within_is_component_wise() {
    assert!(
        path_is_within(Path::new("/a/b/c"), Path::new("/a/b")),
        "a descendant is within its ancestor"
    );
    assert!(
        path_is_within(Path::new("/a/b"), Path::new("/a/b")),
        "a path is within itself"
    );
    assert!(
        !path_is_within(Path::new("/a/bc"), Path::new("/a/b")),
        "/a/bc is not within /a/b — component boundary, not string prefix"
    );
    assert!(
        !path_is_within(Path::new("/a"), Path::new("/a/b")),
        "an ancestor is not within its descendant"
    );
}

#[test]
fn argv_names_path_requires_absolute_paths() {
    let argv = vec![
        "cargo".to_owned(),
        "--target-dir".to_owned(),
        "/parent/target-gate-42".to_owned(),
    ];
    assert!(
        argv_names_path(&argv, Path::new("/parent/target-gate-42")),
        "an absolute argv path under the candidate names it"
    );
    assert!(
        !argv_names_path(&argv, Path::new("/parent/target-gate-99")),
        "an absolute argv path under a different dir does not name the candidate"
    );
    let relative = vec![
        "cargo".to_owned(),
        "--target-dir".to_owned(),
        "../target-gate-42".to_owned(),
    ];
    assert!(
        !argv_names_path(&relative, Path::new("/parent/target-gate-42")),
        "a relative argv path is not resolved against the process cwd (documented residual)"
    );
}
