//! The custom-content REST face (E9): party members put their own rows into
//! the corpus at the point of use — an item in the inventory, a spell at the
//! composer, a condition at the picker. Contract:
//! `specs/010-rules-tooltips-custom-content/contracts/custom-rows-rest.md`.
//!
//! Shape of the law here: custom rows are `corpus_entries` rows with
//! `lane = 'custom'`, `source_id = NULL` (the structural importer-isolation
//! guarantee, FR-8), `modifiers = NULL` (zero engine math, FR-6), and a
//! `data.custom` block holding `description` and optional `value_or_rank`
//! (the writer contract E2's migration 4 delegates to this epic). Reads ride
//! existing surfaces — the picker (`GET /conditions`) and the kind list
//! here; ownership gates writes only (FR-5): any party member creates, the
//! creator is the sole editor, and every refusal that reaches this module
//! persists a `forbidden_custom_write` audit row (the GM's refusals never do
//! reach it — `gm_read_only` intercepts first and audits `forbidden_gm_write`).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use sqlx::PgPool;

use crate::auth::AuthState;
use crate::auth::audit::{self, AuditEvent, AuditOutcome};
use crate::auth::authz::{self, Action, Actor, Resource};
use crate::auth::error::Forbidden;
use crate::auth::middleware::SessionAccount;
use crate::http::REQUEST_ID_HEADER;

/// The caps (contract §1). The web client mirrors these bounds and reasons;
/// the server re-validates everything (defense in depth).
const NAME_CAP_CHARS: usize = 64;
const DESCRIPTION_CAP_CHARS: usize = 280;
const SPELL_RANK_RANGE: std::ops::RangeInclusive<i64> = 0..=10;
const CONDITION_VALUE_RANGE: std::ops::RangeInclusive<i64> = 1..=20;

/// A validation refusal, pre-wire: the field and the bound. Small and
/// copyable on purpose — it rides the `Err` side of every validator here,
/// and only [`Rejection::respond`] touches the wire shape.
struct Rejection {
    field: String,
    reason: &'static str,
}

impl Rejection {
    fn new(field: &str, reason: &'static str) -> Self {
        Self {
            field: field.to_owned(),
            reason,
        }
    }

    /// The contract's 400 shape: the field and the bound, nothing else.
    fn respond(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "validation", "field": self.field, "reason": self.reason})),
        )
            .into_response()
    }
}

/// The kinds a custom row can take. Deliberately not the importer's
/// [`crate::import::model::Kind`]: that pair is the imported lane's; this
/// one includes spells and is the create path's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Item,
    Spell,
    Condition,
}

impl Kind {
    fn parse(raw: &str) -> Result<Self, Rejection> {
        match raw {
            "item" => Ok(Self::Item),
            "spell" => Ok(Self::Spell),
            "condition" => Ok(Self::Condition),
            _ => Err(Rejection::new("kind", "item, spell, or condition")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Spell => "spell",
            Self::Condition => "condition",
        }
    }
}

/// The request body's fields, extracted by hand: a derive-based extractor
/// would answer type mismatches with axum's 422 text body, bypassing the
/// contract's 400-with-field-and-reason shape. Unknown fields are refused —
/// the body is a closed set (`kind`, `name`, `description`,
/// `value_or_rank`), and a silent ignore would let a typo'd field look
/// accepted.
#[derive(Debug, Default)]
struct Fields {
    kind: Option<String>,
    name: Option<String>,
    description: Option<String>,
    value_or_rank: Option<i64>,
    /// An explicit `null` counts as provided-and-absent: a spell rank
    /// cannot be cleared (it is required), a condition's value can.
    value_provided: bool,
}

fn extract_fields(body: &JsonValue) -> Result<Fields, Rejection> {
    let Some(map) = body.as_object() else {
        return Err(Rejection::new("body", "a JSON object"));
    };
    let mut fields = Fields::default();
    for (key, value) in map {
        match key.as_str() {
            "kind" => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| Rejection::new("kind", "a string"))?;
                fields.kind = Some(raw.to_owned());
            }
            "name" => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| Rejection::new("name", "a string"))?;
                fields.name = Some(raw.to_owned());
            }
            "description" => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| Rejection::new("description", "a string"))?;
                fields.description = Some(raw.to_owned());
            }
            "value_or_rank" => {
                fields.value_provided = true;
                if value.is_null() {
                    continue;
                }
                let Some(raw) = value.as_i64() else {
                    return Err(Rejection::new("value_or_rank", "an integer"));
                };
                fields.value_or_rank = Some(raw);
            }
            unknown => return Err(Rejection::new(unknown, "unknown field")),
        }
    }
    Ok(fields)
}

/// The name: trimmed, then 1..=64 characters. The trim comes first —
/// padding is not length.
fn trimmed_name(raw: Option<&str>) -> Result<String, Rejection> {
    let trimmed = raw.unwrap_or_default().trim();
    if trimmed.is_empty() || trimmed.chars().count() > NAME_CAP_CHARS {
        return Err(Rejection::new("name", "1..64 characters after trim"));
    }
    Ok(trimmed.to_owned())
}

/// The description: trimmed, then 0..=280 characters.
fn trimmed_description(raw: Option<&str>) -> Result<String, Rejection> {
    let trimmed = raw.unwrap_or_default().trim();
    if trimmed.chars().count() > DESCRIPTION_CAP_CHARS {
        return Err(Rejection::new(
            "description",
            "0..280 characters after trim",
        ));
    }
    Ok(trimmed.to_owned())
}

/// The per-kind `value_or_rank` law (contract §1): items carry none, spell
/// ranks are required 0..=10, condition values are optional 1..=20 (a
/// display note — never an apply input, FR-6).
fn validate_value(kind: Kind, value: Option<i64>) -> Result<Option<i64>, Rejection> {
    match kind {
        Kind::Item => {
            if value.is_some() {
                return Err(Rejection::new("value_or_rank", "not allowed for items"));
            }
            Ok(None)
        }
        Kind::Spell => match value {
            None => Err(Rejection::new("value_or_rank", "required for spells")),
            Some(rank) if SPELL_RANK_RANGE.contains(&rank) => Ok(Some(rank)),
            Some(_) => Err(Rejection::new("value_or_rank", "0..10 for spells")),
        },
        Kind::Condition => match value {
            None => Ok(None),
            Some(value) if CONDITION_VALUE_RANGE.contains(&value) => Ok(Some(value)),
            Some(_) => Err(Rejection::new("value_or_rank", "1..20 for conditions")),
        },
    }
}

/// The `data.custom` block this module owns (the E9 writer contract): plain
/// text description plus the optional value — no `import` block ever, so the
/// picker's NULL-tier default reads the row as display-only.
fn custom_block(description: &str, value_or_rank: Option<i64>) -> JsonValue {
    let mut block = serde_json::Map::new();
    block.insert("description".to_owned(), json!(description));
    if let Some(value) = value_or_rank {
        block.insert("value_or_rank".to_owned(), json!(value));
    }
    json!({ "custom": block })
}

/// The row as the client renders it — create answers 201, edit 200 (the
/// contract fixes both), the body identical.
fn row_response(
    status: StatusCode,
    corpus_entry_id: i64,
    kind: &str,
    name: &str,
    description: &str,
    value_or_rank: Option<i64>,
    created_by_sub: &str,
) -> Response {
    (
        status,
        Json(json!({
            "corpus_entry_id": corpus_entry_id,
            "kind": kind,
            "name": name,
            "lane": "custom",
            "description": description,
            "value_or_rank": value_or_rank,
            "created_by_sub": created_by_sub,
        })),
    )
        .into_response()
}

/// The caller's character in `{party_id}` — the create gate's membership
/// fact. `None` for the GM too (the GM owns no character), so the read-only
/// seat falls out of the same check (FR-5).
///
/// Fail-closed on database trouble: a failed lookup admits nobody.
async fn character_in_party(
    pool: &PgPool,
    party_id: i64,
    sub: &str,
) -> Result<Option<i64>, StatusCode> {
    sqlx::query_scalar(
        "SELECT id FROM characters WHERE party_id = $1 AND owner_sub = $2 ORDER BY id LIMIT 1",
    )
    .bind(party_id)
    .bind(sub)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, party_id, sub, "custom create membership lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Persist one `forbidden_custom_write` row, fail-closed: the contract's
/// "403 AND an audit row" makes the record part of the refusal — a denial
/// that cannot be persisted is a 500, never a silent 403 (SC-8's rule, the
/// same one `gm_read_only` runs on).
async fn audit_denied_edit(
    pool: &PgPool,
    actor_sub: &str,
    corpus_entry_id: i64,
    request_id: Option<String>,
) -> Result<(), StatusCode> {
    audit::try_record(
        pool,
        AuditEvent::ForbiddenCustomWrite,
        Some(actor_sub),
        &format!("custom/{corpus_entry_id}"),
        AuditOutcome::Denied,
        request_id.as_deref(),
    )
    .await
    .map_err(|error| {
        tracing::error!(
            error = %error,
            sub = actor_sub,
            corpus_entry_id,
            "failed to persist the forbidden_custom_write audit row"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// The `x-request-id` the request-id layer generated or propagated.
fn request_id_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// `POST /api/parties/{party_id}/custom` — create one custom-lane row.
/// Validation runs before any write; the corpus row and (for items) the
/// inventory row commit in one transaction. No WS broadcast (design D3):
/// party-wide visibility is the picker/composer query truth.
pub async fn create(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
    body: Json<JsonValue>,
) -> Response {
    let fields = match extract_fields(&body) {
        Ok(fields) => fields,
        Err(rejection) => return rejection.respond(),
    };
    let Some(kind_raw) = fields.kind.as_deref() else {
        return Rejection::new("kind", "item, spell, or condition").respond();
    };
    let kind = match Kind::parse(kind_raw) {
        Ok(kind) => kind,
        Err(rejection) => return rejection.respond(),
    };
    let row_name = match trimmed_name(fields.name.as_deref()) {
        Ok(name) => name,
        Err(rejection) => return rejection.respond(),
    };
    let description = match trimmed_description(fields.description.as_deref()) {
        Ok(description) => description,
        Err(rejection) => return rejection.respond(),
    };
    let value_or_rank = match validate_value(kind, fields.value_or_rank) {
        Ok(value) => value,
        Err(rejection) => return rejection.respond(),
    };

    let character_id = match character_in_party(&auth.pool, party_id, &account.sub).await {
        Ok(Some(character_id)) => character_id,
        // Outsiders, and the GM who owns no character, write nothing here.
        Ok(None) => return Forbidden.into_response(),
        Err(status) => return status.into_response(),
    };

    let mut tx = match auth.pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(error = %error, party_id, "custom create transaction failed to open");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let corpus_entry_id: i64 = match sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub) \
         VALUES ($1, $2, 'custom', $3, $4) RETURNING id",
    )
    .bind(kind.as_str())
    .bind(&row_name)
    .bind(custom_block(&description, value_or_rank))
    .bind(&account.sub)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(error = %error, party_id, sub = %account.sub, "custom row insert failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if kind == Kind::Item {
        // US-3 AC-1: quantity renders as base_qty + delta; a custom item has
        // no anchor base, so the first create's delta is the explicit 1 —
        // the column default 0 would render qty 0 (contract §1). The name
        // is the inventory anchor (PRIMARY KEY (character_id, item_name)),
        // so a second create of the same item accumulates: the character
        // holds another one. Corpus rows stay distinct either way (§3).
        let inserted = sqlx::query(
            "INSERT INTO character_inventory_live (character_id, item_name, qty_delta) \
             VALUES ($1, $2, 1) \
             ON CONFLICT (character_id, item_name) DO UPDATE \
             SET qty_delta = character_inventory_live.qty_delta + 1, updated_at = now()",
        )
        .bind(character_id)
        .bind(&row_name)
        .execute(&mut *tx)
        .await;
        if let Err(error) = inserted {
            tracing::error!(error = %error, character_id, "custom item inventory row insert failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    if let Err(error) = tx.commit().await {
        tracing::error!(error = %error, party_id, "custom create transaction failed to commit");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    tracing::info!(
        party_id,
        corpus_entry_id,
        kind = kind.as_str(),
        sub = %account.sub,
        "custom row created"
    );
    row_response(
        StatusCode::CREATED,
        corpus_entry_id,
        kind.as_str(),
        &row_name,
        &description,
        value_or_rank,
        &account.sub,
    )
}

/// The list query: `?kind=spell|item|condition` — the picker read is the
/// separate, already-shipped surface.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    kind: Option<String>,
}

/// One stored custom row as the list reads it: id, name, the custom block's
/// description and optional value, and the creator.
type StoredCustom = (
    i64,
    String,
    Option<String>,
    Option<JsonValue>,
    Option<String>,
);

/// `GET /api/parties/{party_id}/custom?kind=spell|item|condition` — the
/// client-side list shape for the composer, the inventory merge, and the
/// custom-condition chips (US-1 AC-5: the chip popup joins the creator's
/// description client-side from this read; chips stay name-only on the
/// wire). Party-readable (member or GM) — reads gate nothing (ownership
/// gates writes).
pub async fn list(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> Response {
    if !crate::engine_host::rest::party_readable(&auth.pool, &account, party_id).await {
        return Forbidden.into_response();
    }
    let Some(kind_raw) = query.kind.as_deref() else {
        return Rejection::new("kind", "spell, item, or condition").respond();
    };
    let Ok(kind @ (Kind::Spell | Kind::Item | Kind::Condition)) = Kind::parse(kind_raw) else {
        return Rejection::new("kind", "spell, item, or condition").respond();
    };
    let rows: Vec<StoredCustom> = match sqlx::query_as(
        "SELECT id, name, \
                data->'custom'->>'description', \
                data->'custom'->'value_or_rank', \
                created_by_sub \
         FROM corpus_entries WHERE kind = $1 AND lane = 'custom' ORDER BY name, id",
    )
    .bind(kind.as_str())
    .fetch_all(&auth.pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(error = %error, party_id, "custom list query failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let listed: Vec<JsonValue> = rows
        .into_iter()
        .map(
            |(corpus_entry_id, name, description, value_or_rank, created_by_sub)| {
                json!({
                    "corpus_entry_id": corpus_entry_id,
                    "name": name,
                    "description": description,
                    "value_or_rank": value_or_rank,
                    "created_by_sub": created_by_sub,
                })
            },
        )
        .collect();
    Json(listed).into_response()
}

/// The stored fields an edit touches, loaded fresh per request.
#[derive(sqlx::FromRow)]
struct StoredRow {
    kind_str: String,
    name: String,
    data: JsonValue,
    creator_sub: Option<String>,
}

/// The patched triple an edit writes.
struct Patched {
    name: String,
    description: String,
    value_or_rank: Option<i64>,
}

/// Apply the provided subset onto the stored row; absent fields keep their
/// stored values. Pure — no I/O — so the caps tests can drive it directly
/// through the handlers while this stays one readable law.
fn applied_edits(kind: Kind, fields: &Fields, stored: &StoredRow) -> Result<Patched, Rejection> {
    let name = if fields.name.is_some() {
        trimmed_name(fields.name.as_deref())?
    } else {
        stored.name.clone()
    };
    let stored_description = stored
        .data
        .get("custom")
        .and_then(|custom| custom.get("description"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let description = if fields.description.is_some() {
        trimmed_description(fields.description.as_deref())?
    } else {
        stored_description.to_owned()
    };
    let stored_value = stored
        .data
        .get("custom")
        .and_then(|custom| custom.get("value_or_rank"))
        .and_then(JsonValue::as_i64);
    let value_or_rank = if fields.value_provided {
        validate_value(kind, fields.value_or_rank)?
    } else {
        stored_value
    };
    Ok(Patched {
        name,
        description,
        value_or_rank,
    })
}

/// `PATCH /api/parties/{party_id}/custom/{corpus_entry_id}` — the creator's
/// edit. The lane gate runs first (an imported row is nobody's custom row to
/// edit — curation is E15), then E3's matrix (`authorize`) decides; both
/// refusals persist their audit row before the 403 leaves.
pub async fn edit(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
    Path((party_id, corpus_entry_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    body: Json<JsonValue>,
) -> Response {
    // The path's party segment must name a real party the actor belongs
    // to — the edit gate below is creator-only, but the route itself is
    // party-scoped. A foreign or nonexistent party is a resource path that
    // does not exist: 404, decided before any body is read.
    match character_in_party(&auth.pool, party_id, &account.sub).await {
        Ok(Some(_)) => {}
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(status) => return status.into_response(),
    }
    let fields = match extract_fields(&body) {
        Ok(fields) => fields,
        Err(rejection) => return rejection.respond(),
    };
    if fields.kind.is_some() {
        // The edit vocabulary is a subset of { name, description,
        // value_or_rank }; the kind a row lives in never changes.
        return Rejection::new("kind", "unknown field").respond();
    }
    let request_id = request_id_from(&headers);

    let loaded: Option<StoredRow> = match sqlx::query_as(
        "SELECT kind AS kind_str, name, data, created_by_sub AS creator_sub \
         FROM corpus_entries WHERE id = $1 AND lane = 'custom'",
    )
    .bind(corpus_entry_id)
    .fetch_optional(&auth.pool)
    .await
    {
        Ok(loaded) => loaded,
        Err(error) => {
            // Fail-closed: a database fault must not wear the 404 that a
            // genuinely absent row wears — the row's absence is unproven.
            tracing::error!(error = %error, corpus_entry_id, "custom edit load failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // The refusal law, one decision: a row outside the custom lane (or
    // absent entirely from that lane) is refused against the audit trail
    // unless it exists as a non-custom row — then the route never owned it
    // and the answer is the plain 404.
    let Some(stored) = loaded else {
        return missing_or_foreign_row(&auth.pool, corpus_entry_id, &account.sub, request_id).await;
    };
    let Ok(kind) = Kind::parse(&stored.kind_str) else {
        // A custom row whose kind is outside the vocabulary cannot have
        // been created by this module and cannot be re-validated here.
        tracing::error!(corpus_entry_id, kind = %stored.kind_str, "custom row carries an unknown kind");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let actor = Actor {
        sub: account.sub.clone(),
        role: account.role,
    };
    let creator_sub = stored.creator_sub.clone().unwrap_or_default();
    let verdict = authz::authorize(
        &actor,
        Action::Write,
        &Resource::CustomRow {
            creator_sub: creator_sub.clone(),
        },
    );
    if verdict == authz::Verdict::Deny {
        // The creator is the sole writer (FR-5); every other hand — member
        // or GM — is refused indistinguishably, per the E3 payload rule,
        // and the refusal persists its audit row before the 403 leaves.
        if let Err(status) =
            audit_denied_edit(&auth.pool, &account.sub, corpus_entry_id, request_id).await
        {
            return status.into_response();
        }
        return Forbidden.into_response();
    }

    let patched = match applied_edits(kind, &fields, &stored) {
        Ok(patched) => patched,
        Err(rejection) => return rejection.respond(),
    };

    // The custom block is this module's writer contract — rebuilt whole, so
    // a cleared value is gone rather than stale. The lane guard rides in the
    // WHERE clause: the row cannot change lanes under us mid-request.
    let updated = sqlx::query(
        "UPDATE corpus_entries SET name = $1, data = data || $2, updated_at = now() \
         WHERE id = $3 AND lane = 'custom'",
    )
    .bind(&patched.name)
    .bind(custom_block(&patched.description, patched.value_or_rank))
    .bind(corpus_entry_id)
    .execute(&auth.pool)
    .await;
    match updated {
        Ok(result) if result.rows_affected() == 1 => {}
        Ok(_) => {
            tracing::error!(corpus_entry_id, "custom edit updated no rows");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(error) => {
            tracing::error!(error = %error, corpus_entry_id, "custom edit update failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    tracing::info!(corpus_entry_id, sub = %account.sub, "custom row edited");
    row_response(
        StatusCode::OK,
        corpus_entry_id,
        kind.as_str(),
        &patched.name,
        &patched.description,
        patched.value_or_rank,
        // The creator, not the actor: only the creator passes the gate
        // today, but the field's name promises the row's creator.
        &creator_sub,
    )
}

/// The refusal path for an id this route does not own: a `custom` row that
/// vanished between load and now is a 404; a row on another lane — or a
/// missing id — is a lane-gated refusal, audited like any other (curation
/// editing is E15; an unknown id learns nothing either way).
///
/// Fail-closed on database trouble: a failed existence check refuses with
/// 500 — the row's absence is then unproven, and 404 would leak less but
/// audit nothing.
async fn missing_or_foreign_row(
    pool: &PgPool,
    corpus_entry_id: i64,
    actor_sub: &str,
    request_id: Option<String>,
) -> Response {
    // Fail-closed: a failed existence check refuses with 500 — the row's
    // absence is then unproven, and 404 would leak less but audit nothing.
    let exists: Option<bool> = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM corpus_entries WHERE id = $1 AND lane <> 'custom')",
    )
    .bind(corpus_entry_id)
    .fetch_one(pool)
    .await
    {
        Ok(exists) => exists,
        Err(error) => {
            tracing::error!(error = %error, corpus_entry_id, "custom lane existence check failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match exists {
        Some(true) => {
            if let Err(status) =
                audit_denied_edit(pool, actor_sub, corpus_entry_id, request_id).await
            {
                return status.into_response();
            }
            Forbidden.into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
