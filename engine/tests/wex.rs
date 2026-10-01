//! The engine's test suites.
//!
//! Part 1 (Task 1): the closed stat vocabulary — rejection of everything
//! outside it, and the blanket expansion sets as data (design's expansion
//! table, spec FR-2).
//!
//! Part 2 (Task 2): the twelve named worked examples WEx-1…12 — one
//! `#[test]` each, names verbatim from the spec's Stacking Math table, each
//! doc-comment citing the Player Core rule it pins. These are the
//! stop-the-line tripwire (Constitution Article IV).

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live at crate root by cargo convention"
)]

use hireling_engine::model::{ActiveEffect, BaseStats, Modifier, ModifierType};
use hireling_engine::stack::{StackedStat, candidates, stack_one};
use hireling_engine::vocab::{SingleStat, Stat, StatInstances, expand};

// ---------------------------------------------------------------------------
// Shared fixtures — the smallest sheet that shows every output slot class.
// ---------------------------------------------------------------------------

/// The sample sheet: one strike, one caster block, two skills. Bases chosen
/// so every total in the WEx assertions is visibly base + modifiers.
fn sample_base() -> BaseStats {
    BaseStats {
        schema: hireling_engine::model::BASE_SCHEMA.to_owned(),
        level: 3,
        stats: hireling_engine::model::Stats {
            ac: 16,
            fort: 7,
            reflex: 6,
            will: 7,
            perception: 5,
            speed: 25,
            class_dc: Some(19),
            strikes: vec![hireling_engine::model::StrikeBase {
                key: "Staff".to_owned(),
                label: "Staff".to_owned(),
                attack: 4,
                damage: "1d4-1".to_owned(),
                damage_flat: -1,
            }],
            casters: vec![hireling_engine::model::CasterBase {
                caster_key: "Wizard".to_owned(),
                spell_attack: 9,
                spell_dc: 19,
            }],
            skills: vec![
                hireling_engine::model::SkillBase {
                    name: "acrobatics".to_owned(),
                    total: 1,
                },
                hireling_engine::model::SkillBase {
                    name: "lore:underworld".to_owned(),
                    total: 9,
                },
            ],
        },
    }
}

fn instances_of(base: &BaseStats) -> StatInstances {
    base.stat_instances()
}

/// One effect with the given modifiers, targeting the sample character (id 7).
fn effect(effect_id: i64, modifiers: Vec<Modifier>) -> ActiveEffect {
    ActiveEffect {
        effect_id,
        name: format!("Effect {effect_id}"),
        source_character_id: 3,
        targets: vec![7],
        modifiers,
        duration_note: String::new(),
        active: true,
        version: 1,
        tracked_manually: false,
    }
}

fn modifier(modifier_type: ModifierType, stat: &str, value: i32) -> Modifier {
    Modifier {
        modifier_type,
        stat: Stat::parse(stat)
            .unwrap_or_else(|err| panic!("fixture stat `{stat}` must parse: {err}"))
            .stat_name(),
        value,
    }
}

/// Stack `stat_text` for the sample sheet under `effects`; the base value
/// comes from the sheet's matching slot.
fn stacked(
    base: &BaseStats,
    instances: &StatInstances,
    effects: &[ActiveEffect],
    stat_text: &str,
    base_value: i32,
) -> StackedStat {
    let all = candidates(effects, 7, instances);
    let mine: Vec<_> = all
        .iter()
        .filter(|candidate| candidate.stat_ref.as_str() == stat_text)
        .collect();
    stack_one(base_value, &mine)
}

/// Stack the single-stat global slot named by `stat` for character 7.
fn stacked_global(
    base: &BaseStats,
    instances: &StatInstances,
    effects: &[ActiveEffect],
    stat: SingleStat,
) -> StackedStat {
    let value = match stat {
        SingleStat::Ac => base.stats.ac,
        SingleStat::Fort => base.stats.fort,
        SingleStat::Ref => base.stats.reflex,
        SingleStat::Will => base.stats.will,
        SingleStat::Perception => base.stats.perception,
        SingleStat::Speed => base.stats.speed,
        SingleStat::ClassDc => base.stats.class_dc.unwrap_or(0),
        SingleStat::Attack | SingleStat::Damage => {
            base.stats.strikes.first().map_or(0, |strike| strike.attack)
        }
        SingleStat::SpellAttack | SingleStat::SpellDc => base
            .stats
            .casters
            .first()
            .map_or(0, |caster| caster.spell_attack),
    };
    stacked(base, instances, effects, stat.as_str(), value)
}

// ---------------------------------------------------------------------------
// Part 1: the closed vocabulary (FR-2)
// ---------------------------------------------------------------------------

#[test]
fn every_single_stat_string_parses_to_its_variant() {
    let cases = [
        "ac",
        "fort",
        "ref",
        "will",
        "perception",
        "speed",
        "attack",
        "damage",
        "spell_attack",
        "spell_dc",
        "class_dc",
    ];
    for text in cases {
        let stat = Stat::parse(text).unwrap_or_else(|err| panic!("`{text}` must parse: {err}"));
        assert!(
            matches!(&stat, Stat::Single(_)) && stat.as_str() == text,
            "`{text}` must parse to the single stat it names, got {stat:?}"
        );
    }
}

#[test]
fn every_blanket_string_parses_to_its_variant() {
    for text in ["all_checks", "all_dcs", "all_checks_and_dcs"] {
        let stat = Stat::parse(text).unwrap_or_else(|err| panic!("`{text}` must parse: {err}"));
        assert!(
            matches!(stat, Stat::Blanket(_)),
            "`{text}` must parse to a blanket target, got {stat:?}"
        );
    }
}

#[test]
fn unknown_stats_are_rejected_loudly() {
    let rejected = [
        "armor",
        "AC",
        "hitpoints",
        "all",
        "skill",
        "skill:",
        "skills:acrobatics",
        "lore:underworld",
        "",
        "ac ",
        " attacks",
    ];
    for text in rejected {
        let err = Stat::parse(text).expect_err("outside the closed vocabulary");
        assert!(
            !err.is_empty(),
            "rejection of `{text}` must carry a human-readable reason"
        );
    }
}

#[test]
fn skill_prefixed_stats_parse_core_and_lore_names() {
    let core = Stat::parse("skill:acrobatics").expect("core skill parses");
    assert!(
        matches!(&core, Stat::Skill(name) if name.as_str() == "skill:acrobatics"),
        "core skill parses with its full `skill:` wire text, got {core:?}"
    );
    let lore = Stat::parse("skill:lore:underworld").expect("lore skill parses");
    assert!(
        matches!(&lore, Stat::Skill(name) if name.as_str() == "skill:lore:underworld"),
        "lore skill keeps its `lore:` name under the `skill:` prefix, got {lore:?}"
    );
}

#[test]
fn bare_skill_prefix_and_unnormalized_names_are_rejected() {
    // NOTE: a well-formed lore name without its `lore:` prefix (e.g.
    // `skill:underworld`) is NOT rejectable at parse — lore names are
    // per-character data the vocabulary cannot enumerate. It parses, and
    // then matches no instance on a sheet that lacks that lore: no total
    // changes, nothing emitted, and the WRITE PATH warns at apply time
    // (spec edge case). The parser's loud rejection covers non-canonical
    // text and unknown non-skill stats.
    let rejected = [
        "skill:",           // bare prefix (plan Task 1)
        "skill:Acrobatics", // uppercase — typos must be loud, not silent no-ops
        "skill:mror holds", // spaces are not canonical stat text
        "skill:lore:",      // lore prefix without a name
    ];
    for text in rejected {
        assert!(
            Stat::parse(text).is_err(),
            "`{text}` must be rejected: the vocabulary is closed and names are canonical"
        );
    }
}

#[test]
fn stat_strings_round_trip() {
    let cases = [
        "ac",
        "all_checks_and_dcs",
        "skill:acrobatics",
        "skill:lore:underworld",
    ];
    for text in cases {
        let parsed = Stat::parse(text).expect("parses");
        assert_eq!(
            parsed.as_str(),
            text,
            "stat strings must round-trip — DB rows and wire frames carry them verbatim"
        );
    }
}

#[test]
fn modifier_types_are_the_closed_four() {
    for text in ["circumstance", "status", "item", "untyped"] {
        assert!(
            ModifierType::parse(text).is_some(),
            "`{text}` is one of the four modifier types"
        );
    }
    assert!(
        ModifierType::parse("proficiency").is_none(),
        "a fifth type is outside the vocabulary"
    );
}

// -- the expansion sets are data, exactly as the design tables them --------

/// A fixed three-skill, one-strike, one-caster instance set: enough to see
/// every expansion member class (per-strike, per-caster, per-skill, global).
fn sample_instances() -> StatInstances {
    instances_of(&sample_base())
}

#[test]
fn all_checks_expands_to_checks_never_damage_or_speed() {
    let expanded = expand(Blanket::AllChecks, &sample_instances());
    let names: Vec<&str> = expanded
        .iter()
        .map(hireling_engine::vocab::StatRef::as_str)
        .collect();
    // Checks: attack per strike, spell_attack per caster, saves, perception,
    // every skill instance. NOT damage, NOT speed (design's expansion table).
    let expected = [
        "attack",       // the staff
        "spell_attack", // Wizard
        "fort",
        "ref",
        "will",
        "perception",
        "skill:acrobatics",
        "skill:lore:underworld",
    ];
    assert_eq!(
        names, expected,
        "all_checks expansion must match the design table exactly, in order"
    );
}

#[test]
fn all_dcs_is_ac_class_dc_and_spell_dc_per_caster() {
    let expanded = expand(Blanket::AllDcs, &sample_instances());
    let names: Vec<&str> = expanded
        .iter()
        .map(hireling_engine::vocab::StatRef::as_str)
        .collect();
    assert_eq!(
        names,
        ["ac", "class_dc", "spell_dc"],
        "all_dcs = ac + class_dc + spell_dc (AC is a DC — Player Core)"
    );
}

#[test]
fn all_checks_and_dcs_is_the_exact_union() {
    let instances = sample_instances();
    let union = expand(Blanket::AllChecksAndDcs, &instances);
    let mut expected = expand(Blanket::AllChecks, &instances);
    expected.extend(expand(Blanket::AllDcs, &instances));
    let names: Vec<&str> = union
        .iter()
        .map(hireling_engine::vocab::StatRef::as_str)
        .collect();
    let expected_names: Vec<&str> = expected
        .iter()
        .map(hireling_engine::vocab::StatRef::as_str)
        .collect();
    assert_eq!(
        names, expected_names,
        "the union blanket expands to exactly the union of the two data tables"
    );
}

#[test]
fn expansion_is_a_function_of_the_sheets_instances() {
    // No strikes, no casters, one skill: the per-instance members vanish,
    // the globals and the skills stay. A blanket is expanded against THIS
    // sheet, not against a universal list (spec Q1/Q2).
    let instances = StatInstances {
        strikes: vec![],
        casters: vec![],
        skills: vec![Stat::parse("skill:acrobatics")]
            .into_iter()
            .flatten()
            .filter_map(|stat| match stat {
                Stat::Skill(name) => Some(name),
                _ => None,
            })
            .collect(),
    };
    let expanded = expand(Blanket::AllChecks, &instances);
    let names: Vec<&str> = expanded
        .iter()
        .map(hireling_engine::vocab::StatRef::as_str)
        .collect();
    assert_eq!(
        names,
        ["fort", "ref", "will", "perception", "skill:acrobatics"],
        "with no strikes or casters, all_checks carries only globals and this sheet's skills"
    );
}

#[test]
fn core_skills_are_the_exports_eighteen() {
    assert_eq!(
        CORE_SKILLS.len(),
        18,
        "the design pins CORE_SKILLS at the export's 18 skill keys"
    );
    for skill_name in CORE_SKILLS {
        let stat = format!("skill:{skill_name}");
        assert!(
            Stat::parse(&stat).is_ok(),
            "every core skill name must form a parseable `skill:` stat"
        );
    }
}

// ---------------------------------------------------------------------------
// Part 2: the worked examples (Task 2 — the stop-the-line tripwire).
// ---------------------------------------------------------------------------

use hireling_engine::vocab::{Blanket, CORE_SKILLS};

// -- WEx-1 ------------------------------------------------------------------
/// Player Core, Bonuses (general rule): "If you have multiple bonuses of the
/// same type, you can only benefit from the highest one" — a +2 status and a
/// +1 status to Armor Class yield +2; the +1 is suppressed.
#[test]
fn wex_1_same_type_bonuses_take_the_highest_once() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Status, "ac", 2)]),
        effect(2, vec![modifier(ModifierType::Status, "ac", 1)]),
    ];
    let ac = stacked_global(&base, &instances, &effects, SingleStat::Ac);
    assert_eq!(ac.total, 18, "16 base + highest status bonus only (+2)");
    assert_eq!(ac.applied.len(), 1, "exactly one status bonus applies");
    assert_eq!(ac.applied[0].effect_id, 1, "the +2 (lower effect id) wins");
    assert_eq!(
        ac.suppressed.len(),
        1,
        "the +1 is a first-class suppressed entry"
    );
    assert_eq!(
        ac.suppressed[0].reason,
        hireling_engine::model::SuppressionReason::SameTypeLowerBonus
    );
    assert_eq!(
        ac.suppressed[0].suppressed_by_effect_id,
        Some(1),
        "the suppressed entry names its winner"
    );
}

// -- WEx-2 ------------------------------------------------------------------
/// Player Core, Bonuses: bonuses of DIFFERENT types stack — a +1 status and
/// a +2 circumstance to AC both apply (+3 total).
#[test]
fn wex_2_different_type_bonuses_stack() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Status, "ac", 1)]),
        effect(2, vec![modifier(ModifierType::Circumstance, "ac", 2)]),
    ];
    let ac = stacked_global(&base, &instances, &effects, SingleStat::Ac);
    assert_eq!(ac.total, 19, "16 + 1 status + 2 circumstance");
    assert_eq!(ac.applied.len(), 2, "both bonuses apply");
    assert!(
        ac.suppressed.is_empty(),
        "different types never suppress each other"
    );
}

// -- WEx-3 ------------------------------------------------------------------
/// Player Core, Penalties (general rule): with multiple penalties of the
/// same type, "you take only the worst one" — −2 status and −1 status to
/// Will yield −2; the −1 is suppressed.
#[test]
fn wex_3_same_type_penalties_take_the_worst_once() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Status, "will", -2)]),
        effect(2, vec![modifier(ModifierType::Status, "will", -1)]),
    ];
    let will = stacked_global(&base, &instances, &effects, SingleStat::Will);
    assert_eq!(will.total, 5, "7 base + worst status penalty only (−2)");
    assert_eq!(will.applied.len(), 1, "exactly one status penalty applies");
    assert_eq!(
        will.suppressed.len(),
        1,
        "the −1 is suppressed, not stacked"
    );
    assert_eq!(
        will.suppressed[0].reason,
        hireling_engine::model::SuppressionReason::SameTypeLighterPenalty
    );
}

// -- WEx-4 ------------------------------------------------------------------
/// Player Core: bonuses and penalties of the same type resolve separately
/// and BOTH apply — +2 status and −1 status to attack give net +1, both
/// listed as applied.
#[test]
fn wex_4_bonus_and_penalty_of_same_type_both_apply() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Status, "attack", 2)]),
        effect(2, vec![modifier(ModifierType::Status, "attack", -1)]),
    ];
    let attack = stacked_global(&base, &instances, &effects, SingleStat::Attack);
    assert_eq!(attack.total, 5, "4 base + 2 status − 1 status = 5 (net +1)");
    assert_eq!(
        attack.applied.len(),
        2,
        "the best status bonus AND the worst status penalty both apply"
    );
    assert!(
        attack.suppressed.is_empty(),
        "one bonus + one penalty: nothing to suppress"
    );
}

// -- WEx-5 ------------------------------------------------------------------
/// Player Core, Frightened (condition): "You take a status penalty equal to
/// this value to all your checks and DCs." Frightened 2 lands −2 status on
/// EVERY `all_checks_and_dcs` member — and on nothing else: damage and
/// speed never move.
#[test]
fn wex_5_frightened_hits_all_checks_and_dcs_not_damage_or_speed() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![effect(
        1,
        vec![modifier(ModifierType::Status, "all_checks_and_dcs", -2)],
    )];

    for stat in [
        SingleStat::Ac,
        SingleStat::Fort,
        SingleStat::Ref,
        SingleStat::Will,
        SingleStat::Perception,
        SingleStat::ClassDc,
    ] {
        let stacked = stacked_global(&base, &instances, &effects, stat);
        assert_eq!(
            stacked.total,
            stacked.base - 2,
            "{} must drop by exactly 2 under frightened 2",
            stat.as_str()
        );
    }
    let spell_attack = stacked(&base, &instances, &effects, "spell_attack", 9);
    let spell_dc = stacked(&base, &instances, &effects, "spell_dc", 19);
    assert_eq!(spell_attack.total, 7, "spell attacks are checks");
    assert_eq!(spell_dc.total, 17, "DCs take the penalty too");
    let attack = stacked(&base, &instances, &effects, "attack", 4);
    assert_eq!(attack.total, 2, "attack rolls are checks");

    // The footprint boundary: damage and speed are NOT checks or DCs.
    let damage = stacked(&base, &instances, &effects, "damage", -1);
    assert_eq!(damage.total, -1, "damage never moves under frightened");
    assert!(damage.applied.is_empty(), "no modifier may land on damage");
    let speed = stacked(&base, &instances, &effects, "speed", 25);
    assert_eq!(speed.total, 25, "speed never moves under frightened");
    assert!(speed.applied.is_empty(), "no modifier may land on speed");
}

// -- WEx-6 ------------------------------------------------------------------
/// Player Core (the PRD's own example): Bless and Inspire Courage each grant
/// a +1 status bonus to attack. Same type — only +1 applies; the loser is
/// emitted suppressed, naming the winner.
#[test]
fn wex_6_bless_vs_inspire_courage_one_status_bonus_applies() {
    let base = sample_base();
    let instances = instances_of(&base);
    let mut bless = effect(1, vec![modifier(ModifierType::Status, "attack", 1)]);
    bless.name = "Bless".to_owned();
    let mut inspire = effect(2, vec![modifier(ModifierType::Status, "attack", 1)]);
    inspire.name = "Inspire Courage".to_owned();
    let effects = vec![bless, inspire];

    let attack = stacked_global(&base, &instances, &effects, SingleStat::Attack);
    assert_eq!(attack.total, 5, "4 base + 1 status (never +2)");
    assert_eq!(attack.applied.len(), 1, "one +1 status applies");
    assert_eq!(
        attack.applied[0].effect_id, 1,
        "deterministic winner: lower effect id"
    );
    assert_eq!(attack.suppressed.len(), 1, "the other +1 is suppressed");
    assert_eq!(attack.suppressed[0].effect_name, "Inspire Courage");
    assert_eq!(
        attack.suppressed[0].reason,
        hireling_engine::model::SuppressionReason::SameTypeTie,
        "equal values are a tie, broken by (effect_id, ord)"
    );
    assert_eq!(
        attack.suppressed[0].suppressed_by_effect_id,
        Some(1),
        "the tie loser names the stable winner"
    );
}

// -- WEx-7 ------------------------------------------------------------------
/// Player Core, Bonuses: untyped bonuses STACK — +1 untyped and +2 untyped
/// to damage both apply (+3).
#[test]
fn wex_7_untyped_bonuses_stack_fully() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Untyped, "damage", 1)]),
        effect(2, vec![modifier(ModifierType::Untyped, "damage", 2)]),
    ];
    let damage = stacked(&base, &instances, &effects, "damage", -1);
    assert_eq!(
        damage.total, 2,
        "−1 flat + 1 + 2 = +2 — every untyped applies"
    );
    assert_eq!(damage.applied.len(), 2, "both untyped bonuses are applied");
}

// -- WEx-8 ------------------------------------------------------------------
/// Player Core, Penalties: untyped penalties STACK — −1 untyped and −2
/// untyped to AC both apply (−3).
#[test]
fn wex_8_untyped_penalties_stack_fully() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Untyped, "ac", -1)]),
        effect(2, vec![modifier(ModifierType::Untyped, "ac", -2)]),
    ];
    let ac = stacked_global(&base, &instances, &effects, SingleStat::Ac);
    assert_eq!(ac.total, 13, "16 − 1 − 2 — every untyped penalty applies");
    assert_eq!(ac.applied.len(), 2, "both untyped penalties are applied");
    assert!(
        ac.suppressed.is_empty(),
        "untyped penalties never suppress each other"
    );
}

// -- WEx-9 ------------------------------------------------------------------
/// Player Core (blanket + specific, same type): a −2 status
/// `all_checks_and_dcs` and a direct −1 status `ac` are two status penalties
/// on AC after the blanket expands — the worst (−2) applies, the −1 is
/// suppressed. Expansion happens BEFORE stacking is evaluated.
#[test]
fn wex_9_blanket_expands_before_stacking_competes_per_stat() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(
            1,
            vec![modifier(ModifierType::Status, "all_checks_and_dcs", -2)],
        ),
        effect(2, vec![modifier(ModifierType::Status, "ac", -1)]),
    ];
    let ac = stacked_global(&base, &instances, &effects, SingleStat::Ac);
    assert_eq!(ac.total, 14, "16 − 2 (worst status penalty only)");
    assert_eq!(ac.applied.len(), 1);
    assert_eq!(ac.applied[0].effect_id, 1, "the blanket's −2 wins");
    assert_eq!(
        ac.suppressed.len(),
        1,
        "the direct −1 lost the same-type contest"
    );
    assert_eq!(
        ac.suppressed[0].reason,
        hireling_engine::model::SuppressionReason::SameTypeLighterPenalty
    );
    // ...and the blanket still covers the rest of its footprint.
    let fort = stacked_global(&base, &instances, &effects, SingleStat::Fort);
    assert_eq!(fort.total, 5, "7 − 2: the blanket member took its penalty");
}

// -- WEx-10 -----------------------------------------------------------------
/// Player Core, Bonuses (item): armor potency and another item bonus to AC —
/// the highest item bonus applies once; the +1 item is suppressed by the +2.
#[test]
fn wex_10_item_bonuses_highest_once() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![
        effect(1, vec![modifier(ModifierType::Item, "ac", 1)]),
        effect(2, vec![modifier(ModifierType::Item, "ac", 2)]),
    ];
    let ac = stacked_global(&base, &instances, &effects, SingleStat::Ac);
    assert_eq!(ac.total, 18, "16 + 2 item (highest once)");
    assert_eq!(ac.applied[0].effect_id, 2, "the +2 item bonus applies");
    assert_eq!(ac.suppressed.len(), 1);
    assert_eq!(
        ac.suppressed[0].reason,
        hireling_engine::model::SuppressionReason::SameTypeLowerBonus
    );
}

// -- WEx-11 -----------------------------------------------------------------
/// Player Core: AC is a DC. An `all_dcs` modifier moves `ac`, `class_dc`,
/// and `spell_dc` — and NOTHING else (not the saves, not attacks, not
/// skills).
#[test]
fn wex_11_ac_is_a_dc_and_all_dcs_moves_exactly_the_dc_set() {
    let base = sample_base();
    let instances = instances_of(&base);
    let effects = vec![effect(
        1,
        vec![modifier(ModifierType::Circumstance, "all_dcs", 1)],
    )];

    for dc_stat in [SingleStat::Ac, SingleStat::ClassDc] {
        let stacked = stacked_global(&base, &instances, &effects, dc_stat);
        assert_eq!(
            stacked.total,
            stacked.base + 1,
            "{} is a DC and must rise by 1",
            dc_stat.as_str()
        );
    }
    let spell_dc = stacked(&base, &instances, &effects, "spell_dc", 19);
    assert_eq!(spell_dc.total, 20, "spell_dc is a DC and rises");

    // Nothing outside the DC set moves.
    for check_stat in [
        SingleStat::Fort,
        SingleStat::Ref,
        SingleStat::Will,
        SingleStat::Perception,
    ] {
        let stacked = stacked_global(&base, &instances, &effects, check_stat);
        assert_eq!(
            stacked.total,
            stacked.base,
            "{} is a check, not a DC — all_dcs must not touch it",
            check_stat.as_str()
        );
    }
    let attack = stacked(&base, &instances, &effects, "attack", 4);
    assert_eq!(attack.total, 4, "attacks are not DCs");
    let acrobatics = stacked(&base, &instances, &effects, "skill:acrobatics", 1);
    assert_eq!(acrobatics.total, 1, "skills are not DCs");
}

// -- WEx-12 -----------------------------------------------------------------
/// Player Core + E4 seed (`polarity`): the sign of a valued condition is
/// STORED DATA. Frightened's mapping is polarity `negative`, so frightened
/// 1..4 resolve to −1..−4 status penalties — never bonuses, whatever the
/// applied value.
#[test]
fn wex_12_valued_condition_sign_is_stored_polarity_data() {
    let base = sample_base();
    let instances = instances_of(&base);
    // The write path (engine_host apply) resolves value × polarity and
    // stores the signed result; the engine sees −N status. Frightened 1..4
    // therefore lands as −1..−4 on every all_checks_and_dcs member.
    for value in 1..=4 {
        let effects = vec![effect(
            1,
            vec![modifier(ModifierType::Status, "all_checks_and_dcs", -value)],
        )];
        let fort = stacked_global(&base, &instances, &effects, SingleStat::Fort);
        assert_eq!(
            fort.total,
            base.stats.fort - value,
            "frightened {value} must be −{value} (stored negative polarity), never a bonus"
        );
        assert_eq!(fort.applied[0].value, -value);
        assert!(
            fort.applied[0].value < 0,
            "the sign is data: always a penalty here"
        );
    }
}
