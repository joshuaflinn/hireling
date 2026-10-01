//! Unit tests for the transform — the fixture as ground truth (contract
//! §6), plus the drift and anchor edge cases the plan pins: duplicate caster
//! names, dangling container UUIDs, section type drift.

use serde_json::Value;

use super::transform;
use crate::pbimport::fixtures::reference_export;
use crate::pbimport::model::parse_and_validate;

/// Transform the frozen fixture.
fn fixture_sheet() -> (super::BaseSheet, super::SectionSkips) {
    let export = parse_and_validate(reference_export()).expect("fixture validates");
    transform(&export)
}

/// The sheet's caster at `index` (present by construction in these tests).
fn caster(sheet: &super::BaseSheet, index: usize) -> &super::Caster {
    sheet
        .spellcasters
        .get(index)
        .unwrap_or_else(|| panic!("caster {index} present"))
}

/// The sheet's equipment entry at `index`.
fn item(sheet: &super::BaseSheet, index: usize) -> &super::InventoryItem {
    sheet
        .equipment
        .get(index)
        .unwrap_or_else(|| panic!("equipment {index} present"))
}

/// Mutate the fixture's build object, re-validate, transform.
fn transformed_mutation(
    mutate: impl FnOnce(&mut Value),
) -> (super::BaseSheet, super::SectionSkips) {
    let mut doc: Value = serde_json::from_str(reference_export()).expect("fixture parses");
    let build = doc.get_mut("build").expect("build exists");
    mutate(build);
    let body = serde_json::to_string(&doc).expect("mutation serializes");
    let export = parse_and_validate(&body).expect("mutation validates");
    transform(&export)
}

#[test]
fn identity_is_verbatim() {
    let (sheet, skips) = fixture_sheet();
    let identity = &sheet.identity;
    assert_eq!(identity.name, "Lorum Ipsum", "name");
    assert_eq!(identity.class.as_deref(), Some("Wizard"), "class");
    assert_eq!(identity.dual_class, None, "dualClass null imports as null");
    assert_eq!(identity.level, 3, "level");
    assert_eq!(identity.xp, Some(18), "xp");
    assert_eq!(identity.ancestry.as_deref(), Some("Gnome"), "ancestry");
    assert_eq!(identity.heritage.as_deref(), Some("Wellspring Gnome"));
    assert_eq!(identity.background.as_deref(), Some("Charlatan"));
    assert_eq!(identity.alignment.as_deref(), Some("N"), "alignment");
    assert_eq!(identity.deity.as_deref(), Some("Not set"), "deity");
    assert_eq!(
        identity.age.as_deref(),
        Some("44"),
        "age imports as-is (string)"
    );
    assert_eq!(identity.gender.as_deref(), Some("Male"), "gender");
    assert_eq!(identity.size, Some(1), "size");
    assert_eq!(identity.size_name.as_deref(), Some("Small"), "sizeName");
    assert_eq!(identity.keyability.as_deref(), Some("int"), "keyability");
    assert_eq!(
        identity.languages,
        vec![
            "Aklo", "Common", "Dwarven", "Fey", "Gnomish", "Goblin", "Ysoki"
        ],
        "languages"
    );
    assert_eq!(sheet.schema, "hireling.base_sheet.v1", "schema tag");
    assert!(skips.sections.is_empty(), "fixture drifts nothing");
    assert!(
        skips.unresolved_containers.is_empty(),
        "fixture has no dangling containers"
    );
}

#[test]
fn ability_scores_are_the_six_fixture_values() {
    let (sheet, _) = fixture_sheet();
    let abilities = &sheet.abilities;
    assert_eq!(abilities.str, 8, "str");
    assert_eq!(abilities.dex, 12, "dex");
    assert_eq!(abilities.con, 14, "con");
    assert_eq!(abilities.int, 18, "int");
    assert_eq!(abilities.wis, 10, "wis");
    assert_eq!(abilities.cha, 16, "cha");
    assert!(
        sheet.abilities.breakdown.is_some(),
        "breakdown passes through"
    );
}

#[test]
fn max_hp_follows_the_contract_formula() {
    let (sheet, _) = fixture_sheet();
    // 8 ancestry + 6 class + 0 bonus + 0 per-level × (3 − 1) = 14.
    assert_eq!(sheet.hp.max_hp, 14, "contract §3.3 worked example");
    assert_eq!(sheet.hp.ancestryhp, 8, "ancestryhp");
    assert_eq!(sheet.hp.classhp, 6, "classhp");
    assert_eq!(sheet.hp.bonushp, 0, "bonushp");
    assert_eq!(sheet.hp.bonushp_per_level, 0, "bonushpPerLevel");
}

#[test]
fn ac_inputs_are_the_fixture_values_verbatim() {
    let (sheet, _) = fixture_sheet();
    let ac = sheet.ac.as_ref().expect("fixture carries acTotal");
    assert_eq!(
        ac.get("acTotal").and_then(Value::as_i64),
        Some(16),
        "acTotal"
    );
    assert_eq!(
        ac.get("acAbilityBonus").and_then(Value::as_i64),
        Some(1),
        "acAbilityBonus (fixture: 1)"
    );
    assert_eq!(
        ac.get("acProfBonus").and_then(Value::as_i64),
        Some(5),
        "acProfBonus (fixture: 5)"
    );
    assert_eq!(ac.get("acItemBonus").and_then(Value::as_i64), Some(0));
    assert_eq!(
        ac.get("shieldBonus"),
        Some(&Value::Null),
        "shieldBonus is null in the fixture and stays null"
    );
}

#[test]
fn spellcasters_carry_both_blocks_with_their_keys_and_layouts() {
    let (sheet, _) = fixture_sheet();
    assert_eq!(sheet.spellcasters.len(), 2, "two caster blocks");
    let wizard = caster(&sheet, 0);
    let gnome = caster(&sheet, 1);
    assert_eq!(wizard.caster_key, "Wizard", "unique name keeps bare key");
    assert_eq!(gnome.caster_key, "Wellspring Gnome", "second block's key");
    assert_eq!(wizard.magic_tradition.as_deref(), Some("arcane"));
    assert_eq!(wizard.spellcasting_type.as_deref(), Some("prepared"));
    assert_eq!(wizard.ability.as_deref(), Some("int"));
    assert_eq!(wizard.proficiency, Some(2));
    assert!(!wizard.innate, "Wizard block is not innate");
    assert_eq!(wizard.per_day.first(), Some(&6), "cantrips");
    assert_eq!(wizard.per_day.get(1), Some(&4), "rank 1");
    assert_eq!(wizard.per_day.get(2), Some(&3), "rank 2");
    assert_eq!(wizard.per_day.len(), 11, "per_day normalized to 11 entries");
    assert!(
        gnome.innate,
        "the Wellspring Gnome block is the innate regression case"
    );
    assert_eq!(gnome.per_day.first(), Some(&1), "one innate cantrip");
}

#[test]
fn prepared_rank_one_starts_with_the_homebrew_spell() {
    let (sheet, _) = fixture_sheet();
    let wizard = caster(&sheet, 0);
    let rank_one = wizard
        .prepared
        .iter()
        .find(|list| list.rank == 1)
        .expect("rank 1 prepared list");
    assert_eq!(
        rank_one.spells.first().map(String::as_str),
        Some("500 Toads"),
        "homebrew names import as any other (contract §6)"
    );
}

#[test]
fn equipment_resolves_containers_and_flags_invested() {
    let (sheet, _) = fixture_sheet();
    assert_eq!(sheet.equipment.len(), 16, "16 entries (contract §6)");

    let backpack = item(&sheet, 0);
    assert_eq!(backpack.name, "Backpack");
    assert_eq!(backpack.container, None, "Backpack is not inside itself");
    assert!(backpack.invested, "Invested marker parsed");

    let bedroll = item(&sheet, 1);
    assert_eq!(bedroll.name, "Bedroll");
    assert_eq!(
        bedroll.container.as_deref(),
        Some("Backpack"),
        "UUID resolved to the container name"
    );

    let lock = item(&sheet, 11);
    assert_eq!(lock.name, "Lock (Simple)");
    assert_eq!(
        lock.container.as_deref(),
        Some("Giant body's sack"),
        "the second container resolves too"
    );
}

#[test]
fn containers_carry_the_extradimensional_flag() {
    let (sheet, _) = fixture_sheet();
    assert_eq!(sheet.containers.len(), 2, "two containers");
    let by_name = |name: &str| {
        sheet
            .containers
            .iter()
            .find(|container| container.name == name)
            .unwrap_or_else(|| panic!("container {name} present"))
    };
    let backpack = by_name("Backpack");
    let sack = by_name("Giant body's sack");
    assert!(!backpack.extradimensional, "Backpack is mundane");
    assert!(backpack.backpack, "Backpack flag from the container entry");
    assert!(
        sack.extradimensional,
        "bagOfHolding maps to extradimensional"
    );
    assert!(!sack.backpack, "the sack is not a backpack");
}

#[test]
fn money_is_the_fixture_coin_counts() {
    let (sheet, _) = fixture_sheet();
    let money = sheet.money.as_ref().expect("fixture carries money");
    assert_eq!(money.get("cp").and_then(Value::as_i64), Some(4), "cp");
    assert_eq!(money.get("sp").and_then(Value::as_i64), Some(2), "sp");
    assert_eq!(money.get("gp").and_then(Value::as_i64), Some(24), "gp");
    assert_eq!(money.get("pp").and_then(Value::as_i64), Some(0), "pp");
}

#[test]
fn the_familiar_arrives_as_a_companion() {
    let (sheet, _) = fixture_sheet();
    assert_eq!(sheet.companions.len(), 1, "one familiar");
    let pippin = sheet.companions.first().expect("the familiar is present");
    assert_eq!(pippin.kind.as_deref(), Some("Familiar"), "type");
    assert_eq!(pippin.name.as_deref(), Some("Familiar (Pippin)"), "name");
    assert!(
        pippin
            .name
            .as_deref()
            .is_some_and(|name| name.contains("Pippin")),
        "Pippin is present"
    );
}

#[test]
fn raw_passthrough_holds_the_unmodeled_sections_verbatim() {
    let export = parse_and_validate(reference_export()).expect("fixture validates");
    let (sheet, _) = fixture_sheet();
    let build = export.build();
    assert_eq!(
        sheet
            .raw
            .get("feats")
            .and_then(|feats| feats.get(0))
            .and_then(|feat| feat.get(0)),
        build
            .get("feats")
            .and_then(|feats| feats.get(0))
            .and_then(|feat| feat.get(0)),
        "raw.feats[0][0] == \"Charming Liar\" (verbatim passthrough)"
    );
    for section in [
        "feats",
        "specials",
        "resistances",
        "rituals",
        "formula",
        "mods",
        "inventorMods",
        "pets",
    ] {
        assert_eq!(
            sheet.raw.get(section),
            build.get(section),
            "raw.{section} passes through verbatim"
        );
    }
    // The typed sections are NOT in raw.
    assert!(
        sheet.raw.get("money").is_none() && sheet.raw.get("equipment").is_none(),
        "modeled sections never duplicate into raw"
    );
}

#[test]
fn weapons_armor_focus_lores_proficiencies_land_in_their_sections() {
    let export = parse_and_validate(reference_export()).expect("fixture validates");
    let (sheet, _) = fixture_sheet();
    let build = export.build();
    assert_eq!(
        sheet.weapons.as_ref(),
        build.get("weapons"),
        "weapons verbatim"
    );
    assert_eq!(sheet.armor.as_ref(), build.get("armor"), "armor verbatim");
    assert_eq!(sheet.focus.as_ref(), build.get("focus"), "focus verbatim");
    assert_eq!(sheet.focus_points, 1, "focusPoints");
    assert_eq!(
        sheet.proficiencies,
        build
            .get("proficiencies")
            .cloned()
            .expect("proficiencies object"),
        "flat map verbatim"
    );
    assert_eq!(
        sheet.lores,
        vec![
            super::Lore {
                name: "Underworld".to_owned(),
                rank: 2
            },
            super::Lore {
                name: "Mror Holds History".to_owned(),
                rank: 4
            },
        ],
        "lores normalized from pairs"
    );
}

/// Replace a top-level build key wholesale.
fn replace_key(build: &mut Value, key: &str, value: Value) {
    build
        .as_object_mut()
        .expect("build is an object")
        .insert(key.to_owned(), value);
}

/// Set a nested path inside the build object (fixture paths exist). Array
/// steps are numeric strings.
fn set_at(build: &mut Value, path: &[&str], value: Value) {
    let (last, parents) = path.split_last().expect("non-empty path");
    let mut node = build;
    for step in parents {
        if let Ok(index) = step.parse::<usize>() {
            node = node.get_mut(index).expect("fixture array index exists");
        } else {
            node = node.get_mut(*step).expect("fixture parent exists");
        }
    }
    if let Ok(index) = last.parse::<usize>() {
        let slot = node
            .as_array_mut()
            .expect("fixture array exists")
            .get_mut(index)
            .expect("fixture array index exists");
        *slot = value;
    } else {
        node.as_object_mut()
            .expect("fixture node is an object")
            .insert((*last).to_owned(), value);
    }
}

#[test]
fn duplicate_caster_names_get_ordinal_suffixes() {
    let (sheet, _) = transformed_mutation(|build| {
        replace_key(
            build,
            "spellCasters",
            serde_json::json!([
                { "name": "Wizard", "perDay": [1,0,0,0,0,0,0,0,0,0,0] },
                { "name": "Wizard", "perDay": [2,0,0,0,0,0,0,0,0,0,0] }
            ]),
        );
    });
    let keys: Vec<&str> = sheet
        .spellcasters
        .iter()
        .map(|caster| caster.caster_key.as_str())
        .collect();
    assert_eq!(keys, vec!["Wizard", "Wizard#2"], "FR-10 tie-break");
}

#[test]
fn a_dangling_container_uuid_imports_without_container_and_is_noticed() {
    let (sheet, skips) = transformed_mutation(|build| {
        set_at(
            build,
            &["equipment", "1", "2"],
            Value::String("00000000-0000-0000-0000-000000000000".to_owned()),
        );
    });
    let bedroll = item(&sheet, 1);
    assert_eq!(bedroll.name, "Bedroll");
    assert_eq!(bedroll.container, None, "unresolved UUID = no container");
    assert_eq!(
        skips.unresolved_containers,
        vec!["Bedroll".to_owned()],
        "the dangling reference is noticed"
    );
}

#[test]
fn a_type_drifted_equipment_section_is_skipped_whole() {
    let (sheet, skips) = transformed_mutation(|build| {
        replace_key(build, "equipment", serde_json::json!({ "not": "an array" }));
    });
    assert!(
        sheet.equipment.is_empty(),
        "the drifted section imports empty"
    );
    assert_eq!(
        skips.sections,
        vec!["equipment".to_owned()],
        "contract §5: skipped whole, surfaced for the diff"
    );
    assert_eq!(sheet.containers.len(), 2, "everything else still imports");
    assert_eq!(sheet.spellcasters.len(), 2, "spellcasting unaffected");
}

#[test]
fn slot_layout_enumerates_every_rank_position() {
    let (sheet, _) = fixture_sheet();
    let layout = sheet.slot_layout();
    // Wizard: 6 cantrips + 4 rank-1 + 3 rank-2; gnome: 1 cantrip.
    assert_eq!(
        layout.len(),
        6 + 4 + 3 + 1,
        "every perDay position materializes"
    );
    let wizard_rank1: Vec<i64> = layout
        .iter()
        .filter(|(key, rank, _)| key == "Wizard" && *rank == 1)
        .map(|(_, _, index)| *index)
        .collect();
    assert_eq!(wizard_rank1, vec![0, 1, 2, 3], "rank-1 indices 0..4");
    assert!(
        layout.contains(&("Wellspring Gnome".to_owned(), 0, 0)),
        "the innate cantrip materializes"
    );
}

// E6 Task 4: the web engine suite consumes the transform's output for the
// reference export as its fixture (`web/tests/data/base_sheet_reference.json`).
// This test pins that file to the transform: when E5's transform moves, the
// Rust gate flags the drift instead of the sheet silently computing on a
// stale shape. Regenerate with HIRELING_REGEN_WEB_FIXTURE=1.
#[test]
fn the_web_base_sheet_fixture_matches_the_transform() {
    let (sheet, skips) = fixture_sheet();
    assert!(
        skips.sections.is_empty(),
        "the reference export has no drifted sections: {:?}",
        skips.sections
    );
    let json = serde_json::to_string_pretty(&sheet).expect("fixture serializes");
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/web/tests/data/base_sheet_reference.json"
    );
    if std::env::var("HIRELING_REGEN_WEB_FIXTURE").is_ok() {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).expect("fixture directory");
        }
        std::fs::write(path, json).expect("fixture write");
        return;
    }
    let committed = std::fs::read_to_string(path)
        .expect("web fixture exists; regen with HIRELING_REGEN_WEB_FIXTURE=1");
    assert_eq!(
        committed.trim_end(),
        json.trim_end(),
        "the web base_sheet fixture drifted from the transform; \
         re-run cargo test with HIRELING_REGEN_WEB_FIXTURE=1"
    );
}
