//! D2's one-source invariant: the vocabulary STRING tables the seed and the
//! write path consume are pinned to the engine's ENUMS. A const that drifts
//! from its enum is a second vocabulary — the exact thing D2 forbids.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live at crate root by cargo convention"
)]

use hireling_engine::vocab::{
    BLANKET_STATS, Blanket, MODIFIER_TYPES, ModifierType, SINGLE_STATS, SingleStat,
};

#[test]
fn blanket_stats_are_the_blanket_enums_wire_text() {
    let from_enum: Vec<&str> = [
        Blanket::AllChecks,
        Blanket::AllDcs,
        Blanket::AllChecksAndDcs,
    ]
    .iter()
    .map(|blanket| blanket.as_str())
    .collect();
    assert_eq!(BLANKET_STATS.to_vec(), from_enum);
    for text in BLANKET_STATS {
        assert!(
            Blanket::parse(text).is_some(),
            "`{text}` must parse to its enum"
        );
    }
}

#[test]
fn modifier_types_are_the_type_enums_wire_text() {
    let from_enum: Vec<&str> = [
        ModifierType::Circumstance,
        ModifierType::Status,
        ModifierType::Item,
        ModifierType::Untyped,
    ]
    .iter()
    .map(|modifier_type| modifier_type.as_str())
    .collect();
    assert_eq!(MODIFIER_TYPES.to_vec(), from_enum);
    for text in MODIFIER_TYPES {
        assert!(
            ModifierType::parse(text).is_some(),
            "`{text}` must parse to its enum"
        );
    }
}

#[test]
fn single_stats_are_the_single_enums_wire_text() {
    let from_enum: Vec<&str> = SingleStat::ALL.iter().map(|stat| stat.as_str()).collect();
    assert_eq!(SINGLE_STATS.to_vec(), from_enum);
}
