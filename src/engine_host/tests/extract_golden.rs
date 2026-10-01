//! Task 5 golden (spec FR-4): the reference export (`#pbExport`, Lorum
//! Ipsum, level 3) extracts to a `BaseStats` whose every derived base
//! equals the prototype's rendered numbers. The prototype is display
//! truth — a mismatch here adjusts the math table in `extract.rs`, never
//! a value in this file. `level_adjust = 0` pins prototype parity; ±1
//! asserts the design table directly (the prototype has no adjust render:
//! untrained adds neither level nor bonus, trained adds both).

use crate::engine_host::extract::extract;
use crate::pbimport::{model, transform};
use hireling_engine::model::BaseStats;

/// The prototype's own export, verbatim.
const REFERENCE: &str = include_str!("../../../tests/data/pb_export_reference.json");

/// A minimal valid export (required shape only) for degraded-section and
/// weapon-keying units.
const MINIMAL: &str = r#"{"success":true,"build":{"name":"Two Staves","level":2,"abilities":{"str":10,"dex":10,"con":10,"int":10,"wis":10,"cha":10},"proficiencies":{},"weapons":[{"name":"Staff","die":"d6","attack":2,"damageBonus":0},{"name":"Staff","die":"d8","attack":3,"damageBonus":1}]}}"#;

/// Parse + transform the reference export (the E5 pipeline, unchanged).
/// This module is `#[cfg(test)]`: clippy.toml already exempts
/// expect/unwrap/panic here — no suppression needed (one would be an
/// unfulfilled expectation).
fn reference_sheet() -> transform::BaseSheet {
    let export = model::parse_and_validate(REFERENCE).expect("reference export is valid");
    let (sheet, skips) = transform::transform(&export);
    assert!(
        !skips.sections.iter().any(|section| section == "attributes"),
        "the reference export must carry attributes verbatim"
    );
    sheet
}

fn minimal_sheet() -> transform::BaseSheet {
    let export = model::parse_and_validate(MINIMAL).expect("minimal export is valid");
    transform::transform(&export).0
}

fn skill_total(base: &BaseStats, name: &str) -> i32 {
    base.stats
        .skills
        .iter()
        .find(|skill| skill.name == name)
        .map_or_else(
            || panic!("skill {name} missing from extraction"),
            |skill| skill.total,
        )
}

// -- the golden: every derived base equals the prototype's rendered value --

/// Prototype parity at `level_adjust = 0`: globals, saves, class DC.
#[test]
fn golden_globals_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.schema, hireling_engine::model::BASE_SCHEMA);
    assert_eq!(base.level, 3);
    assert_eq!(base.stats.ac, 16, "acTotal verbatim");
    assert_eq!(base.stats.speed, 25, "speed + speedBonus");
    assert_eq!(base.stats.fort, 9, "3·1 + con 2 + trained 4");
    assert_eq!(base.stats.reflex, 8, "3·1 + dex 1 + trained 4");
    assert_eq!(base.stats.will, 11, "3·1 + wis 0 + expert 8");
    assert_eq!(base.stats.perception, 7, "3·1 + wis 0 + trained 4");
    assert_eq!(
        base.stats.class_dc,
        Some(21),
        "10 + 3·1 + key int 4 + trained 4"
    );
}

/// Prototype parity: skills (core + lores), 18 core entries in vocabulary
/// order plus the sheet's lores.
#[test]
fn golden_skills_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.stats.skills.len(), 20, "18 core + 2 lores");
    assert_eq!(skill_total(&base, "acrobatics"), 1, "untrained: dex only");
    assert_eq!(skill_total(&base, "arcana"), 11, "3·1 + int 4 + trained 4");
    assert_eq!(skill_total(&base, "athletics"), -1, "untrained: str only");
    assert_eq!(
        skill_total(&base, "deception"),
        14,
        "3·1 + cha 3 + expert 8"
    );
    assert_eq!(
        skill_total(&base, "diplomacy"),
        10,
        "3·1 + cha 3 + trained 4"
    );
    assert_eq!(skill_total(&base, "stealth"), 8);
    assert_eq!(skill_total(&base, "thievery"), 8);
    assert_eq!(skill_total(&base, "computers"), 4, "untrained: int only");
    assert_eq!(skill_total(&base, "piloting"), 1, "untrained: dex only");
    assert_eq!(
        skill_total(&base, "lore:underworld"),
        11,
        "3·1 + int 4 + trained 4 (rank 2)"
    );
    assert_eq!(
        skill_total(&base, "lore:mror_holds_history"),
        15,
        "3·1 + int 4 + expert 8 (rank 4); spaces underscored, lowercase"
    );
}

/// Prototype parity: strikes verbatim and caster blocks derived.
#[test]
fn golden_strikes_and_casters_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.stats.strikes.len(), 1);
    let staff = base.stats.strikes.first().expect("one strike");
    assert_eq!(staff.key, "Staff");
    assert_eq!(staff.label, "Staff");
    assert_eq!(staff.attack, 4, "weapons[].attack verbatim");
    assert_eq!(
        staff.damage, "1d4-1",
        "die + damageBonus as the roll string"
    );
    assert_eq!(staff.damage_flat, -1, "damageBonus is the flat part");

    assert_eq!(base.stats.casters.len(), 2);
    let wizard = base.stats.casters.first().expect("wizard");
    assert_eq!(wizard.caster_key, "Wizard");
    assert_eq!(wizard.spell_attack, 11, "3·1 + int 4 + trained 4");
    assert_eq!(wizard.spell_dc, 21, "spell_attack + 10");
    let gnome = base.stats.casters.get(1).expect("gnome");
    assert_eq!(gnome.caster_key, "Wellspring Gnome");
    assert_eq!(gnome.spell_attack, 10, "3·1 + cha 3 + trained 4 (innate)");
    assert_eq!(gnome.spell_dc, 20);
}

// -- level_adjust ±1 asserts the design table (not the prototype) --

/// Trained things rise with `eff_level`; untrained skills and verbatim
/// totals (ac, speed, strikes) do not move.
#[test]
fn level_adjust_plus_one_adds_level_to_trained_only() {
    let base = extract(&reference_sheet(), 1);
    assert_eq!(base.level, 4);
    assert_eq!(base.stats.fort, 10);
    assert_eq!(base.stats.will, 12);
    assert_eq!(base.stats.class_dc, Some(22));
    assert_eq!(skill_total(&base, "arcana"), 12);
    assert_eq!(skill_total(&base, "lore:underworld"), 12);
    assert_eq!(skill_total(&base, "acrobatics"), 1, "untrained: no level");
    assert_eq!(base.stats.ac, 16, "verbatim total: no level");
    assert_eq!(base.stats.strikes.first().expect("staff").attack, 4);
}

#[test]
fn level_adjust_minus_one_subtracts_level_from_trained_only() {
    let base = extract(&reference_sheet(), -1);
    assert_eq!(base.level, 2);
    assert_eq!(base.stats.fort, 8);
    assert_eq!(base.stats.class_dc, Some(20));
    assert_eq!(skill_total(&base, "lore:mror_holds_history"), 14);
    assert_eq!(skill_total(&base, "medicine"), 0, "untrained: no level");
}

// -- degraded sections and keying rules --

/// A sheet without `attributes` degrades to speed 0 — never a panic,
/// never an invention of a plausible value.
#[test]
fn missing_attributes_degrade_speed_to_zero() {
    let base = extract(&minimal_sheet(), 0);
    assert_eq!(base.stats.speed, 0);
    assert_eq!(base.stats.class_dc, None, "rank 0: null, nothing invented");
}

/// Duplicate weapon names: the first keeps the bare name, later ones get
/// `name#2` by array order — stable strike keys (E5's FR-10 rule).
#[test]
fn duplicate_weapon_names_get_stable_suffixes() {
    let base = extract(&minimal_sheet(), 0);
    let keys: Vec<&str> = base
        .stats
        .strikes
        .iter()
        .map(|strike| strike.key.as_str())
        .collect();
    assert_eq!(keys, vec!["Staff", "Staff#2"]);
    let second = base.stats.strikes.get(1).expect("second strike");
    assert_eq!(second.attack, 3);
    assert_eq!(second.damage, "1d8+1");
    assert_eq!(second.damage_flat, 1);
}
