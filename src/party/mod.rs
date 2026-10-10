//! The party roster read (E10 design D1): every party character's
//! `me`-shape payload, party-scoped, for the session's resolved party.
//! One read; no write surface exists in this module — E3's authz stays
//! the only gate any write passes.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::middleware::SessionAccount;
use crate::pbimport::payload::character_payload;

/// Resolution rule (data-model §2): the caller's own character's party;
/// else the single POC party; else an honest 409 — never a guess.
///
/// Determinism (Thrane's condition C3): the own-character lookup carries
/// `ORDER BY id LIMIT 1`. `characters.owner_sub` is UNIQUE today (one
/// character per account, migration 1), so the clause is belt — the day
/// the schema allows two, this read still resolves deterministically
/// instead of surprising sqlx with two rows.
///
/// # Errors
///
/// `409` when no party can be resolved exactly (zero parties, or many for
/// a characterless caller — data-model §2); `500` surfaces nothing — the
/// database error is logged here and the handler answers an empty body.
async fn resolve_party(pool: &PgPool, caller_sub: &str) -> Result<i64, StatusCode> {
    let own: Option<i64> = sqlx::query_scalar(
        "SELECT party_id FROM characters WHERE owner_sub = $1 ORDER BY id LIMIT 1",
    )
    .bind(caller_sub)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, sub = %caller_sub, "party resolution failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(party_id) = own {
        return Ok(party_id);
    }
    // Characterless caller (GM or first-run player): exactly one party
    // resolves; zero or many is a 409 — the rule exists so it can never
    // silently pick (data-model §2).
    let parties: Vec<i64> = sqlx::query_scalar("SELECT id FROM parties ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, sub = %caller_sub, "party enumeration failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    match parties.as_slice() {
        [only] => Ok(*only),
        [] | [_, _, ..] => Err(StatusCode::CONFLICT),
    }
}

/// `GET /api/party/roster` — contracts/roster-rest.md. 200 with the
/// `{party_id, you, characters[]}` wrapper (an empty party is `[]`, never
/// 204); 401 sits in the middleware; 409 when resolution cannot resolve
/// exactly one party; 500 logged with no body (house error style).
pub async fn roster_read(State(auth): State<Arc<AuthState>>, account: SessionAccount) -> Response {
    let pool = &auth.pool;
    let party_id = match resolve_party(pool, &account.sub).await {
        Ok(id) => id,
        Err(status) => return status.into_response(),
    };
    let ids: Vec<i64> =
        match sqlx::query_scalar("SELECT id FROM characters WHERE party_id = $1 ORDER BY id")
            .bind(party_id)
            .fetch_all(pool)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(error = %error, party_id, "roster character query failed");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
    let mut characters = Vec::with_capacity(ids.len());
    for id in ids {
        match character_payload(pool, id).await {
            Ok(payload) => characters.push(payload),
            Err(error) => {
                tracing::error!(error = %error, character_id = id, party_id, "roster payload failed");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    tracing::debug!(
        party_id,
        count = characters.len(),
        sub = %account.sub,
        "roster read served"
    );
    (
        StatusCode::OK,
        Json(json!({
            "party_id": party_id,
            "you": { "sub": account.sub, "role": account.role.as_str() },
            "characters": characters,
        })),
    )
        .into_response()
}
