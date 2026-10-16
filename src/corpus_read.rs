//! Read-only corpus lookups for the Dave-parity surfaces (specs/015): the
//! Reference column (actions, conditions), detail popups (spells, feats,
//! class features, actions, items), curriculum resolution, and the summons
//! browser (creatures). Game data is the Foundry pack as imported — nothing
//! here is hardcoded (Josh, 2026-10-15).
//!
//! Session-gated by the protected nest; reads are global corpus, so any
//! signed-in account may read (the corpus is not party data). Every route is
//! GET and writes nothing.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::auth::AuthState;
use crate::auth::middleware::SessionAccount;

/// Kinds this API serves (the stored `corpus_entries.kind` values).
const READABLE_KINDS: &[&str] = &[
    "condition",
    "item",
    "spell",
    "feat",
    "class_feature",
    "action",
    "creature",
    "rules_page",
];

/// Most names one lookup may ask for — a sheet resolves its own spells,
/// feats and items, never the whole corpus.
const MAX_NAMES: usize = 200;

/// One corpus entry, trimmed to what the UI renders. `detail` carries the
/// kind-specific display fields; `description` is the raw Foundry HTML
/// (enriched + made inert client-side, never `{@html}`).
#[derive(Debug, Serialize)]
pub struct Entry {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub lane: String,
    pub source_id: Option<String>,
    pub description: String,
    pub traits: Vec<String>,
    pub rarity: Option<String>,
    pub level: Option<i64>,
    pub publication: Option<String>,
    pub detail: Value,
}

#[derive(Debug, Deserialize)]
pub struct NamesQuery {
    /// Comma-separated exact names (case-insensitive).
    #[serde(default)]
    pub names: String,
    /// Comma-separated upstream `_id`s (curriculum links carry these).
    #[serde(default)]
    pub ids: String,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Publication title filter (exact), e.g. `Pathfinder Player Core`.
    pub publication: Option<String>,
    /// Required trait (lowercase slug), e.g. `undead`.
    #[serde(rename = "trait")]
    pub trait_: Option<String>,
    /// Inclusive level bounds (spells: rank; creatures/feats/items: level).
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    /// Case-insensitive name substring.
    pub q: Option<String>,
}

fn kind_ok(kind: &str) -> bool {
    READABLE_KINDS.contains(&kind)
}

type Row = (i64, String, String, String, Option<String>, Value);

/// `GET /api/corpus/{kind}?names=a,b&ids=x,y` — entries by exact name
/// (case-insensitive) or upstream id. Imported rows win over custom rows of
/// the same name (custom cannot shadow the corpus).
pub async fn lookup(
    State(auth): State<Arc<AuthState>>,
    _account: SessionAccount,
    Path(kind): Path<String>,
    Query(query): Query<NamesQuery>,
) -> Response {
    if !kind_ok(&kind) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let names: Vec<String> = split_list(&query.names)
        .into_iter()
        .map(|name| name.to_lowercase())
        .collect();
    let ids = split_list(&query.ids);
    if names.len() + ids.len() > MAX_NAMES {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"code": "too_many", "message": format!("at most {MAX_NAMES} names")}})),
        )
            .into_response();
    }
    if names.is_empty() && ids.is_empty() {
        return Json(Vec::<Entry>::new()).into_response();
    }
    let rows: Result<Vec<Row>, _> = sqlx::query_as(
        "SELECT id, kind, name, lane, source_id, data FROM corpus_entries \
         WHERE kind = $1 AND (lower(name) = ANY($2) OR source_id = ANY($3)) \
         ORDER BY (lane = 'custom'), name, id",
    )
    .bind(&kind)
    .bind(&names)
    .bind(&ids)
    .fetch_all(&auth.pool)
    .await;
    respond(rows, &kind)
}

/// `GET /api/corpus/{kind}/list?publication=&trait=&min_level=&max_level=&q=`
/// — a filtered browse list (the Reference column, the summons browser).
pub async fn list(
    State(auth): State<Arc<AuthState>>,
    _account: SessionAccount,
    Path(kind): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    if !kind_ok(&kind) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Level lives at system.level.value, or system.details.level.value for
    // creatures — coalesce covers both without dynamic SQL.
    let rows: Result<Vec<Row>, _> = sqlx::query_as(
        "SELECT id, kind, name, lane, source_id, data FROM corpus_entries \
         WHERE kind = $1 AND lane <> 'custom' \
           AND ($2::text IS NULL OR coalesce(data->'upstream'->'system'->'publication'->>'title', \
                data->'upstream'->'system'->'details'->'publication'->>'title') = $2) \
           AND ($3::text IS NULL OR data->'upstream'->'system'->'traits'->'value' ? $3) \
           AND ($4::bigint IS NULL OR coalesce(data->'upstream'->'system'->'level'->>'value', \
                data->'upstream'->'system'->'details'->'level'->>'value')::bigint >= $4) \
           AND ($5::bigint IS NULL OR coalesce(data->'upstream'->'system'->'level'->>'value', \
                data->'upstream'->'system'->'details'->'level'->>'value')::bigint <= $5) \
           AND ($6::text IS NULL OR name ILIKE '%' || $6 || '%') \
         ORDER BY name, id LIMIT 500",
    )
    .bind(&kind)
    .bind(query.publication.as_deref())
    .bind(query.trait_.as_deref().map(str::to_lowercase))
    .bind(query.min_level)
    .bind(query.max_level)
    .bind(query.q.as_deref())
    .fetch_all(&auth.pool)
    .await;
    respond(rows, &kind)
}

fn respond(rows: Result<Vec<Row>, sqlx::Error>, kind: &str) -> Response {
    match rows {
        Ok(rows) => Json(rows.into_iter().map(entry_of).collect::<Vec<_>>()).into_response(),
        Err(error) => {
            tracing::error!(error = %error, kind, "corpus read failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Shape one stored row for the UI.
fn entry_of((id, kind, name, lane, source_id, data): Row) -> Entry {
    let upstream = data.get("upstream").cloned().unwrap_or(Value::Null);
    let system = upstream.get("system").cloned().unwrap_or(Value::Null);
    let custom = data.get("custom");
    let description = custom
        .and_then(|c| c.get("description"))
        .and_then(Value::as_str)
        .map(|text| format!("<p>{}</p>", escape(text)))
        .or_else(|| {
            system
                .pointer("/description/value")
                .or_else(|| system.pointer("/details/publicNotes"))
                .or_else(|| upstream.pointer("/text/content"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let traits = system
        .pointer("/traits/value")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let rarity = system
        .pointer("/traits/rarity")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let level = system
        .pointer("/level/value")
        .or_else(|| system.pointer("/details/level/value"))
        .and_then(Value::as_i64);
    let publication = system
        .pointer("/publication/title")
        .or_else(|| system.pointer("/details/publication/title"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let detail = detail_of(&kind, &system);
    Entry {
        id,
        kind,
        name,
        lane,
        source_id,
        description,
        traits,
        rarity,
        level,
        publication,
        detail,
    }
}

/// Kind-specific display fields — the stat lines Dave's popups show.
fn detail_of(kind: &str, system: &Value) -> Value {
    let pick = |pointer: &str| system.pointer(pointer).cloned().unwrap_or(Value::Null);
    match kind {
        "spell" => json!({
            "time": pick("/time/value"),
            "range": pick("/range/value"),
            "area": pick("/area"),
            "target": pick("/target/value"),
            "duration": pick("/duration"),
            "defense": pick("/defense"),
            "traditions": pick("/traits/traditions"),
            "damage": pick("/damage"),
            "heightening": pick("/heightening"),
            "cost": pick("/cost/value"),
        }),
        "feat" | "class_feature" => json!({
            "action_type": pick("/actionType/value"),
            "actions": pick("/actions/value"),
            "category": pick("/category"),
            "prerequisites": pick("/prerequisites/value"),
            "frequency": pick("/frequency"),
            "trigger": pick("/trigger/value"),
            "requirements": pick("/requirements/value"),
        }),
        "action" => json!({
            "action_type": pick("/actionType/value"),
            "actions": pick("/actions/value"),
            "category": pick("/category"),
            "slug": pick("/slug"),
        }),
        "item" => json!({
            "type": pick("/category"),
            "bulk": pick("/bulk/value"),
            "price": pick("/price/value"),
            "usage": pick("/usage/value"),
            "damage": pick("/damage"),
            "ac_bonus": pick("/acBonus"),
            "dex_cap": pick("/dexCap"),
            "hardness": pick("/hardness"),
            "hp": pick("/hp"),
        }),
        "creature" => json!({
            "size": pick("/traits/size/value"),
            "ac": pick("/attributes/ac/value"),
            "hp": pick("/attributes/hp/max"),
            "speed": pick("/attributes/speed"),
            "perception": pick("/perception/mod"),
            "saves": pick("/saves"),
            "abilities": pick("/abilities"),
            "skills": pick("/skills"),
            "immunities": pick("/attributes/immunities"),
            "weaknesses": pick("/attributes/weaknesses"),
            "resistances": pick("/attributes/resistances"),
            "languages": pick("/details/languages"),
            "blurb": pick("/details/blurb"),
        }),
        _ => Value::Null,
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `GET /api/corpus/creature/{id}/items` — a creature's embedded items
/// (strikes, actions, spells), for the summons stat card. Creatures carry
/// them in the upstream document's `items` array.
pub async fn creature_items(
    State(auth): State<Arc<AuthState>>,
    _account: SessionAccount,
    Path(id): Path<i64>,
) -> Response {
    let row: Result<Option<(Value,)>, _> = sqlx::query_as(
        "SELECT data->'upstream'->'items' FROM corpus_entries WHERE id = $1 AND kind = 'creature'",
    )
    .bind(id)
    .fetch_optional(&auth.pool)
    .await;
    match row {
        Ok(Some((items,))) => {
            let trimmed: Vec<Value> = items
                .as_array()
                .map(|all| {
                    all.iter()
                        .map(|item| {
                            json!({
                                "name": item.get("name"),
                                "type": item.get("type"),
                                "system": {
                                    "description": item.pointer("/system/description/value"),
                                    "actionType": item.pointer("/system/actionType/value"),
                                    "actions": item.pointer("/system/actions/value"),
                                    "bonus": item.pointer("/system/bonus/value"),
                                    "damageRolls": item.pointer("/system/damageRolls"),
                                    "traits": item.pointer("/system/traits/value"),
                                    "level": item.pointer("/system/level/value"),
                                }
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Json(trimmed).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(error = %error, id, "creature items read failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_list_trims_and_drops_empties() {
        assert_eq!(split_list(" a, ,b ,"), vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn unknown_kinds_are_not_readable() {
        assert!(kind_ok("spell"));
        assert!(!kind_ok("accounts"));
    }

    #[test]
    fn custom_description_is_escaped_not_trusted() {
        let entry = entry_of((
            1,
            "spell".into(),
            "X".into(),
            "custom".into(),
            None,
            json!({"custom": {"description": "<script>x</script>"}}),
        ));
        assert_eq!(entry.description, "<p>&lt;script&gt;x&lt;/script&gt;</p>");
    }
}
