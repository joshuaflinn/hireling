//! The book-value read (E12 plan Task 5, spec FR-6, design D5): one
//! session-gated, party-scoped REST read that resolves an item name to the
//! corpus's stored price for the sell prompt. The E9 precedent
//! (`GET /api/parties/{id}/conditions`) is the shape: party-readable (any
//! member or the GM), writes nothing, no write row in the ownership matrix.
//!
//! The route **reports, never converts**: a found price echoes the corpus
//! coin object verbatim — pp included. The pp→gp exchange is the sell
//! apply's credit site (plan Task 3), the one place a rate may be quoted,
//! per rules-exactness. Resolution is deliberately unforgiving (D5): the
//! match set is every imported-or-custom item row whose name equals the
//! query case-insensitively; reduce it to *distinct non-empty coin objects*
//! — exactly one resolves, zero, several, missing or empty prices all read
//! as "no book value". An unpriced duplicate drops out of the reduction; it
//! neither poisons a priced match nor cures a disagreement. There is no
//! lane filter — price presence decides (custom rows carry no upstream doc
//! and so read as unpriced by the same rule).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use crate::auth::AuthState;
use crate::auth::error::Forbidden;
use crate::auth::middleware::SessionAccount;

/// The lookup query: `?name=<item>` — the corpus name to resolve.
#[derive(Debug, Deserialize)]
pub struct BookValueQuery {
    name: Option<String>,
}

/// The D5 reduction over one name's match set: each entry is the row's
/// `price.value` coin object, `None` when the price path is absent. The
/// single distinct non-empty coin object wins; anything else — no rows, no
/// priced row, two disagreeing rows — is "no book value".
fn resolve_price(matches: Vec<Option<JsonValue>>) -> Option<JsonValue> {
    let mut distinct: Vec<JsonValue> = Vec::new();
    for value in matches.into_iter().flatten() {
        // A coin object is a JSON object with at least one key: absent,
        // null, non-object, and empty all read as unpriced (the probed pack
        // ships one legacy doc with an empty `price.value` — foundry-packs
        // contract §5). An all-zero price is a real price, not an absence.
        let Some(object) = value.as_object() else {
            continue;
        };
        if object.is_empty() || distinct.iter().any(|seen| seen == &value) {
            continue;
        }
        distinct.push(value);
    }
    match distinct.len() {
        1 => distinct.pop(),
        _ => None,
    }
}

/// `GET /api/parties/{party_id}/items/book-value?name=<item>` — the sell
/// prompt's book-value prefill (spec FR-6). Session-gated by the router;
/// party-readable (member or GM) per request.
///
/// The stored price path runs through the importer's wrapper:
/// `data->'upstream'->'system'->'price'->'value'` — design D5's SQL sketch
/// wrote `data->'system'…` and omits the `upstream` hop that
/// `import::transform::row_payload` puts every imported doc under. The
/// discrepancy is recorded on the task and PR, not silently absorbed.
pub async fn resolve(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
    Query(query): Query<BookValueQuery>,
) -> Response {
    if !crate::engine_host::rest::party_readable(&auth.pool, &account, party_id).await {
        return Forbidden.into_response();
    }
    // The sell dialog always sends a name; a missing or empty one is a
    // wiring mistake worth naming (custom.rs `list`'s 400 shape), not a
    // silent "no book value".
    let Some(name) = query.name.filter(|name| !name.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"field": "name", "reason": "the item name to look up is required"})),
        )
            .into_response();
    };
    let rows: Vec<Option<JsonValue>> = sqlx::query_scalar(
        "SELECT data->'upstream'->'system'->'price'->'value' \
         FROM corpus_entries WHERE kind = 'item' AND lower(name) = lower($1)",
    )
    .bind(name)
    .fetch_all(&auth.pool)
    .await
    .unwrap_or_else(|error| {
        // The lookup is a prompt-time convenience (D5: the confirmed value
        // is what the QM entered) — a failed lookup degrades to "no book
        // value", loudly logged.
        tracing::error!(error = %error, party_id, "book-value query failed");
        Vec::new()
    });
    match resolve_price(rows) {
        Some(price) => Json(json!({"found": true, "price": price})).into_response(),
        None => Json(json!({"found": false})).into_response(),
    }
}

#[cfg(test)]
#[path = "tests/book_value.rs"]
mod tests;
