//! Unit tests for the tier seed and the closed stat vocabulary.

use super::*;
use crate::import::SEED_JSON;

fn entry_json(tier: &str, modifiers: &str) -> String {
    let entry = raw_entry(tier, modifiers);
    let mut file = String::from(r#"{"conditions": ["#);
    file.push_str(&entry);
    file.push_str("]}");
    file
}

fn raw_entry(tier: &str, modifiers: &str) -> String {
    format!(
        r#"{{
            "upstream_id": "abc123",
            "name": "Test Condition",
            "tier": "{tier}",
            "modifiers": {modifiers}
        }}"#
    )
}

#[test]
fn frightened_seed_loads_with_parameterized_mapping() {
    let seed = load_seed(SEED_JSON).expect("the checked-in seed must be valid");
    let frightened = seed.get("TBSHQspnbcqxsmjL").expect("seed maps frightened");
    assert_eq!(
        frightened.tier,
        Tier::EngineMath,
        "frightened is engine math"
    );
    assert_eq!(
        frightened.valued,
        Some(true),
        "seed expects frightened valued"
    );
    let modifiers = &frightened.modifiers;
    assert_eq!(modifiers.len(), 1, "one shared mapping — not one per value");
    let mapping = modifiers.first().expect("one mapping exists");
    assert_eq!(
        mapping.modifier_type, "status",
        "status penalty per the rules text"
    );
    assert_eq!(
        mapping.stat, "all_checks_and_dcs",
        "frightened's exact footprint"
    );
    assert_eq!(
        mapping.value_kind,
        ValueKind::ConditionValue,
        "frightened 1..4 share one parameterized mapping (FR-9)"
    );
    let concealed = seed
        .get("DmAIPqOBomZ7H95W")
        .expect("seed documents concealed");
    assert_eq!(
        concealed.tier,
        Tier::DisplayOnly,
        "concealed is tracked manually"
    );
    assert!(
        concealed.modifiers.is_empty(),
        "display-only conditions carry zero mappings"
    );
}

#[test]
fn engine_math_requires_mappings() {
    let err = load_seed(&entry_json("engine_math", "[]"))
        .expect_err("engine_math with no mapping is a seed bug");
    assert!(
        err.to_string().contains("no modifier mappings"),
        "error must state the all-or-nothing rule, got: {err}"
    );
}

#[test]
fn display_only_forbids_mappings() {
    let err = load_seed(&entry_json(
        "display_only",
        r#"[{"modifier_type":"status","stat":"ac","value_kind":"constant","value":-1}]"#,
    ))
    .expect_err("display_only with mappings is a seed bug");
    assert!(
        err.to_string().contains("all-or-nothing"),
        "error must state the all-or-nothing rule, got: {err}"
    );
}

#[test]
fn unknown_stat_fails_at_load() {
    let err = load_seed(&entry_json(
        "engine_math",
        r#"[{"modifier_type":"status","stat":"charisma_modifier","value_kind":"constant","value":-1}]"#,
    ))
    .expect_err("stats outside the vocabulary must fail the seed at load");
    assert!(
        err.to_string().contains("charisma_modifier"),
        "error must name the offending stat, got: {err}"
    );
}

#[test]
fn skill_stats_are_vocabulary() {
    assert!(
        validate_stat("skill:athletics").is_ok(),
        "skill:<name> is part of the closed vocabulary"
    );
    assert!(
        validate_stat("skill:").is_err(),
        "a bare `skill:` with no name must fail"
    );
}

#[test]
fn constant_mappings_need_a_value() {
    let err = load_seed(&entry_json(
        "engine_math",
        r#"[{"modifier_type":"status","stat":"ac","value_kind":"constant"}]"#,
    ))
    .expect_err("constant mappings must carry their value");
    assert!(
        err.to_string().contains("no value"),
        "error must name the missing value, got: {err}"
    );
}

#[test]
fn condition_value_mappings_must_not_carry_a_value() {
    let err = load_seed(&entry_json(
        "engine_math",
        r#"[{"modifier_type":"status","stat":"ac","value_kind":"condition_value","value":2}]"#,
    ))
    .expect_err("parameterized mappings take the value from the condition");
    assert!(
        err.to_string().contains("carries a value"),
        "error must explain the parameterization rule, got: {err}"
    );
}

#[test]
fn duplicate_upstream_ids_fail() {
    let entry = raw_entry("display_only", "[]").replace("abc123", "dup1");
    let doubled = format!(r#"{{"conditions": [{entry}, {entry}]}}"#);
    let err = load_seed(&doubled).expect_err("a condition maps exactly once");
    assert!(
        err.to_string().contains("duplicate upstream_id"),
        "error must name the duplication, got: {err}"
    );
}

#[test]
fn unknown_tier_fails() {
    let err = load_seed(&entry_json("vibes", "[]")).expect_err("unknown tiers are typos");
    assert!(
        err.to_string().contains("vibes"),
        "error must name the bad tier, got: {err}"
    );
}
