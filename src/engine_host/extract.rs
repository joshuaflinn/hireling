//! `BaseStats` extraction (spec FR-4, design D3) — the host-side glue from
//! E5's normalized `base_sheet` to the engine's input shape. The math is
//! the design's table: verbatim totals where the export carries them
//! (`acTotal`, strikes, speed), rank + ability derivation everywhere else
//! (`eff_level·(rank≥1) + abil_mod + prof_bonus(rank)`; untrained adds
//! neither level nor bonus).
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
    // One trained-stat formula: level and proficiency only at rank ≥ 1.
    let trained = |rank_key: &str, ability: &str| {
        let rank = prof_rank(sheet, rank_key);
        i32_of(eff_level) * i32::from(rank >= 1) + prof_bonus(rank) + mod_of(ability)
    };

    let class_dc = {
        let rank = prof_rank(sheet, "classDC");
        (rank >= 1).then(|| {
            let key_mod = sheet.identity.keyability.as_deref().map_or(0, mod_of);
            10 + i32_of(eff_level) + key_mod + prof_bonus(rank)
        })
    };

    let strikes = strike_bases(sheet);

    let casters = sheet
        .spellcasters
        .iter()
        .map(|caster| {
            let rank = caster.proficiency.unwrap_or(0);
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

/// `prof_bonus: 0→+0 (no level), 1→+2, 2→+4, 3→+6, 4→+8` (design table).
fn prof_bonus(rank: i64) -> i32 {
    match rank {
        1 => 2,
        2 => 4,
        3 => 6,
        4 => 8,
        _ => 0,
    }
}

/// One proficiency rank from the sheet's `proficiencies` map; a missing
/// key is rank 0 (untrained), degrading the stat, never panicking.
fn prof_rank(sheet: &BaseSheet, key: &str) -> i64 {
    sheet
        .proficiencies
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
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

/// Strike bases from the export's verbatim `weapons` array: attack and
/// `damageBonus` verbatim, the roll string from `die` + bonus, and stable
/// keys — the name when unique, `name#2` on later duplicates (E5's FR-10
/// rule, mirrored here).
fn strike_bases(sheet: &BaseSheet) -> Vec<StrikeBase> {
    let Some(weapons) = sheet.weapons.as_ref().and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    weapons
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
            let attack = weapon
                .get("attack")
                .and_then(serde_json::Value::as_i64)
                .map_or(0, i32_of);
            let damage_flat = weapon
                .get("damageBonus")
                .and_then(serde_json::Value::as_i64)
                .map_or(0, i32_of);
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
        .collect()
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

/// i64 → i32 for game-bounded values (never near the i32 edge in practice);
/// overflow degrades to 0 rather than panicking or wrapping.
fn i32_of(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(0)
}
