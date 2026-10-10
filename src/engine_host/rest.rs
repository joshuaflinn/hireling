//! The read-only REST face of the engine (plan Task 9): the condition
//! picker (corpus rows, not code), the party's active effects, and one
//! character's derived bootstrap. Session-gated by the router's middleware;
//! party-readable (any member or the GM), write nothing — E3's matrix has
//! no write row here and the routes take no write path.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::auth::AuthState;
use crate::auth::error::Forbidden;
use crate::auth::middleware::SessionAccount;

/// The party-read gate: the account owns a character in the party or holds
/// the GM seat — the handshake rule, per request. Deny on database trouble
/// (a failed lookup admits nobody).
pub(crate) async fn party_readable(
    pool: &sqlx::PgPool,
    account: &SessionAccount,
    party_id: i64,
) -> bool {
    if account.role == crate::auth::authz::Role::Gm {
        return sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM parties WHERE id = $1)")
            .bind(party_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM characters WHERE party_id = $1 AND owner_sub = $2)",
    )
    .bind(party_id)
    .bind(&account.sub)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// One corpus row as the picker reads it.
type PickerRow = (i64, String, String, Option<String>, Option<JsonValue>);

/// One picker row: a corpus condition as the UI may offer it. `valued` is
/// READ from the stored mappings (any `condition_value` row), never from a
/// name list — the corpus data rules (FR-8).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ConditionPick {
    pub corpus_entry_id: i64,
    pub name: String,
    /// `engine_math` or `display_only` — the importer's stored verdict.
    pub tier: String,
    /// The lane the row came in on (`core`, `imported`, `custom`).
    pub lane: String,
    pub valued: bool,
}

/// `GET /api/parties/{party_id}/conditions` — the condition picker, one row
/// per corpus condition. Global corpus, offered per party (the route's party
/// segment is the session's scope gate).
pub async fn conditions(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
) -> Response {
    if !party_readable(&auth.pool, &account, party_id).await {
        return Forbidden.into_response();
    }
    let rows: Vec<PickerRow> = sqlx::query_as(
        "SELECT id, name, lane, data->'import'->>'tier', modifiers \
         FROM corpus_entries WHERE kind = 'condition' ORDER BY name, id",
    )
    .fetch_all(&auth.pool)
    .await
    .unwrap_or_else(|error| {
        tracing::error!(error = %error, party_id, "condition picker query failed");
        Vec::new()
    });
    let picks: Vec<ConditionPick> = rows
        .into_iter()
        .map(|(corpus_entry_id, name, lane, tier, modifiers)| {
            let valued = modifiers
                .as_ref()
                .and_then(JsonValue::as_array)
                .is_some_and(|mappings| {
                    mappings.iter().any(|row| {
                        row.get("value_kind").and_then(JsonValue::as_str) == Some("condition_value")
                    })
                });
            ConditionPick {
                corpus_entry_id,
                name,
                tier: tier.unwrap_or_else(|| "display_only".to_owned()),
                lane,
                valued,
            }
        })
        .collect();
    Json(picks).into_response()
}

/// `GET /api/parties/{party_id}/effects` — the party's active effects, all
/// sources, whole-row shape (the snapshot's effect value plus the id and
/// version the client needs to address them).
pub async fn effects(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
) -> Response {
    if !party_readable(&auth.pool, &account, party_id).await {
        return Forbidden.into_response();
    }
    match crate::engine_host::load::party_effects(&auth.pool, party_id).await {
        Ok(all) => {
            let active: Vec<_> = all.into_iter().filter(|effect| effect.active).collect();
            Json(active).into_response()
        }
        Err(error) => {
            tracing::error!(error = %error, party_id, "effects read failed");
            Forbidden.into_response()
        }
    }
}

/// `GET /api/characters/{character_id}/derived` — one character's
/// [`EngineOutput`] as the sheet's bootstrap. Party-readable: the requester
/// must sit in the character's own party (or be the GM).
pub async fn derived(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(character_id): Path<i64>,
) -> Response {
    let party_id: Option<i64> = sqlx::query_scalar("SELECT party_id FROM characters WHERE id = $1")
        .bind(character_id)
        .fetch_optional(&auth.pool)
        .await
        .unwrap_or(None);
    let Some(party_id) = party_id else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    if !party_readable(&auth.pool, &account, party_id).await {
        return Forbidden.into_response();
    }
    match crate::engine_host::recompute::recompute_character(&auth.pool, character_id).await {
        Ok(output) => Json(output).into_response(),
        Err(error) => {
            tracing::error!(error = %error, character_id, "derived read failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
