//! The axum handlers for the import surface (FR-15/FR-17, design §3).
//!
//! `POST /api/characters/import` takes the export JSON verbatim as the
//! request body — one endpoint serves paste and upload. It sits inside E3's
//! protected nest (`resolve_session` → `require_auth` → `gm_read_only`), so
//! GM write attempts are rejected and audited by the middleware before this
//! handler runs; the handler re-checks the seat belt-and-braces and scopes
//! everything to the session's `sub` — there is no character-id to forge.
//!
//! `GET /api/characters/me` returns the caller's own character (204 when
//! none): identity summary, base sheet, vitals, slot rows, inventory rows.
//!
//! Shell layer: all logic lives in the pure modules and the store.

use axum::Json;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sqlx::Row as _;
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::authz::{Action, Actor, Resource};
use crate::auth::error::{Forbidden, Unauthenticated};
use crate::auth::middleware::SessionAccount;
use crate::pbimport::bulk;
use crate::pbimport::error::ImportError;
use crate::pbimport::store::{RunImportError, record_rejection, run_import};

/// `POST /api/characters/import` — the import endpoint (FR-1, FR-17).
pub async fn import_character(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    headers: HeaderMap,
    body: String,
) -> Response {
    if account.role != crate::auth::authz::Role::Player {
        // Belt-and-braces: gm_read_only already rejects GM writes upstream;
        // deny-by-default if a future refactor ever reorders the layers.
        return Forbidden.into_response();
    }
    let actor = Actor {
        sub: account.sub.clone(),
        role: account.role,
    };
    let request_id = headers
        .get(crate::http::REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    match run_import(&auth.pool, &actor, request_id.as_deref(), &body).await {
        Ok(outcome) => (StatusCode::OK, Json(outcome)).into_response(),
        Err(RunImportError::Invalid(failure)) => {
            // One audit row per attempt (E5 guardrail: log import attempts
            // with account + outcome) — the denial lands even though the
            // body never reached storage.
            record_rejection(&auth.pool, &actor, request_id.as_deref(), failure).await;
            failure_response(failure)
        }
        Err(RunImportError::Database(error)) => {
            tracing::error!(
                error = %error,
                sub = %account.sub,
                request_id = request_id.as_deref().unwrap_or(""),
                "import failed on the database; rolled back"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(RunImportError::Invariant(what)) => {
            tracing::error!(
                invariant = %what,
                sub = %account.sub,
                request_id = request_id.as_deref().unwrap_or(""),
                "import hit a broken schema invariant"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The contract §4 failure payload: E3's standard envelope shape carrying
/// the class status, code, and the exact verbatim message.
fn failure_response(failure: ImportError) -> Response {
    let status = StatusCode::from_u16(failure.status()).unwrap_or(StatusCode::BAD_REQUEST);
    let body = serde_json::json!({
        "error": {
            "code": failure.code(),
            "message": failure.message(),
        }
    });
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// `GET /api/characters/me` — the caller's own character, self-scoped
/// (design §3). The ownership matrix reads this route: a read by an
/// authenticated account is allowed; only the owner's data is reachable.
pub async fn me(State(auth): State<Arc<AuthState>>, account: SessionAccount) -> Response {
    let actor = Actor {
        sub: account.sub.clone(),
        role: account.role,
    };
    if crate::auth::authz::authorize(
        &actor,
        Action::Read,
        &Resource::Character {
            owner_sub: account.sub.clone(),
        },
    ) == crate::auth::authz::Verdict::Deny
    {
        // Unreachable for a self-owned resource — deny-by-default posture.
        return Unauthenticated.into_response();
    }
    match load_me(&auth.pool, &account.sub).await {
        Ok(Some(payload)) => (StatusCode::OK, Json(payload)).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::error!(error = %error, sub = %account.sub, "failed to load character");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The self-scoped read: summary + `base_sheet` + vitals + slots + inventory.
async fn load_me(pool: &sqlx::PgPool, sub: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let Some(row) =
        sqlx::query("SELECT id, owner_sub, base_sheet FROM characters WHERE owner_sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let character_id: i64 = row.get("id");
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

    Ok(Some(serde_json::json!({
        "character": summary,
        "base_sheet": base_sheet,
        "vitals": vitals,
        "slots": slots,
        "inventory": inventory,
        "item_bulk": item_bulk,
        "item_traits": item_traits,
    })))
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;

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
    // Equipment AND weapons both reach the corpus: strike rows read their
    // trait chips through `item_traits`. Both sections are arrays of
    // objects carrying `name`. Armor has no chip consumer yet and stays
    // out until one exists.
    let mut item_names: Vec<String> = Vec::new();
    for section in ["equipment", "weapons"] {
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
