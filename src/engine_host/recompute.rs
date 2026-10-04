//! Recompute (D7 architecture): a character's stored state → the engine's
//! [`EngineOutput`] — base extraction (D3), party effect loading, and the
//! pure engine `compute`, one read-only pass per character. No math here:
//! shape translation and I/O orchestration only.

use std::collections::BTreeMap;

use anyhow::Context as _;
use serde_json::Value as JsonValue;
use sqlx::PgPool;

use crate::engine_host::extract::extract;
use crate::engine_host::load::party_effects;
use crate::pbimport::transform::BaseSheet;
use hireling_engine::compute::compute;
use hireling_engine::model::EngineOutput;

/// The derived output for one roster character, from the character's own
/// stored row (`base_sheet` + live `level_adjust`) and the party's effects.
/// Source names resolve from each party member's `base_sheet.identity.name`
/// — the engine never sees the storage (FR-2 boundary).
///
/// # Errors
///
/// A missing character, an unparseable stored `base_sheet`, or any database
/// failure — corrupt state fails loudly, it never degrades a derived total.
pub async fn recompute_character(pool: &PgPool, character_id: i64) -> anyhow::Result<EngineOutput> {
    let row: Option<(i64, JsonValue, i32)> = sqlx::query_as(
        "SELECT c.party_id, c.base_sheet, v.level_adjust \
         FROM characters c JOIN character_vitals v ON v.character_id = c.id \
         WHERE c.id = $1",
    )
    .bind(character_id)
    .fetch_optional(pool)
    .await
    .context("recompute: character lookup")?;
    let Some((party_id, sheet_json, level_adjust)) = row else {
        anyhow::bail!("recompute: character {character_id} does not exist");
    };
    let sheet: BaseSheet = serde_json::from_value(sheet_json)
        .context("recompute: stored base_sheet does not parse")?;
    let base = extract(&sheet, i64::from(level_adjust));

    let effects = party_effects(pool, party_id).await?;
    let source_names = party_source_names(pool, party_id).await?;
    Ok(compute(character_id, &base, &effects, &source_names))
}

/// The derived output for every roster character, id order — the catch-up
/// snapshot's `derived` array (wire-protocol §8, E8 extension).
///
/// # Errors
///
/// Whatever [`recompute_character`] raises; a party member that fails
/// fails the whole snapshot — a partial derived array would look complete.
pub async fn party_derived(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<EngineOutput>> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM characters WHERE party_id = $1 ORDER BY id")
            .bind(party_id)
            .fetch_all(pool)
            .await
            .context("recompute: roster lookup")?;
    let mut outputs = Vec::with_capacity(ids.len());
    for character_id in ids {
        outputs.push(recompute_character(pool, character_id).await?);
    }
    Ok(outputs)
}

/// Each party member's display name (`base_sheet.identity.name`), keyed by
/// character id — the chip source names. A member with no name in the sheet
/// carries an empty string: degraded display, never a wrong name.
async fn party_source_names(pool: &PgPool, party_id: i64) -> anyhow::Result<BTreeMap<i64, String>> {
    let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, base_sheet->'identity'->>'name' FROM characters WHERE party_id = $1",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("recompute: source name lookup")?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| (id, name.unwrap_or_default()))
        .collect())
}
