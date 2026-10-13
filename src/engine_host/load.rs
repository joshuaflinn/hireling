//! Effect loading (D7): a party's stored effect rows → the engine's
//! `ActiveEffect[]` — targets, ord-ordered modifiers, version, and the
//! corpus chip flag. I/O orchestration only: no game math here. A stored
//! stat or type outside the closed vocabulary fails the load loudly — a
//! silently skipped row would lie to every derived total downstream (SC-4).

use std::collections::BTreeMap;

use anyhow::Context as _;
use sqlx::{PgPool, Row as _};

use crate::engine_host::parse_modifier_row;
use crate::pbimport::transform::BaseSheet;
use hireling_engine::model::ActiveEffect;

/// The character's weapon trait map from the items corpus (gh#62 design
/// D1): every `base_sheet.weapons` name, lowercased, to its corpus traits
/// in display form. One shared query — `corpus::item_rows`, the same
/// statement the bootstrap's `item_traits`/`item_bulk` maps read — so the
/// stored document shape cannot drift between bootstrap and strikes (the
/// defect class this epic kills). Names the corpus does not carry are
/// absent from the map; extraction renders them empty.
///
/// # Errors
///
/// Database failures propagate.
pub async fn weapon_trait_map(
    pool: &PgPool,
    sheet: &BaseSheet,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let names: Vec<String> = sheet
        .weapons
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .map(|weapons| {
            weapons
                .iter()
                .filter_map(|weapon| weapon.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut map = BTreeMap::new();
    if names.is_empty() {
        return Ok(map);
    }
    let rows = crate::pbimport::corpus::item_rows(pool, &names)
        .await
        .context("load weapon trait map")?;
    map.extend(rows.into_iter().map(|row| (row.name_lower, row.traits)));
    Ok(map)
}

/// Every effect of `party_id` — active AND ended (`active=false` is state,
/// not deletion; FR-15). The engine's compute filters actives; the chips
/// carry the flag. Deterministic order: effects by id, targets by
/// character id, modifiers by ord.
///
/// # Errors
///
/// Database failures propagate; a modifier row with a stat or type outside
/// the closed vocabulary is a corrupt-store error naming the offending row.
pub async fn party_effects(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<ActiveEffect>> {
    let rows = sqlx::query(
        "SELECT id, source_character_id, name, duration_note, active, version, tracked_manually \
         FROM effects WHERE party_id = $1 ORDER BY id",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("load party effects")?;
    let mut effects = Vec::with_capacity(rows.len());
    for row in rows {
        let effect_id: i64 = row.try_get("id").context("effect id")?;
        let targets: Vec<i64> = sqlx::query_scalar(
            "SELECT character_id FROM effect_targets WHERE effect_id = $1 ORDER BY character_id",
        )
        .bind(effect_id)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load targets of effect {effect_id}"))?;
        let modifier_rows: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT type, stat, value FROM effect_modifiers WHERE effect_id = $1 ORDER BY ord",
        )
        .bind(effect_id)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load modifiers of effect {effect_id}"))?;
        let mut modifiers = Vec::with_capacity(modifier_rows.len());
        for (ord, (modifier_type_text, stat_text, value)) in modifier_rows.into_iter().enumerate() {
            modifiers.push(parse_modifier_row(
                format!("effect {effect_id} modifier {ord}"),
                &modifier_type_text,
                &stat_text,
                value,
            )?);
        }
        effects.push(ActiveEffect {
            effect_id,
            name: row.try_get("name").context("effect name")?,
            source_character_id: row
                .try_get("source_character_id")
                .context("effect source")?,
            targets,
            modifiers,
            duration_note: row.try_get("duration_note").context("effect duration")?,
            active: row.try_get("active").context("effect active")?,
            version: row.try_get("version").context("effect version")?,
            tracked_manually: row
                .try_get("tracked_manually")
                .context("effect tracked_manually")?,
        });
    }
    Ok(effects)
}
