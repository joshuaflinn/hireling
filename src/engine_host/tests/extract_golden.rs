//! Task 5 golden (spec FR-4): the reference export (`#pbExport`, Lorum
//! Ipsum, level 3) extracts to a `BaseStats` whose every derived base
//! equals the prototype's rendered numbers. The prototype is display
//! truth — a mismatch here adjusts the math table in `extract.rs`, never
//! a value in this file. `level_adjust = 0` pins prototype parity; ±1
//! asserts the design table directly (the prototype has no adjust render:
//! untrained adds neither level nor bonus, trained adds both — including
//! re-derived strike attacks); +2 pins the wizard class progression
//! (display level 5 ≥ the reflex→expert threshold, the only one it
//! crosses).

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
/// Bonus = raw rank + level (`pb = rank > 0 ? rank + level : 0`):
/// trained(L3) = 2+3 = +5, expert(L3) = 4+3 = +7.
#[test]
fn golden_globals_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.schema, hireling_engine::model::BASE_SCHEMA);
    assert_eq!(base.level, 3);
    assert_eq!(
        base.stats.ac, 16,
        "AC re-derived from the worn armor's rank — agrees with the frozen acTotal exactly at the export's own level (E6 finding 8; the ±1 test pins the divergence)"
    );
    assert_eq!(base.stats.speed, 25, "speed + speedBonus");
    assert_eq!(base.stats.fort, 7, "con 2 + trained 2+3");
    assert_eq!(base.stats.reflex, 6, "dex 1 + trained 2+3");
    assert_eq!(base.stats.will, 7, "wis 0 + expert 4+3");
    assert_eq!(base.stats.perception, 5, "wis 0 + trained 2+3");
    assert_eq!(
        base.stats.class_dc,
        Some(19),
        "10 + key int 4 + trained 2+3"
    );
}

/// Prototype parity: skills (core + lores), 18 core entries in vocabulary
/// order plus the sheet's lores.
#[test]
fn golden_skills_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.stats.skills.len(), 20, "18 core + 2 lores");
    assert_eq!(skill_total(&base, "acrobatics"), 1, "untrained: dex only");
    assert_eq!(skill_total(&base, "arcana"), 9, "int 4 + trained 2+3");
    assert_eq!(skill_total(&base, "athletics"), -1, "untrained: str only");
    assert_eq!(skill_total(&base, "deception"), 10, "cha 3 + expert 4+3");
    assert_eq!(skill_total(&base, "diplomacy"), 8, "cha 3 + trained 2+3");
    assert_eq!(skill_total(&base, "stealth"), 6);
    assert_eq!(skill_total(&base, "thievery"), 6);
    assert_eq!(skill_total(&base, "computers"), 4, "untrained: int only");
    assert_eq!(skill_total(&base, "piloting"), 1, "untrained: dex only");
    assert_eq!(
        skill_total(&base, "lore:underworld"),
        9,
        "int 4 + trained 2+3 (rank 2)"
    );
    assert_eq!(
        skill_total(&base, "lore:mror_holds_history"),
        11,
        "int 4 + expert 4+3 (rank 4); spaces underscored, lowercase"
    );
}

/// `skills[].rank` rides verbatim (contract §3 — the rank letter and
/// untrained dimming render from it), and lore rows carry the export's
/// display name as `label`; core rows omit the field entirely.
#[test]
fn golden_skill_render_inputs_ride_the_row() {
    let base = extract(&reference_sheet(), 0);
    let row_of = |name: &str| {
        base.stats
            .skills
            .iter()
            .find(|skill| skill.name == name)
            .unwrap_or_else(|| panic!("skill {name} missing"))
    };
    assert_eq!(row_of("acrobatics").rank, 0, "untrained");
    assert_eq!(row_of("arcana").rank, 2, "trained");
    assert_eq!(row_of("deception").rank, 4, "expert");
    assert_eq!(row_of("acrobatics").label, None, "core rows omit the label");
    let underworld = row_of("lore:underworld");
    assert_eq!(underworld.rank, 2);
    assert_eq!(
        underworld.label.as_deref(),
        Some("Underworld"),
        "the export's display name, verbatim — the canonical key lost the case"
    );
    let mror = row_of("lore:mror_holds_history");
    assert_eq!(mror.rank, 4);
    assert_eq!(mror.label.as_deref(), Some("Mror Holds History"));
    // The rank survives compute into the wire shape (the contract's rule:
    // render input, not derivable from total).
    let output =
        hireling_engine::compute::compute(7, &base, &[], &std::collections::BTreeMap::new());
    let wire = output
        .derived
        .skills
        .iter()
        .find(|skill| skill.name == "lore:mror_holds_history")
        .expect("lore on the wire");
    assert_eq!(wire.rank, 4);
    assert_eq!(wire.label.as_deref(), Some("Mror Holds History"));
    let core = output
        .derived
        .skills
        .iter()
        .find(|skill| skill.name == "acrobatics")
        .expect("core on the wire");
    assert_eq!(core.rank, 0);
    assert_eq!(core.label, None);
}

/// Prototype parity: strikes re-derived (attack = ability + rank+level +
/// pot; damage flat = Str + mastery) with the unarmed Fist appended, and
/// caster blocks derived.
#[test]
fn golden_strikes_and_casters_match_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.stats.strikes.len(), 2, "the Staff + the unarmed Fist");
    let staff = base.stats.strikes.first().expect("staff strike");
    assert_eq!(staff.key, "Staff");
    assert_eq!(staff.label, "Staff");
    assert_eq!(
        staff.attack, 4,
        "str −1 + trained 2+3 + pot 0 — the export's verbatim 4 coincides"
    );
    assert_eq!(
        staff.damage, "d4−1",
        "the die VERBATIM + signed flat (str −1, U+2212) — the sheet's rendered string"
    );
    assert_eq!(staff.damage_flat, -1, "str + mastery(2, 3) = −1");
    assert_eq!(staff.map, 5, "not Agile: −5/−10");
    assert_eq!(staff.damage_type, "B");
    assert_eq!(staff.damage_type_name, "bludgeoning");
    assert_eq!(staff.traits, vec!["Monk", "Two-Hand d8"], "POC trait map");
    let fist = base.stats.strikes.get(1).expect("fist strike");
    assert_eq!(fist.key, "Fist");
    assert_eq!(fist.label, "Fist");
    assert_eq!(fist.attack, 6, "best(str,dex) = dex 1 + trained 2+3");
    assert_eq!(fist.damage, "d4−1", "d4 + str −1");
    assert_eq!(fist.damage_flat, -1);
    assert_eq!(fist.map, 4, "Agile: −4/−8");
    assert_eq!(
        fist.traits,
        vec!["Agile", "Finesse", "Nonlethal", "Unarmed"],
        "the fixed unarmed trait row"
    );

    assert_eq!(base.stats.casters.len(), 2);
    let wizard = base.stats.casters.first().expect("wizard");
    assert_eq!(wizard.caster_key, "Wizard");
    assert_eq!(wizard.spell_attack, 9, "int 4 + trained 2+3");
    assert_eq!(wizard.spell_dc, 19, "spell_attack + 10");
    assert!(!wizard.innate, "prepared block");
    let gnome = base.stats.casters.get(1).expect("gnome");
    assert_eq!(gnome.caster_key, "Wellspring Gnome");
    assert_eq!(gnome.spell_attack, 8, "cha 3 + trained 2+3 (innate)");
    assert_eq!(gnome.spell_dc, 18);
    assert!(gnome.innate, "the innate badge rides the block verbatim");
}

/// Prototype parity for the modifier-free render inputs (contract §3
/// `render_base`, design D3). `hp_max` is the PROTOTYPE's formula — CON and
/// the per-level bonus at every level — pinned at the same values
/// base.test.js pins for the swap's byte-identical check.
#[test]
fn golden_render_base_matches_the_prototype() {
    let base = extract(&reference_sheet(), 0);
    assert_eq!(base.render_base.level, 3);
    assert_eq!(
        base.render_base.hp_max, 32,
        "8 + (6 + CON 2) × 3 — CON counts at every level"
    );
    assert_eq!(base.render_base.focus_max, 1, "focusPoints verbatim");
    assert_eq!(base.render_base.hero_max, 3, "constant 3 at POC");
    assert_eq!(base.render_base.cantrip_rank, 2, "⌈3 / 2⌉");
    let attributes = base.render_base.attributes;
    assert_eq!(attributes.r#str, -1);
    assert_eq!(attributes.dex, 1);
    assert_eq!(attributes.con, 2);
    assert_eq!(attributes.int, 4);
    assert_eq!(attributes.wis, 0);
    assert_eq!(attributes.cha, 3);

    // The hp ceiling re-derives with eff_level; the clamp holds at 20.
    assert_eq!(extract(&reference_sheet(), 1).render_base.hp_max, 40);
    assert_eq!(extract(&reference_sheet(), -2).render_base.hp_max, 16);
    assert_eq!(extract(&reference_sheet(), -2).render_base.cantrip_rank, 1);
    assert_eq!(extract(&reference_sheet(), 2).render_base.cantrip_rank, 3);
    let at_twenty = extract(&reference_sheet(), 17);
    assert_eq!(at_twenty.render_base.level, 20, "clamped");
    assert_eq!(at_twenty.render_base.hp_max, 168, "8 + 8 × 20");
}

// -- level_adjust ±1 asserts the design table (not the prototype) --

/// Trained things rise with `eff_level` — including re-derived strike
/// attacks and AC (worn-armor rank); untrained skills and the verbatim
/// speed total do not move.
#[test]
fn level_adjust_plus_one_adds_level_to_trained_only() {
    let base = extract(&reference_sheet(), 1);
    assert_eq!(base.level, 4);
    assert_eq!(base.stats.fort, 8, "con 2 + trained 2+4");
    assert_eq!(base.stats.will, 8, "wis 0 + expert 4+4");
    assert_eq!(base.stats.class_dc, Some(20));
    assert_eq!(skill_total(&base, "arcana"), 10);
    assert_eq!(skill_total(&base, "lore:underworld"), 10);
    assert_eq!(skill_total(&base, "acrobatics"), 1, "untrained: no level");
    assert_eq!(
        base.stats.ac, 17,
        "AC RE-DERIVES from the worn armor's rank at eff_level (E6 finding 8): \
         10 + dex 1 + trained 2+4 — the frozen acTotal would have stayed 16"
    );
    assert_eq!(
        base.stats.strikes.first().expect("staff").attack,
        5,
        "re-derived: str −1 + trained 2+4 — rises with level, unlike the verbatim pin"
    );
}

#[test]
fn level_adjust_minus_one_subtracts_level_from_trained_only() {
    let base = extract(&reference_sheet(), -1);
    assert_eq!(base.level, 2);
    assert_eq!(base.stats.fort, 6, "con 2 + trained 2+2");
    assert_eq!(base.stats.class_dc, Some(18));
    assert_eq!(skill_total(&base, "lore:mror_holds_history"), 10);
    assert_eq!(skill_total(&base, "medicine"), 0, "untrained: no level");
}

/// Wizard class progression: at display level 5 (export 3, adjust +2) the
/// reflex bump to expert fires — the only threshold ≥5 crossed — while
/// fortitude (≥9) and perception (≥11) stay trained. Nothing below the
/// export's level bumps.
#[test]
fn wizard_progression_bumps_ranks_above_the_export_level() {
    let base = extract(&reference_sheet(), 2);
    assert_eq!(base.level, 5);
    assert_eq!(
        base.stats.reflex, 10,
        "dex 1 + expert 4+5 — bumped from trained by the ≥5 threshold"
    );
    assert_eq!(base.stats.fort, 9, "con 2 + trained 2+5 — no bump below ≥9");
    assert_eq!(
        base.stats.perception, 7,
        "wis 0 + trained 2+5 — no bump below ≥11"
    );
    assert_eq!(skill_total(&base, "acrobatics"), 1, "untrained never bumps");
}

/// The caster rank rule: a prepared caster takes the better of block and
/// tradition, so at display level 7 the castingArcane bump (≥7) lifts the
/// wizard's spell attack to expert — while the innate caster keeps her
/// own block rank (E6 pins the same numbers at feat/8 base.test.js).
#[test]
fn caster_rank_takes_the_tradition_bump_innate_keeps_hers() {
    let base = extract(&reference_sheet(), 4);
    assert_eq!(base.level, 7);
    let wizard = base.stats.casters.first().expect("wizard");
    assert_eq!(
        wizard.spell_attack, 15,
        "int 4 + expert 4+7 — max(block 2, tradition 4) after the ≥7 bump"
    );
    let gnome = base.stats.casters.get(1).expect("gnome");
    assert_eq!(
        gnome.spell_attack, 12,
        "cha 3 + trained 2+7 — innate: her own block rank, no tradition max"
    );
}

// -- degraded sections and keying rules --

/// `damageType` is READ from the export, not assumed (MOR-69 finding 2):
/// the reference corpus carries exactly one literal ("B"), which a
/// hardcoded constant would also pass. A second fixture carries "S" and
/// asserts the slashing arm; the missing-key default asserts "" — no
/// invention.
#[test]
fn damage_type_reads_the_export_not_a_constant() {
    let export = model::parse_and_validate(
        r#"{"success":true,"build":{"name":"Dagger Kit","level":2,"abilities":{"str":10,"dex":10,"con":10,"int":10,"wis":10,"cha":10},"proficiencies":{},"weapons":[{"name":"Dagger","die":"d4","attack":2,"damageBonus":0,"damageType":"S"}]}}"#,
    )
    .expect("dagger export is valid");
    let base = extract(&transform::transform(&export).0, 0);
    let dagger = base.stats.strikes.first().expect("dagger strike");
    assert_eq!(dagger.damage_type, "S", "the export's letter, verbatim");
    assert_eq!(dagger.damage_type_name, "slashing", "the S arm of the map");

    let minimal = extract(&minimal_sheet(), 0);
    let staff = minimal.stats.strikes.first().expect("staff strike");
    assert_eq!(
        staff.damage_type, "",
        "no damageType key: empty, not a guess"
    );
    assert_eq!(staff.damage_type_name, "");
}

/// A sheet without `attributes` degrades to speed 0 — never a panic,
/// never an invention of a plausible value.
#[test]
fn missing_attributes_degrade_speed_to_zero() {
    let base = extract(&minimal_sheet(), 0);
    assert_eq!(base.stats.speed, 0);
    assert_eq!(base.stats.class_dc, None, "rank 0: null, nothing invented");
}

/// Duplicate weapon names: the first keeps the bare name, later ones get
/// `name#2` by array order — stable strike keys (E5's FR-10 rule). The
/// appended unarmed Fist goes through the same counter.
#[test]
fn duplicate_weapon_names_get_stable_suffixes() {
    let base = extract(&minimal_sheet(), 0);
    let keys: Vec<&str> = base
        .stats
        .strikes
        .iter()
        .map(|strike| strike.key.as_str())
        .collect();
    assert_eq!(keys, vec!["Staff", "Staff#2", "Fist"]);
    let second = base.stats.strikes.get(1).expect("second strike");
    assert_eq!(
        second.attack, 0,
        "re-derived: no proficiencies table (untrained), str ±0, no pot — verbatim 3 ignored"
    );
    assert_eq!(second.damage, "d8", "re-derived flat 0: bare die, verbatim");
    assert_eq!(second.damage_flat, 0);
    let fist = base.stats.strikes.get(2).expect("fist");
    assert_eq!(fist.key, "Fist");
    assert_eq!(fist.attack, 0, "untrained unarmed at level 2: no bonus");
}
