//! Task 6 apply tests (FR-8, D7): corpus row → resolved signed modifiers.
//! The tier and the value expectations come from the STORED corpus rows
//! (the exact jsonb shape E4's transform writes), never from code — the
//! one condition name below is the worked-example citation WEx-12 already
//! carries (Player Core: frightened 2 is a −2 status penalty to all checks
//! and DCs). A corrupt row is loud at apply, never a silent one.

use serde_json::{Value, json};

use crate::engine_host::apply::{ApplyError, apply_condition};
use hireling_engine::model::Modifier;
use hireling_engine::vocab::{ModifierType, Stat};

/// The exact stored shape E4's transform writes for a `condition_value`
/// mapping with a negative polarity (frightened's seed row).
fn frightened_mapping() -> Value {
    json!([{
        "type": "status",
        "stat": "all_checks_and_dcs",
        "value": Value::Null,
        "value_kind": "condition_value",
        "polarity": "negative",
    }])
}

fn stat(text: &str) -> hireling_engine::vocab::StatName {
    Stat::parse(text)
        .expect("test stat is canonical")
        .stat_name()
}

#[test]
fn valued_condition_at_n_resolves_signed_modifiers() {
    // WEx-12: frightened 1..4 → −1..−4 status, never a bonus. The mapping
    // row is value_kind=condition_value × polarity=negative; apply at 2
    // stores −2, signed, exactly as the corpus states.
    let resolved = apply_condition(Some("engine_math"), Some(&frightened_mapping()), Some(2))
        .expect("frightened mapping resolves");
    assert!(!resolved.tracked_manually);
    assert_eq!(
        resolved.modifiers,
        vec![Modifier {
            modifier_type: ModifierType::Status,
            stat: stat("all_checks_and_dcs"),
            value: -2,
        }],
    );
    // The value rides in, not in the data: the same mapping at 4 → −4.
    let at_four = apply_condition(Some("engine_math"), Some(&frightened_mapping()), Some(4))
        .expect("frightened mapping resolves");
    assert_eq!(
        at_four.modifiers.first().map(|modifier| modifier.value),
        Some(-4)
    );
}

#[test]
fn positive_polarity_resolves_as_bonus() {
    let mapping = json!([{
        "type": "status",
        "stat": "attack",
        "value": Value::Null,
        "value_kind": "condition_value",
        "polarity": "positive",
    }]);
    let resolved = apply_condition(Some("engine_math"), Some(&mapping), Some(1))
        .expect("positive mapping resolves");
    assert_eq!(
        resolved.modifiers.first().map(|modifier| modifier.value),
        Some(1)
    );
}

#[test]
fn constant_mapping_carries_its_signed_value() {
    let mapping = json!([{
        "type": "circumstance",
        "stat": "ac",
        "value": 1,
        "value_kind": "constant",
    }]);
    let resolved = apply_condition(Some("engine_math"), Some(&mapping), None)
        .expect("constant mapping resolves");
    assert_eq!(
        resolved.modifiers,
        vec![Modifier {
            modifier_type: ModifierType::Circumstance,
            stat: stat("ac"),
            value: 1,
        }],
    );
    // A negative constant is stored negative — the sign is the data.
    let penalty = json!([{
        "type": "item",
        "stat": "speed",
        "value": -10,
        "value_kind": "constant",
    }]);
    let penalized =
        apply_condition(Some("engine_math"), Some(&penalty), Some(3)).expect("resolves");
    assert_eq!(
        penalized.modifiers.first().map(|modifier| modifier.value),
        Some(-10)
    );
}

#[test]
fn mappings_order_is_preserved() {
    // ord = array index on insert; stacking ties break by (effect_id, ord),
    // so apply must carry the stored order through untouched.
    let mapping = json!([
        {"type": "status", "stat": "all_checks_and_dcs", "value": Value::Null,
         "value_kind": "condition_value", "polarity": "negative"},
        {"type": "circumstance", "stat": "ac", "value": 2, "value_kind": "constant"},
    ]);
    let ordered = apply_condition(Some("engine_math"), Some(&mapping), Some(1))
        .expect("two mappings resolve");
    assert_eq!(
        ordered.modifiers,
        vec![
            Modifier {
                modifier_type: ModifierType::Status,
                stat: stat("all_checks_and_dcs"),
                value: -1,
            },
            Modifier {
                modifier_type: ModifierType::Circumstance,
                stat: stat("ac"),
                value: 2,
            },
        ],
        "stored array order is the modifier order"
    );
}

#[test]
fn display_only_tier_yields_badge_only() {
    // US-3: a display-only condition is a badge chip with tracked_manually
    // and ZERO math — the engine never fabricates modifiers for it.
    let resolved =
        apply_condition(Some("display_only"), None, None).expect("display-only resolves");
    assert!(resolved.tracked_manually);
    assert!(resolved.modifiers.is_empty());
    // Even with a condition value supplied: nothing to resolve it against.
    let with_value = apply_condition(Some("display_only"), None, Some(2))
        .expect("display-only ignores the value");
    assert!(with_value.tracked_manually);
    assert!(with_value.modifiers.is_empty());
}

#[test]
fn absent_or_empty_mappings_mean_display_only() {
    // FR-8: mappings absent/empty ⇒ display-only, zero math — the fail-safe
    // direction, whatever the tier label says.
    for modifiers in [None, Some(&json!([]))] {
        let fallback = apply_condition(Some("engine_math"), modifiers, Some(1))
            .expect("absent/empty mappings are display-only");
        assert!(fallback.tracked_manually, "mappings {modifiers:?}");
        assert!(fallback.modifiers.is_empty());
    }
}

#[test]
fn invalid_corpus_stat_is_loud_at_apply() {
    // A stat outside the closed vocabulary can never resolve silently:
    // loud error naming the offending stat, never a dropped row.
    let mapping = json!([{
        "type": "status",
        "stat": "initiative",
        "value": 1,
        "value_kind": "constant",
    }]);
    let err = apply_condition(Some("engine_math"), Some(&mapping), None)
        .expect_err("invalid stat is loud");
    assert!(err.problem.contains("initiative"), "{}", err.problem);
}

#[test]
fn unknown_modifier_type_is_loud() {
    let mapping = json!([{
        "type": "morale",
        "stat": "attack",
        "value": 1,
        "value_kind": "constant",
    }]);
    let err = apply_condition(Some("engine_math"), Some(&mapping), None)
        .expect_err("unknown type is loud");
    assert!(err.problem.contains("morale"), "{}", err.problem);
}

#[test]
fn corrupt_mapping_shape_is_loud() {
    // A parameterized row without its polarity cannot be resolved: guessing
    // a sign would be the engine lying (E4's resolve_mapping_value returns
    // None for exactly this). Also: an unknown value_kind.
    let signless = json!([{
        "type": "status",
        "stat": "all_checks_and_dcs",
        "value": Value::Null,
        "value_kind": "condition_value",
    }]);
    let err = apply_condition(Some("engine_math"), Some(&signless), Some(2))
        .expect_err("signless parameterized row is loud");
    assert!(
        err.problem.contains("all_checks_and_dcs"),
        "{}",
        err.problem
    );

    let alien_kind = json!([{
        "type": "status",
        "stat": "attack",
        "value": 1,
        "value_kind": "roll_twice",
    }]);
    assert!(apply_condition(Some("engine_math"), Some(&alien_kind), None).is_err());
}

#[test]
fn valued_mapping_without_a_value_is_loud() {
    // frightened applied at no value: there is nothing to scale — apply
    // never defaults the condition's value.
    let err = apply_condition(Some("engine_math"), Some(&frightened_mapping()), None)
        .expect_err("valued mapping without a value is loud");
    assert!(err.problem.contains("value"), "{}", err.problem);
}

#[test]
fn tier_label_disagreeing_with_mappings_is_loud() {
    // The importer's all-or-nothing rule makes these rows unstorable; a
    // hand-edited corpus row that disagrees is corrupt and stays loud.
    let constant = json!([{
        "type": "circumstance", "stat": "ac", "value": 1, "value_kind": "constant",
    }]);
    let mismatched = apply_condition(Some("display_only"), Some(&constant), None)
        .expect_err("display_only label with mappings is loud");
    assert!(
        mismatched.problem.contains("display_only"),
        "{}",
        mismatched.problem
    );

    let alien_tier = apply_condition(Some("tierless_garbage"), Some(&constant), None)
        .expect_err("unknown tier label with mappings is loud");
    assert!(
        alien_tier.problem.contains("tierless_garbage"),
        "{}",
        alien_tier.problem
    );
}

#[test]
fn null_tier_label_with_mappings_still_resolves() {
    // data.import.tier is advisory once mappings exist — the mappings are
    // the operative engine-math data (a legacy row without the stamp).
    let constant = json!([{
        "type": "circumstance", "stat": "ac", "value": 1, "value_kind": "constant",
    }]);
    let legacy = apply_condition(None, Some(&constant), None).expect("mappings are operative");
    assert!(!legacy.tracked_manually);
    assert_eq!(legacy.modifiers.len(), 1);
}

#[test]
fn error_is_displayable() {
    let err = ApplyError {
        problem: "test problem".to_owned(),
    };
    assert_eq!(err.to_string(), "condition apply: test problem");
}
