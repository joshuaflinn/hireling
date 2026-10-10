//! `BaseStats` extraction (spec FR-4, design D3) — the host-side glue from
//! E5's normalized `base_sheet` to the engine's input shape. The math is
//! the design's table: verbatim totals where the export carries them
//! (`acTotal`, speed), rank + ability derivation everywhere else
//! (`eff_level·(rank≥1) + abil_mod + prof_bonus(rank)`; untrained adds
//! neither level nor bonus), the prototype's strike re-derivation, and
//! the wizard class progression when displaying above the export's level.
//!
//! Degraded sections degrade the STAT (0 / null), never the import — the
//! same contract-§5 philosophy as the transform. The golden test
//! (`tests/extract_golden.rs`) pins every number against the prototype's
//! rendered values; the prototype is display truth.

use hireling_engine::model::{
    Attributes, BASE_SCHEMA, BaseStats, CasterBase, RenderBase, SkillBase, Stats, StrikeBase,
};
use hireling_engine::vocab::{CORE_SKILL_ABILITY, CORE_SKILLS};

use crate::pbimport::transform::BaseSheet;

/// Extract the engine's input for one character. `level_adjust` is the
/// live ±level from the party state; `eff_level = identity.level +
/// level_adjust`, clamped 1..=20.
#[must_use]
pub fn extract(sheet: &BaseSheet, level_adjust: i64) -> BaseStats {
    let eff_level = (sheet.identity.level + level_adjust).clamp(1, 20);

    let abilities = &sheet.abilities;
    let con_mod = ability_mod(abilities.con);
    let mod_of = |ability: &str| match ability {
        "str" => i32_of(ability_mod(abilities.str)),
        "dex" => i32_of(ability_mod(abilities.dex)),
        "con" => i32_of(con_mod),
        "int" => i32_of(ability_mod(abilities.int)),
        "wis" => i32_of(ability_mod(abilities.wis)),
        "cha" => i32_of(ability_mod(abilities.cha)),
        _ => 0,
    };
    let ranks = progressed_ranks(sheet, eff_level);
    let rank_of = |key: &str| ranks.get(key).copied().unwrap_or(0);
    let ac = ac_of(sheet, &ranks, eff_level);
    // One trained-stat formula: level and proficiency only at rank ≥ 1.
    let trained = |rank_key: &str, ability: &str| {
        let rank = rank_of(rank_key);
        i32_of(eff_level) * i32::from(rank >= 1) + prof_bonus(rank) + mod_of(ability)
    };

    let class_dc = {
        let rank = rank_of("classDC");
        (rank >= 1).then(|| {
            let key_mod = sheet.identity.keyability.as_deref().map_or(0, mod_of);
            10 + i32_of(eff_level) + key_mod + prof_bonus(rank)
        })
    };

    let strikes = strike_bases(sheet, &ranks, eff_level);

    // Render inputs with no modifier math (contract §3 render_base, design
    // D3). hp_max is the PROTOTYPE's formula — CON and the per-level bonus
    // count at every level, unlike the stored `hp.max_hp` anchor (the
    // deviation is named in D3; the golden pins 32 for the reference).
    let render_base = render_base_of(sheet, con_mod, eff_level);

    let casters = sheet
        .spellcasters
        .iter()
        .map(|caster| {
            // Innate casters rank by their own block; prepared/focused ones
            // by the better of block and tradition (the prototype's max).
            let tradition_rank = caster.magic_tradition.as_deref().map_or(0, |tradition| {
                rank_of(&format!("casting{}", capitalize(tradition)))
            });
            let rank = if caster.innate {
                caster.proficiency.unwrap_or(0)
            } else {
                caster.proficiency.unwrap_or(0).max(tradition_rank)
            };
            let ability_mod_value = caster.ability.as_deref().map_or(0, mod_of);
            let spell_attack =
                i32_of(eff_level) * i32::from(rank >= 1) + prof_bonus(rank) + ability_mod_value;
            CasterBase {
                caster_key: caster.caster_key.clone(),
                spell_attack,
                spell_dc: spell_attack + 10,
                innate: caster.innate,
            }
        })
        .collect();

    // Skills: the 18 core in vocabulary order, then the sheet's lores.
    // `rank` rides verbatim (contract §3: render input for the rank letter
    // and untrained dimming); lores carry the export's display name as
    // `label` — the canonical `lore:` key lowercases and underscores, so
    // the display case cannot be recovered from it (MOR-50's
    // derive-the-label-from-the-key ruling meets the realized charset and
    // loses; the display input rides the row instead, same as strikes).
    let mut skills: Vec<SkillBase> = CORE_SKILLS
        .iter()
        .zip(CORE_SKILL_ABILITY.iter().map(|(_, ability)| *ability))
        .map(|(skill, ability)| SkillBase {
            name: (*skill).to_owned(),
            total: trained(skill, ability),
            rank: i32_of(rank_of(skill)),
            label: None,
        })
        .collect();
    for lore in &sheet.lores {
        skills.push(SkillBase {
            name: format!("lore:{}", lore_instance_name(&lore.name)),
            total: i32_of(eff_level) * i32::from(lore.rank >= 1)
                + mod_of("int")
                + prof_bonus(lore.rank),
            rank: i32_of(lore.rank),
            label: Some(lore.name.clone()),
        });
    }

    BaseStats {
        schema: BASE_SCHEMA.to_owned(),
        level: i32_of(eff_level),
        stats: Stats {
            ac,
            fort: trained("fortitude", "con"),
            reflex: trained("reflex", "dex"),
            will: trained("will", "wis"),
            perception: trained("perception", "wis"),
            speed: speed_total(sheet),
            class_dc,
            strikes,
            casters,
            skills,
        },
        render_base,
    }
}

/// The modifier-free render inputs (contract §3 `render_base`, design D3):
/// computed once per character, copied to the output verbatim. `con_mod`
/// is the ability modifier already resolved by the caller; the hp ceiling
/// is the PROTOTYPE's formula — CON and the per-level bonus count at
/// every level, unlike the stored `hp.max_hp` anchor.
fn render_base_of(sheet: &BaseSheet, con_mod: i64, eff_level: i64) -> RenderBase {
    RenderBase {
        level: i32_of(eff_level),
        hp_max: i32_of(
            sheet.hp.ancestryhp
                + sheet.hp.bonushp
                + (sheet.hp.classhp + con_mod + sheet.hp.bonushp_per_level) * eff_level,
        ),
        focus_max: i32_of(sheet.focus_points),
        hero_max: 3,
        cantrip_rank: i32_of((eff_level + 1) / 2),
        attributes: Attributes {
            r#str: i32_of(ability_mod(sheet.abilities.str)),
            dex: i32_of(ability_mod(sheet.abilities.dex)),
            con: i32_of(con_mod),
            int: i32_of(ability_mod(sheet.abilities.int)),
            wis: i32_of(ability_mod(sheet.abilities.wis)),
            cha: i32_of(ability_mod(sheet.abilities.cha)),
        },
    }
}

/// `⌊(score − 10) / 2⌋` — the `PF2e` ability modifier, floored (`8 → −1`).
fn ability_mod(score: i64) -> i64 {
    (score - 10).div_euclid(2)
}

/// The proficiency table the whole derivation reads: the export's raw
/// Pathbuilder ranks, bumped by the class progression the prototype
/// applies when displaying above the export's level (POC: Wizard only —
/// `max` semantics, like the prototype's `up`).
fn progressed_ranks(sheet: &BaseSheet, eff_level: i64) -> std::collections::HashMap<String, i64> {
    let mut ranks: std::collections::HashMap<String, i64> = sheet
        .proficiencies
        .as_object()
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, value)| value.as_i64().map(|rank| (key.clone(), rank)))
                .collect()
        })
        .unwrap_or_default();
    if sheet.identity.class.as_deref() == Some("Wizard") && eff_level > sheet.identity.level {
        for (threshold, bumps) in WIZARD_PROGRESSION {
            if eff_level >= *threshold {
                for (key, rank) in *bumps {
                    let current = ranks.get(*key).copied().unwrap_or(0);
                    ranks.insert((*key).to_owned(), (*rank).max(current));
                }
            }
        }
    }
    ranks
}

/// Player Core wizard class progression, the prototype's
/// `wizardProgression` verbatim: `(display level ≥ threshold, bumps)`.
const WIZARD_PROGRESSION: &[(i64, &[(&str, i64)])] = &[
    (5, &[("reflex", 4)]),
    (7, &[("castingArcane", 4)]),
    (9, &[("fortitude", 4)]),
    (11, &[("perception", 4), ("simple", 4), ("unarmed", 4)]),
    (13, &[("unarmored", 4)]),
    (15, &[("castingArcane", 6)]),
    (17, &[("will", 6)]),
    (19, &[("castingArcane", 8)]),
];

/// The export's RAW Pathbuilder rank IS the bonus (untrained 0, trained 2,
/// expert 4, master 6, legendary 8); level rides in the callers'
/// `eff_level·(rank≥1)` term — `pb = rank > 0 ? rank + level : 0`
/// (prototype line 1377). The earlier step encoding (1→+2, 2→+4, …) fed
/// raw ranks into step slots and doubled every proficiency-bearing stat
/// (review finding, MOR-51).
fn prof_bonus(rank: i64) -> i32 {
    i32_of(rank)
}

/// AC RE-DERIVED from the export's parts (E6 spec §4 — the sheet's own
/// AC formula):
/// `10 + acAbilityBonus + proficiency(worn-armor rank, eff_level) +
/// acItemBonus + shieldBonus-while-raised`, with `proficiency = rank > 0 ?
/// rank + eff_level : 0` and the armor category taken from the worn piece
/// (`unarmored` when none). The shield's bonus counts only while the sheet
/// state marks the shield raised (`ac.shieldRaised: true` — Raise a Shield
/// is an action, not a constant). The export's frozen `acTotal` is NOT
/// used: it pins the export level and freezes AC forever, so a
/// `level_adjust` never moved it. At the export's own level the two agree
/// exactly (the reference golden pins 16 both ways); above it the
/// re-derivation tracks the class table — which is the point.
fn ac_of(sheet: &BaseSheet, ranks: &std::collections::HashMap<String, i64>, eff_level: i64) -> i32 {
    let ac_section = sheet.ac.as_ref();
    let part = |key: &str| {
        ac_section
            .and_then(|ac| ac.get(key))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let worn_prof = sheet
        .armor
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .and_then(|pieces| {
            pieces
                .iter()
                .find(|piece| piece.get("worn").and_then(serde_json::Value::as_bool) == Some(true))
        })
        .and_then(|piece| piece.get("prof"))
        .and_then(serde_json::Value::as_str);
    let armor_prof = worn_prof.unwrap_or("unarmored");
    let rank = ranks.get(armor_prof).copied().unwrap_or(0);
    let raised = ac_section
        .and_then(|ac| ac.get("shieldRaised"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let shield = if raised { part("shieldBonus") } else { 0 };
    i32_of(
        10_i64
            + part("acAbilityBonus")
            + eff_level * i64::from(rank >= 1)
            + i64::from(prof_bonus(rank))
            + part("acItemBonus")
            + shield,
    )
}

/// `attributes.speed + attributes.speedBonus`; a missing section is 0.
fn speed_total(sheet: &BaseSheet) -> i32 {
    sheet.attributes.as_ref().map_or(0, |attributes| {
        let speed = attributes
            .get("speed")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let bonus = attributes
            .get("speedBonus")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        i32_of(speed + bonus)
    })
}

/// Strike bases per the prototype's strike math (line 1415): attack is
/// RE-DERIVED — ability (Finesse → best of Str/Dex) + proficiency
/// (rank + level at rank ≥ 1) + potency — never the export's verbatim
/// `attack` (equal at the export's own level, divergent above it).
/// Damage flat is Str plus Weapon Specialization (a class feature —
/// `mastery_damage`). The unarmed Fist row the sheet renders is appended
/// (prototype line 1419).
fn strike_bases(
    sheet: &BaseSheet,
    ranks: &std::collections::HashMap<String, i64>,
    eff_level: i64,
) -> Vec<StrikeBase> {
    let str_mod = i32_of(ability_mod(sheet.abilities.str));
    let dex_mod = i32_of(ability_mod(sheet.abilities.dex));
    let class = sheet.identity.class.as_deref();
    let pb = |rank: i64| i32_of(eff_level) * i32::from(rank >= 1) + prof_bonus(rank);
    let rank_of = |key: &str| ranks.get(key).copied().unwrap_or(0);
    let Some(weapons) = sheet.weapons.as_ref().and_then(serde_json::Value::as_array) else {
        // No weapons: the sheet still renders the unarmed Fist.
        let flat = str_mod + mastery_damage(class, rank_of("unarmed"), eff_level);
        return vec![fist_row(
            "Fist",
            str_mod.max(dex_mod) + pb(rank_of("unarmed")),
            flat,
        )];
    };
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut strikes: Vec<StrikeBase> = weapons
        .iter()
        .map(|weapon| {
            let name = weapon
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let seen = counts.entry(name.to_owned()).or_insert(0);
            *seen += 1;
            let key = if *seen == 1 {
                name.to_owned()
            } else {
                format!("{name}#{seen}")
            };
            let traits = weapon_traits(name)
                .iter()
                .map(|trait_name| (*trait_name).to_owned())
                .collect::<Vec<String>>();
            let rank = rank_of(
                weapon
                    .get("prof")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            );
            let pot = weapon
                .get("pot")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let ability_mod_used = if traits.iter().any(|trait_name| trait_name == "Finesse") {
                str_mod.max(dex_mod)
            } else {
                str_mod
            };
            let attack = ability_mod_used + pb(rank) + i32_of(pot);
            let damage_flat = str_mod + mastery_damage(class, rank, eff_level);
            let die = weapon
                .get("die")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let label = weapon
                .get("display")
                .and_then(serde_json::Value::as_str)
                .filter(|display| !display.is_empty())
                .unwrap_or(name);
            let damage_type = weapon
                .get("damageType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            StrikeBase {
                key,
                label: label.to_owned(),
                attack,
                damage: roll_string(die, damage_flat),
                damage_flat,
                map: map_step(&traits),
                damage_type: damage_type.to_owned(),
                damage_type_name: damage_type_name_of(damage_type).to_owned(),
                traits,
            }
        })
        .collect();
    let fist_seen = counts.entry("Fist".to_owned()).or_insert(0);
    *fist_seen += 1;
    let key = if *fist_seen == 1 {
        "Fist".to_owned()
    } else {
        format!("Fist#{fist_seen}")
    };
    let unarmed = rank_of("unarmed");
    let unarmed_flat = str_mod + mastery_damage(class, unarmed, eff_level);
    strikes.push(fist_row(
        &key,
        str_mod.max(dex_mod) + pb(unarmed),
        unarmed_flat,
    ));
    strikes
}

/// The unarmed Fist row the sheet always renders (prototype line 1419):
/// d4, best of Str/Dex, the fixed unarmed trait row, Agile MAP step.
fn fist_row(key: &str, attack: i32, flat: i32) -> StrikeBase {
    let traits: Vec<String> = weapon_traits("Fist")
        .iter()
        .map(|trait_name| (*trait_name).to_owned())
        .collect();
    StrikeBase {
        key: key.to_owned(),
        label: "Fist".to_owned(),
        attack,
        damage: roll_string("d4", flat),
        damage_flat: flat,
        map: map_step(&traits),
        damage_type: "B".to_owned(),
        damage_type_name: "bludgeoning".to_owned(),
        traits,
    }
}

/// Weapon traits at POC — the prototype's WEAPONS table rows the
/// reference sheet renders. `Finesse` attacks with the best of Str/Dex;
/// `Agile` softens the multiple attack penalty (render-side).
fn weapon_traits(name: &str) -> &'static [&'static str] {
    match name {
        "Staff" => &["Monk", "Two-Hand d8"],
        "Fist" => &["Agile", "Finesse", "Nonlethal", "Unarmed"],
        _ => &[],
    }
}

/// Weapon Specialization — a CLASS feature: each row is (class, the level
/// its advancement table grants it). Every row read 2026-10-10 off that
/// class's remaster page on `AoN` (`2e.aonprd.com/Classes.aspx?ID=n`); the
/// book+page cite per row is the source line the page itself prints for
/// the class entry. 27 classes gain it; absence from this table means
/// the class never does — today only Exemplar and Runesmith, whose
/// pages carry no Weapon Specialization feature section at all (War of
/// Immortals pg. 25; Impossible Magic pg. 44 — their 13th-level rows
/// are divine weapon mastery / weapon mastery, different features),
/// asserted in the golden's `classes_the_table_omits_never_gain_the_feature`.
const WEAPON_SPECIALIZATION: &[(&str, i64)] = &[
    // Gained at 7 — the martial chassis.
    ("Barbarian", 7),    // Player Core 2 pg. 72 (AoN ID 57)
    ("Champion", 7),     // Player Core 2 pg. 86 (AoN ID 58)
    ("Commander", 7),    // Battlecry! pg. 21 (AoN ID 66)
    ("Fighter", 7),      // Player Core pg. 136 (AoN ID 35)
    ("Gunslinger", 7),   // Guns & Gears (Remastered) pg. 105 (AoN ID 20)
    ("Inventor", 7),     // Guns & Gears (Remastered) pg. 16 (AoN ID 19)
    ("Investigator", 7), // Player Core 2 pg. 102 (AoN ID 59)
    ("Magus", 7),        // Impossible Magic pg. 11 (AoN ID 74)
    ("Monk", 7),         // Player Core 2 pg. 116 (AoN ID 60)
    ("Ranger", 7),       // Player Core pg. 154 (AoN ID 36)
    ("Rogue", 7),        // Player Core pg. 168 (AoN ID 37)
    ("Swashbuckler", 7), // Player Core 2 pg. 161 (AoN ID 63)
    ("Thaumaturge", 7),  // Dark Archive (Remastered) pg. 32 (AoN ID 69)
    // Gained at 11 — the Guardian alone; its 13th-level row is weapon
    // mastery, a different feature.
    ("Guardian", 11), // Battlecry! pg. 38 (AoN ID 67)
    // Gained at 13 — the remaster caster chassis: every remaining class
    // whose page carries the feature. (Summoner's 7th-level row lists
    // "eidolon weapon specialization" — the eidolon's feature; the
    // summoner's own is the row below.)
    ("Alchemist", 13),   // Player Core 2 pg. 59 (AoN ID 56)
    ("Animist", 13),     // War of Immortals pg. 10 (AoN ID 64)
    ("Bard", 13),        // Player Core pg. 94 (AoN ID 32)
    ("Cleric", 13),      // Player Core pg. 108 (AoN ID 33)
    ("Druid", 13),       // Player Core pg. 122 (AoN ID 34)
    ("Kineticist", 13),  // Rage of Elements pg. 15 (AoN ID 23)
    ("Necromancer", 13), // Impossible Magic pg. 31 (AoN ID 75)
    ("Oracle", 13),      // Player Core 2 pg. 128 (AoN ID 61)
    ("Psychic", 13),     // Dark Archive (Remastered) pg. 12 (AoN ID 68)
    ("Sorcerer", 13),    // Player Core 2 pg. 144 (AoN ID 62)
    ("Summoner", 13),    // Impossible Magic pg. 64 (AoN ID 77)
    ("Witch", 13),       // Player Core pg. 178 (AoN ID 38)
    ("Wizard", 13),      // Player Core pg. 197 (AoN ID 39)
];

/// Extra strike damage from Weapon Specialization (the prototype's
/// `spec`): expert +2, master +3, legendary +4 with the weapon's rank,
/// from the class's specialization level on — nothing for classes the
/// table does not name (deliberate absence, asserted in the golden).
/// The class matches case-insensitively: the export's casing is data,
/// not law, and an exact-match miss is a silent zero.
fn mastery_damage(class: Option<&str>, rank: i64, level: i64) -> i32 {
    let gained_at = class.and_then(|named| {
        WEAPON_SPECIALIZATION
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(named))
            .map(|&(_, at)| at)
    });
    let Some(gained_at) = gained_at else {
        return 0;
    };
    if level < gained_at {
        return 0;
    }
    match rank {
        4 => 2,
        6 => 3,
        8 => 4,
        _ => 0,
    }
}

/// The MAP STEP a strike row renders: Agile strikes step by 4, all others
/// by 5 (the row shows `−step / −2·step`).
fn map_step(traits: &[String]) -> i32 {
    if traits.iter().any(|trait_name| trait_name == "Agile") {
        4
    } else {
        5
    }
}

/// The export's damage-type letter and its display name — the sheet renders
/// the name (`damage B` → `bludgeoning`); an unknown code renders as itself.
fn damage_type_name_of(code: &str) -> &str {
    match code {
        "B" => "bludgeoning",
        "P" => "piercing",
        "S" => "slashing",
        other => other,
    }
}

/// The sheet's damage expression, exactly as rendered: the export's die
/// VERBATIM (`d4`, no `1` prepended — base.js renders `weapon.die` as-is)
/// plus the signed flat — `+N`, or `−N` with the typographic minus
/// (U+2212, the sheet's convention); a zero flat renders the bare die.
/// Rendered verbatim downstream, never parsed.
fn roll_string(die: &str, bonus: i32) -> String {
    match bonus.cmp(&0) {
        std::cmp::Ordering::Greater => format!("{die}+{bonus}"),
        std::cmp::Ordering::Less => format!("{die}\u{2212}{}", bonus.unsigned_abs()),
        std::cmp::Ordering::Equal => die.to_owned(),
    }
}

/// A lore's instance name: lowercase, letters/digits kept, everything else
/// an underscore (`"Mror Holds History"` → `mror_holds_history`) — the
/// canonical charset `StatName` validates against.
fn lore_instance_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                character
            } else if character.is_ascii_uppercase() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// `arcane` → `Arcane` — the tradition's key into the proficiencies table.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// i64 → i32 for game-bounded values (never near the i32 edge in practice);
/// overflow degrades to 0 rather than panicking or wrapping.
fn i32_of(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(0)
}
