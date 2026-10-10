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
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::authz::{Action, Actor, Resource};
use crate::auth::error::{Forbidden, Unauthenticated};
use crate::auth::middleware::SessionAccount;
use crate::pbimport::error::ImportError;
use crate::pbimport::payload::character_payload;
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

/// The self-scoped read: the caller's one character via the shared
/// payload assembly (E10 plan Task 1 — `me` is the roster's one-character
/// special case). Shape pinned by the router tests on both surfaces.
async fn load_me(pool: &sqlx::PgPool, sub: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let Some(character_id) = owner_character_id(pool, sub).await? else {
        return Ok(None);
    };
    character_payload(pool, character_id).await.map(Some)
}

/// The owner-sub lookup: the one query that scopes `me` to its caller.
async fn owner_character_id(pool: &sqlx::PgPool, sub: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM characters WHERE owner_sub = $1")
        .bind(sub)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
