//! The per-character bootstrap payload (E5's `me` shape, extracted E10
//! plan Task 1): summary + `base_sheet` + vitals + slots + inventory +
//! item corpus. One assembly, two callers — `GET /api/characters/me` (the
//! one-character special case) and `GET /api/party/roster` (every party
//! character). The shape is pinned by both handlers' router tests; if it
//! changes by PR, `specs/009-party-view-gm-seat-pwa/contracts/roster-rest.md`
//! changes in the same PR.

use sqlx::Row as _;

use crate::pbimport::bulk;

/// Assemble the `me`-shape payload for one character row id. Character-
/// keyed queries only — the caller owns any scoping (owner sub for `me`,
/// party id for the roster).
///
/// # Errors
///
/// Propagates sqlx failures (a missing row, a broken schema) to the
/// caller, which answers the house error style: log + 500, no body.
pub async fn character_payload(
    pool: &sqlx::PgPool,
    character_id: i64,
) -> Result<serde_json::Value, sqlx::Error> {
    let row = sqlx::query("SELECT id, owner_sub, base_sheet FROM characters WHERE id = $1")
        .bind(character_id)
        .fetch_one(pool)
        .await?;
    let base_sheet: serde_json::Value = row.get("base_sheet");
    // The identity summary rides the sheet (E2 stores no name columns).
    let identity = base_sheet
        .get("identity")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let summary = serde_json::json!({
        "id": character_id,
        "name": identity.get("name").cloned().unwrap_or(serde_json::Value::Null),
        "level": identity.get("level").cloned().unwrap_or(serde_json::Value::Null),
        "class": identity.get("class").cloned().unwrap_or(serde_json::Value::Null),
        "owner": row.get::<String, _>("owner_sub"),
    });
    let vitals = sqlx::query(
        "SELECT hp, temp_hp, money_gp, money_sp, money_cp, money_pp, level_adjust, \
                focus_current, hero_points, daily \
         FROM character_vitals WHERE character_id = $1",
    )
    .bind(character_id)
    .fetch_optional(pool)
    .await?
    .map(|vitals| {
        serde_json::json!({
            "hp": vitals.get::<i32, _>("hp"),
            "temp_hp": vitals.get::<i32, _>("temp_hp"),
            "money_gp": vitals.get::<i32, _>("money_gp"),
            "money_sp": vitals.get::<i32, _>("money_sp"),
            "money_cp": vitals.get::<i32, _>("money_cp"),
            "money_pp": vitals.get::<i32, _>("money_pp"),
            "level_adjust": vitals.get::<i32, _>("level_adjust"),
            "focus_current": vitals.get::<i32, _>("focus_current"),
            "hero_points": vitals.get::<i32, _>("hero_points"),
            "daily": vitals.get::<serde_json::Value, _>("daily"),
        })
    });
    let slots = sqlx::query(
        "SELECT caster_key, rank, slot_index, used, prepared_spell \
         FROM character_spell_slots WHERE character_id = $1 \
         ORDER BY caster_key, rank, slot_index",
    )
    .bind(character_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|slot| {
        serde_json::json!({
            "caster_key": slot.get::<String, _>("caster_key"),
            "rank": slot.get::<i32, _>("rank"),
            "slot_index": slot.get::<i32, _>("slot_index"),
            "used": slot.get::<bool, _>("used"),
            "prepared_spell": slot.get::<Option<String>, _>("prepared_spell"),
        })
    })
    .collect::<Vec<_>>();
    let inventory = sqlx::query(
        "SELECT item_name, qty_delta FROM character_inventory_live WHERE character_id = $1 \
         ORDER BY item_name",
    )
    .bind(character_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|item| {
        serde_json::json!({
            "name": item.get::<String, _>("item_name"),
            "qty_delta": item.get::<i32, _>("qty_delta"),
        })
    })
    .collect::<Vec<_>>();

    let (item_bulk, item_traits) = load_item_corpus(pool, character_id, &base_sheet).await?;

    Ok(serde_json::json!({
        "character": summary,
        "base_sheet": base_sheet,
        "vitals": vitals,
        "slots": slots,
        "inventory": inventory,
        "item_bulk": item_bulk,
        "item_traits": item_traits,
    }))
}

/// The item-bulk map (E6 design §5): exact-name, case-insensitive
/// resolution of the sheet's item names against the items corpus, in
/// tenths of Bulk. Gaps ride as null and the misses are logged with item
/// name and character id for E4 follow-up — a miss degrades display,
/// never blocks the bootstrap.
async fn load_item_corpus(
    pool: &sqlx::PgPool,
    character_id: i64,
    base_sheet: &serde_json::Value,
) -> Result<
    (
        std::collections::BTreeMap<String, Option<i64>>,
        std::collections::BTreeMap<String, Vec<String>>,
    ),
    sqlx::Error,
> {
    // Equipment, weapons, AND armor all reach the corpus: strike rows read
    // their trait chips through `item_traits`, and the inventory total
    // folds weapon and worn-armor bulk through `item_bulk`. All sections
    // are arrays of objects carrying `name`.
    let mut item_names: Vec<String> = Vec::new();
    for section in ["equipment", "weapons", "armor"] {
        if let Some(items) = base_sheet
            .get(section)
            .and_then(serde_json::Value::as_array)
        {
            for item in items {
                if let Some(name) = item.get("name").and_then(serde_json::Value::as_str) {
                    item_names.push(name.to_owned());
                }
            }
        }
    }
    item_names.sort();
    item_names.dedup();
    let rows: Vec<(String, Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT lower(name), data->'system'->'bulk'->>'value', \
                data->'system'->'traits'->'value' \
         FROM corpus_entries WHERE kind = 'item' AND lower(name) = ANY($1)",
    )
    .bind(
        item_names
            .iter()
            .map(|name| name.to_lowercase())
            .collect::<Vec<_>>(),
    )
    .fetch_all(pool)
    .await?;
    let corpus_rows: Vec<(String, Option<f64>)> = rows
        .iter()
        .map(|(name, raw, _)| {
            (
                name.clone(),
                raw.as_ref().and_then(|raw| raw.parse::<f64>().ok()),
            )
        })
        .collect();
    let trait_rows: Vec<bulk::TraitRow> = rows
        .iter()
        .map(|(name, _, traits)| {
            (
                name.clone(),
                traits
                    .as_ref()
                    .and_then(serde_json::Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect();
    let item_bulk = bulk::item_bulk_map(&item_names, &corpus_rows);
    let item_traits = bulk::item_trait_map(&item_names, &trait_rows);
    let misses: Vec<&str> = item_names
        .iter()
        .map(String::as_str)
        .filter(|name| !item_bulk.get(*name).is_some_and(Option::is_some))
        .filter(|name| {
            !corpus_rows
                .iter()
                .any(|(candidate, _)| candidate == &name.to_lowercase())
        })
        .collect();
    if !misses.is_empty() {
        tracing::info!(
            character_id,
            missed = %misses.join(", "),
            "item bulk unresolved at bootstrap; corpus gaps render as em-dash"
        );
    }
    Ok((item_bulk, item_traits))
}
