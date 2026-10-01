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

use hireling_engine::vocab::{
    Blanket, CORE_SKILLS, ModifierType, Stat, StatInstances, StatRef, expand,
};

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
    StatInstances {
        strikes: vec!["dagger".to_owned()],
        casters: vec!["Wizard".to_owned()],
        skills: vec![skill_name("acrobatics"), skill_name("lore:underworld")],
    }
}

/// A canonical skill name through the public parse path — tests only ever
/// construct names the vocabulary itself accepts.
fn skill_name(name: &str) -> hireling_engine::vocab::StatName {
    match Stat::parse(&format!("skill:{name}")) {
        Ok(Stat::Skill(parsed)) => parsed,
        other => panic!("`skill:{name}` must parse to a skill stat, got {other:?}"),
    }
}

#[test]
fn all_checks_expands_to_checks_never_damage_or_speed() {
    let expanded = expand(Blanket::AllChecks, &sample_instances());
    let names: Vec<&str> = expanded.iter().map(StatRef::as_str).collect();
    // Checks: attack per strike, spell_attack per caster, saves, perception,
    // every skill instance. NOT damage, NOT speed (design's expansion table).
    let expected = [
        "attack",       // the dagger
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
    let names: Vec<&str> = expanded.iter().map(StatRef::as_str).collect();
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
    let names: Vec<&str> = union.iter().map(StatRef::as_str).collect();
    let expected_names: Vec<&str> = expected.iter().map(StatRef::as_str).collect();
    assert_eq!(
        names, expected_names,
        "the union blanket expands to exactly the union of the two data tables"
    );
}

#[test]
fn expansion_is_a_function_of_the_sheets_instances() {
    // No strikes, no casters, two skills: the per-instance members vanish,
    // the globals and the skills stay. A blanket is expanded against THIS
    // sheet, not against a universal list (spec Q1/Q2).
    let instances = StatInstances {
        strikes: vec![],
        casters: vec![],
        skills: vec![skill_name("acrobatics")],
    };
    let expanded = expand(Blanket::AllChecks, &instances);
    let names: Vec<&str> = expanded.iter().map(StatRef::as_str).collect();
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
    for skill in CORE_SKILLS {
        let stat = format!("skill:{skill}");
        assert!(
            Stat::parse(&stat).is_ok(),
            "every core skill name must form a parseable `skill:` stat"
        );
    }
}

// ---------------------------------------------------------------------------
// Part 2: the worked examples — filled by plan Task 2 (stacking core).
// Each test asserts the Player Core outcome named in the spec's table.
// ---------------------------------------------------------------------------

// WEx-1 … WEx-12 land here, before stack.rs compiles green (Task 2).
