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

use hireling_engine::model::{BASE_SCHEMA, BaseStats, CasterBase, SkillBase, Stats, StrikeBase};
use hireling_engine::vocab::{CORE_SKILL_ABILITY, CORE_SKILLS};

use crate::pbimport::transform::BaseSheet;

/// Extract the engine's input for one character. `level_adjust` is the
/// live ±level from the party state; `eff_level = identity.level +
/// level_adjust`, clamped 1..=20.
#[must_use]
pub fn extract(sheet: &BaseSheet, level_adjust: i64) -> BaseStats {
    let eff_level = (sheet.identity.level + level_adjust).clamp(1, 20);

    let abilities = &sheet.abilities;
    let mod_of = |ability: &str| match ability {
        "str" => i32_of(ability_mod(abilities.str)),
        "dex" => i32_of(ability_mod(abilities.dex)),
        "con" => i32_of(ability_mod(abilities.con)),
        "int" => i32_of(ability_mod(abilities.int)),
        "wis" => i32_of(ability_mod(abilities.wis)),
        "cha" => i32_of(ability_mod(abilities.cha)),
        _ => 0,
    };
    let ranks = progressed_ranks(sheet, eff_level);
    let rank_of = |key: &str| ranks.get(key).copied().unwrap_or(0);
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
            }
        })
        .collect();

    // Skills: the 18 core in vocabulary order, then the sheet's lores.
    let mut skills: Vec<SkillBase> = CORE_SKILLS
        .iter()
        .zip(CORE_SKILL_ABILITY.iter().map(|(_, ability)| *ability))
        .map(|(skill, ability)| SkillBase {
            name: (*skill).to_owned(),
            total: trained(skill, ability),
        })
        .collect();
    for lore in &sheet.lores {
        skills.push(SkillBase {
            name: format!("lore:{}", lore_instance_name(&lore.name)),
            total: i32_of(eff_level) * i32::from(lore.rank >= 1)
                + mod_of("int")
                + prof_bonus(lore.rank),
        });
    }

    BaseStats {
        schema: BASE_SCHEMA.to_owned(),
        level: i32_of(eff_level),
        stats: Stats {
            ac: ac_total(sheet),
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

/// `acTotal.acTotal` verbatim; a missing or drifted section is 0.
fn ac_total(sheet: &BaseSheet) -> i32 {
    sheet
        .ac
        .as_ref()
        .and_then(|ac| ac.get("acTotal"))
        .and_then(serde_json::Value::as_i64)
        .map_or(0, i32_of)
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
/// Damage flat is Str plus the mastery bonus (level ≥ 13). The unarmed
/// Fist row the sheet renders is appended (prototype line 1419).
fn strike_bases(
    sheet: &BaseSheet,
    ranks: &std::collections::HashMap<String, i64>,
    eff_level: i64,
) -> Vec<StrikeBase> {
    let str_mod = i32_of(ability_mod(sheet.abilities.str));
    let dex_mod = i32_of(ability_mod(sheet.abilities.dex));
    let pb = |rank: i64| i32_of(eff_level) * i32::from(rank >= 1) + prof_bonus(rank);
    let rank_of = |key: &str| ranks.get(key).copied().unwrap_or(0);
    let Some(weapons) = sheet.weapons.as_ref().and_then(serde_json::Value::as_array) else {
        // No weapons: the sheet still renders the unarmed Fist.
        let flat = str_mod + mastery_damage(rank_of("unarmed"), eff_level);
        return vec![StrikeBase {
            key: "Fist".to_owned(),
            label: "Fist".to_owned(),
            attack: str_mod.max(dex_mod) + pb(rank_of("unarmed")),
            damage: roll_string("d4", flat),
            damage_flat: flat,
        }];
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
            let traits = weapon_traits(name);
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
            let ability_mod_used = if traits.contains(&"Finesse") {
                str_mod.max(dex_mod)
            } else {
                str_mod
            };
            let attack = ability_mod_used + pb(rank) + i32_of(pot);
            let damage_flat = str_mod + mastery_damage(rank, eff_level);
            let die = weapon
                .get("die")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let label = weapon
                .get("display")
                .and_then(serde_json::Value::as_str)
                .filter(|display| !display.is_empty())
                .unwrap_or(name);
            StrikeBase {
                key,
                label: label.to_owned(),
                attack,
                damage: roll_string(die, damage_flat),
                damage_flat,
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
    let unarmed_flat = str_mod + mastery_damage(unarmed, eff_level);
    strikes.push(StrikeBase {
        key,
        label: "Fist".to_owned(),
        attack: str_mod.max(dex_mod) + pb(unarmed),
        damage: roll_string("d4", unarmed_flat),
        damage_flat: unarmed_flat,
    });
    strikes
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

/// Extra strike damage from mastery ranks at level 13+ (the prototype's
/// `spec`): expert +2, master +3, legendary +4; nothing below level 13.
fn mastery_damage(rank: i64, level: i64) -> i32 {
    if level < 13 {
        return 0;
    }
    match rank {
        4 => 2,
        6 => 3,
        8 => 4,
        _ => 0,
    }
}

/// The sheet's damage roll string: `d4` → `1d4`, `+N`/`-N` appended when
/// the flat bonus is nonzero. Rendered verbatim downstream, never parsed.
fn roll_string(die: &str, bonus: i32) -> String {
    let dice_text = if die.starts_with('d') {
        format!("1{die}")
    } else {
        die.to_owned()
    };
    match bonus.cmp(&0) {
        std::cmp::Ordering::Greater => format!("{dice_text}+{bonus}"),
        std::cmp::Ordering::Less => format!("{dice_text}{bonus}"),
        std::cmp::Ordering::Equal => dice_text,
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
