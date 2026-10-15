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
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, below_floor, exhausted_walk_anchor,
    format_gib, guard_outcome, parse_df, parse_floor_gib,
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
            if message.contains("need 10G") && message.contains("just disk-floor-gb=")),
        "one byte below the floor must refuse, naming the floor and the in-tree remedy: {outcome:?}"
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
        "a relative CARGO_TARGET_DIR walks to the cwd's filesystem, not the root (round-2 F5)"
    );
    assert_eq!(
        exhausted_walk_anchor(true),
        "/",
        "an absolute path that exhausts (defensive — the root exists) anchors at /"
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
