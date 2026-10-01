//! Task 4: `EngineOutput` assembly (spec FR-4) — the pure per-character
//! computation over [`hireling_engine::compute::compute`]: base slots +
//! expanded candidates in; every derived slot with applied AND suppressed
//! provenance, plus effect chips, out. Shapes pinned by
//! `specs/008-buff-effect-engine/contracts/engine-output.md`.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live at crate root by cargo convention"
)]

use std::collections::BTreeMap;

use hireling_engine::model::{
    ActiveEffect, BASE_SCHEMA, BaseStats, CasterBase, EngineOutput, Modifier, ModifierType,
    OUTPUT_SCHEMA, SkillBase, StrikeBase,
};

/// Character under computation.
const CHARACTER_ID: i64 = 7;
/// The display name the host resolves for the source character (id 3).
const SOURCE_NAME: &str = "Lorum Ipsum";

/// Two-strike, two-caster, two-skill sheet: every per-instance axis has two
/// members so fan-out (one modifier → every instance) is observable.
fn two_of_everything_base() -> BaseStats {
    BaseStats {
        schema: BASE_SCHEMA.to_owned(),
        level: 5,
        stats: hireling_engine::model::Stats {
            ac: 18,
            fort: 9,
            reflex: 7,
            will: 10,
            perception: 6,
            speed: 25,
            class_dc: None,
            strikes: vec![
                StrikeBase {
                    key: "dagger".to_owned(),
                    label: "Dagger".to_owned(),
                    attack: 11,
                    damage: "1d4+3".to_owned(),
                    damage_flat: 3,
                },
                StrikeBase {
                    key: "longbow".to_owned(),
                    label: "Longbow".to_owned(),
                    attack: 9,
                    damage: "1d8".to_owned(),
                    damage_flat: 0,
                },
            ],
            casters: vec![
                CasterBase {
                    caster_key: "Wizard".to_owned(),
                    spell_attack: 9,
                    spell_dc: 22,
                },
                CasterBase {
                    caster_key: "Cleric".to_owned(),
                    spell_attack: 7,
                    spell_dc: 20,
                },
            ],
            skills: vec![
                SkillBase {
                    name: "acrobatics".to_owned(),
                    total: 2,
                },
                SkillBase {
                    name: "lore:underworld".to_owned(),
                    total: 7,
                },
            ],
        },
    }
}

/// One-effect constructor.
fn effect(effect_id: i64, modifiers: Vec<Modifier>) -> ActiveEffect {
    ActiveEffect {
        effect_id,
        name: format!("effect {effect_id}"),
        source_character_id: 3,
        targets: vec![CHARACTER_ID],
        modifiers,
        duration_note: "10 rounds".to_owned(),
        active: true,
        version: 1042,
        tracked_manually: false,
    }
}

/// One-modifier constructor. A parse failure here is a fixture bug, not
/// runtime input — hence the expect.
#[expect(
    clippy::expect_used,
    reason = "fixture helper in a test crate: a fixture typo must fail loudly"
)]
fn modifier(modifier_type: ModifierType, stat: &str, value: i32) -> Modifier {
    Modifier {
        modifier_type,
        stat: hireling_engine::vocab::Stat::parse(stat)
            .expect("test stats are canonical vocabulary text")
            .stat_name(),
        value,
    }
}

/// The host's source-name table: character 3 renders as the sample name.
fn source_names() -> BTreeMap<i64, String> {
    BTreeMap::from([(3, SOURCE_NAME.to_owned())])
}

// ---------------------------------------------------------------------------
// Per-instance fan-out (Q1: per strike / per caster block)
// ---------------------------------------------------------------------------

/// An `attack` modifier lands on EVERY strike; a `spell_attack` modifier on
/// EVERY caster block — each instance keeps its own base and provenance
/// (contract: per-instance, Q1).
#[test]
fn per_strike_and_per_caster_instances_fan_out() {
    let base = two_of_everything_base();
    let effects = vec![
        effect(41, vec![modifier(ModifierType::Status, "attack", 1)]),
        effect(42, vec![modifier(ModifierType::Item, "spell_attack", 2)]),
    ];
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &effects, &source_names());

    assert_eq!(out.derived.strikes.len(), 2, "one output per strike");
    for strike in &out.derived.strikes {
        assert_eq!(
            strike.attack.applied.len(),
            1,
            "attack modifier on every strike"
        );
        assert_eq!(strike.attack.applied.first().expect("applied").value, 1);
    }
    assert_eq!(
        out.derived.strikes.first().expect("dagger").attack.total,
        12,
        "dagger 11 + 1"
    );
    assert_eq!(
        out.derived.strikes.get(1).expect("longbow").attack.total,
        10,
        "longbow 9 + 1"
    );

    assert_eq!(out.derived.casters.len(), 2, "one output per caster block");
    for caster in &out.derived.casters {
        assert_eq!(
            caster.spell_attack.applied.len(),
            1,
            "spell_attack modifier on every block"
        );
    }
    assert_eq!(
        out.derived
            .casters
            .first()
            .expect("wizard")
            .spell_attack
            .total,
        11,
        "wizard 9 + 2"
    );
    assert_eq!(
        out.derived
            .casters
            .get(1)
            .expect("cleric")
            .spell_attack
            .total,
        9,
        "cleric 7 + 2"
    );
}

// ---------------------------------------------------------------------------
// Null base (contract: nothing invented, provenance still accounted)
// ---------------------------------------------------------------------------

/// `class_dc: null` keeps null base AND total, while the modifier is still
/// listed in `applied` — SC-4 accounting without invention (contract rule).
#[test]
fn null_class_dc_keeps_null_but_accounts_for_modifiers() {
    let base = two_of_everything_base();
    let effects = vec![effect(
        43,
        vec![modifier(ModifierType::Item, "class_dc", 1)],
    )];
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &effects, &source_names());

    let class_dc = &out.derived.class_dc;
    assert_eq!(class_dc.base, None, "no base invented");
    assert_eq!(class_dc.total, None, "no total invented");
    assert_eq!(class_dc.applied.len(), 1, "the modifier is still accounted");
    assert_eq!(class_dc.applied.first().expect("applied").value, 1);
}

// ---------------------------------------------------------------------------
// Zero-value modifier at compute level
// ---------------------------------------------------------------------------

/// A zero-value modifier shows as applied +0 in the assembled output
/// (spec's edge case, now through the full assembly path).
#[test]
fn zero_value_modifier_shows_applied_plus_zero() {
    let base = two_of_everything_base();
    let effects = vec![effect(
        44,
        vec![modifier(ModifierType::Untyped, "speed", 0)],
    )];
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &effects, &source_names());

    assert_eq!(out.derived.speed.total, 25, "+0 changes nothing");
    assert_eq!(out.derived.speed.applied.len(), 1, "the +0 is visible");
    assert_eq!(out.derived.speed.applied.first().expect("applied").value, 0);
}

// ---------------------------------------------------------------------------
// Chips (contract §4)
// ---------------------------------------------------------------------------

/// Chips carry the host-resolved source display name and the corpus
/// `tracked_manually` flag; a display-only condition yields a chip but ZERO
/// numeric delta (spec US-3: badge, not math).
#[test]
fn chips_carry_tracked_manually_and_resolved_source_names() {
    let base = two_of_everything_base();
    let mut display_only = effect(45, vec![]);
    display_only.name = "condition tier".to_owned();
    display_only.tracked_manually = true;
    let out =
        hireling_engine::compute::compute(CHARACTER_ID, &base, &[display_only], &source_names());

    assert_eq!(out.effects.len(), 1, "the display-only condition is a chip");
    let chip = out.effects.first().expect("chip");
    assert_eq!(chip.source_name, SOURCE_NAME, "host-resolved display name");
    assert!(chip.tracked_manually, "the corpus flag rides the chip");
    // Zero numeric delta everywhere: only the three global slots checked
    // representatively — a display-only effect carries no modifiers at all.
    assert_eq!(out.derived.ac.total, base.stats.ac);
    assert_eq!(out.derived.speed.total, base.stats.speed);
    assert!(out.derived.strikes.iter().all(|strike| {
        strike.attack.applied.is_empty() && strike.damage_flat.applied.is_empty()
    }));
}

/// Chips are the active effects targeting THIS character only — ended or
/// foreign-targeted effects are not chips — and they are ordered by
/// `effect_id` regardless of input order (contract: deterministic order).
#[test]
fn chips_are_active_targeted_effects_sorted_by_id() {
    let base = two_of_everything_base();
    let mut ended = effect(46, vec![]);
    ended.active = false;
    let mut elsewhere = effect(47, vec![]);
    elsewhere.targets = vec![999];
    let effects = vec![effect(49, vec![]), ended, elsewhere, effect(48, vec![])];
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &effects, &source_names());

    let ids: Vec<i64> = out.effects.iter().map(|chip| chip.effect_id).collect();
    assert_eq!(ids, vec![48, 49], "active + targeted only, sorted by id");
}

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

/// No effects: every total equals its base, every provenance list is empty,
/// no chips — the sheet renders untouched (identity of the pipeline).
#[test]
fn empty_effects_output_is_bases_with_empty_lists() {
    let base = two_of_everything_base();
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &[], &source_names());

    assert_eq!(out.schema, OUTPUT_SCHEMA);
    assert_eq!(out.character_id, CHARACTER_ID);
    assert_eq!(out.effects, Vec::new());
    assert_eq!(out.derived.ac.total, base.stats.ac);
    assert!(out.derived.ac.applied.is_empty() && out.derived.ac.suppressed.is_empty());
    assert_eq!(out.derived.fort.total, base.stats.fort);
    assert_eq!(out.derived.reflex.total, base.stats.reflex);
    assert_eq!(out.derived.will.total, base.stats.will);
    assert_eq!(out.derived.perception.total, base.stats.perception);
    assert_eq!(out.derived.class_dc.base, None);
    assert!(!out.derived.strikes.is_empty());
    for (strike, base_strike) in out.derived.strikes.iter().zip(&base.stats.strikes) {
        assert_eq!(strike.key, base_strike.key);
        assert_eq!(strike.attack.total, base_strike.attack);
        assert_eq!(strike.damage_flat.total, base_strike.damage_flat);
    }
    for (skill, base_skill) in out.derived.skills.iter().zip(&base.stats.skills) {
        assert_eq!(skill.name, base_skill.name);
        assert_eq!(skill.total, base_skill.total);
    }
}

// ---------------------------------------------------------------------------
// Golden fixture round-trip (Task 4 done-when)
// ---------------------------------------------------------------------------

/// The committed golden fixture round-trips: compute → serialize matches the
/// file byte-for-byte, and parse → compare equals the computed value. The
/// fixture carries one blanket modifier so its expansion into per-slot
/// provenance is pinned in JSON (contract: consumers never see `all_*`).
#[test]
fn golden_engine_output_fixture_round_trips() {
    let base = two_of_everything_base();
    let effects = vec![
        // Bless-style: status +1 on attack and saves via blanket? No —
        // contract shows per-stat; use a frightened-style blanket penalty.
        effect(
            41,
            vec![modifier(ModifierType::Status, "all_checks_and_dcs", -1)],
        ),
        effect(42, vec![modifier(ModifierType::Item, "ac", 1)]),
        effect(43, vec![modifier(ModifierType::Status, "attack", 1)]),
    ];
    let out = hireling_engine::compute::compute(CHARACTER_ID, &base, &effects, &source_names());

    let serialized = serde_json::to_string_pretty(&out).expect("output is serializable");
    let fixture = include_str!("fixtures/golden_output.json");
    assert_eq!(
        serialized, fixture,
        "compute output diverged from the committed golden fixture"
    );
    let parsed: EngineOutput = serde_json::from_str(fixture).expect("fixture parses");
    assert_eq!(parsed, out, "fixture parses back to the same output");
}
