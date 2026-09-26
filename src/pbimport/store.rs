//! The sqlx store and the orchestration transaction (FR-7/8/9/12, design §4).
//!
//! One import attempt is one transaction: caps → parse → validate →
//! transform run pure before any I/O; then a single `BEGIN … SELECT … FOR
//! UPDATE on characters … COMMIT` loads-or-creates the account's character,
//! seeds (first import) or anchors (re-import) live state, replaces
//! `payload_raw` and `base_sheet` wholesale, and appends the audit row. A
//! crash mid-import leaves the previous state fully intact.
//!
//! Active effects are never queried and never written (FR-14). Vitals are
//! written once, at first import, and never touched again (FR-10).

use sqlx::PgPool;
use sqlx::Row as _;

use crate::auth::audit;
use crate::auth::audit::{AuditEvent, AuditOutcome};
use crate::auth::authz::Actor;
use crate::pbimport::anchor::{self, Diff, ItemDelta, PreparedMap, SlotRow};
use crate::pbimport::caps;
use crate::pbimport::error::ImportError;
use crate::pbimport::model::{parse_and_validate, unknown_fields};
use crate::pbimport::transform::transform;

/// How one import attempt failed: the body's fault (4xx class) or the
/// server's (500). A database outage must never surface as a body-fault
/// code, and vice versa.
#[derive(Debug)]
pub enum RunImportError {
    /// The body was rejected by a failure class (contract §4).
    Invalid(ImportError),
    /// Storage failed; the attempt is rolled back.
    Database(sqlx::Error),
    /// A database invariant the migrations guarantee was violated — the
    /// schema is broken; a 500, never a body-fault code.
    Invariant(String),
}

impl From<ImportError> for RunImportError {
    fn from(error: ImportError) -> Self {
        RunImportError::Invalid(error)
    }
}

impl From<sqlx::Error> for RunImportError {
    fn from(error: sqlx::Error) -> Self {
        RunImportError::Database(error)
    }
}

impl std::fmt::Display for RunImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunImportError::Invalid(error) => write!(f, "{error}"),
            RunImportError::Database(error) => write!(f, "database error: {error}"),
            RunImportError::Invariant(what) => write!(f, "broken invariant: {what}"),
        }
    }
}

impl std::error::Error for RunImportError {}

/// The character summary the response carries (data-model §5, design §3).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CharacterSummary {
    pub id: i64,
    pub name: String,
    pub level: i64,
    pub class: Option<String>,
    pub owner: String,
    pub first_import: bool,
}

/// The advisory riding a success response: how many unknown fields were
/// skipped (FR-6).
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Advisory {
    pub skipped_fields: usize,
}

/// Everything a successful import returns (design §3).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportOutcome {
    pub character: CharacterSummary,
    pub diff: Diff,
    pub advisory: Advisory,
}

/// Run one import attempt end to end (design §4).
///
/// # Errors
///
/// [`RunImportError::Invalid`] for any contract §4 failure class;
/// [`RunImportError::Database`] when storage fails (attempt rolled back).
pub async fn run_import(
    pool: &PgPool,
    actor: &Actor,
    request_id: Option<&str>,
    body: &str,
) -> Result<ImportOutcome, RunImportError> {
    // Pure gatekeeping first — classes size, depth, (a), (b), and the
    // class-(c) walk on whatever survives.
    caps::check_size(body.len())?;
    let export = parse_and_validate(body)?;
    caps::check_depth(&export.value)?;
    let unknown_count = unknown_fields(&export).len();
    let (sheet, skips) = transform(&export);

    let mut transaction = pool.begin().await.map_err(RunImportError::Database)?;

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM characters WHERE owner_sub = $1 FOR UPDATE")
            .bind(&actor.sub)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(RunImportError::Database)?;

    let (character_id, first_import) = if let Some(id) = existing {
        (id, false)
    } else {
        let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| {
                RunImportError::Invariant(String::from(
                    "no party exists — migration 20260924000008 (POC seed) did not run",
                ))
            })?;
        let inserted: Option<i64> = sqlx::query_scalar(
            "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (owner_sub) DO NOTHING \
                 RETURNING id",
        )
        .bind(party_id)
        .bind(&actor.sub)
        .bind(body)
        .bind(sqlx::types::Json(&sheet))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RunImportError::Database)?;
        if let Some(id) = inserted {
            seed_vitals(&mut transaction, id, &sheet).await?;
            seed_slots(&mut transaction, id, &sheet).await?;
            (id, true)
        } else {
            // Lost a concurrent first-import race for this account: the
            // winner's row now exists. Lock it and take the re-import
            // path — one character per account holds (FR-8, SC-5).
            let id: i64 =
                sqlx::query_scalar("SELECT id FROM characters WHERE owner_sub = $1 FOR UPDATE")
                    .bind(&actor.sub)
                    .fetch_optional(&mut *transaction)
                    .await?
                    .ok_or_else(|| {
                        RunImportError::Invariant(String::from(
                            "lost first-import race but the winner's row is gone",
                        ))
                    })?;
            (id, false)
        }
    };

    let diff = if first_import {
        Diff::empty(true)
    } else {
        re_import(&mut transaction, character_id, body, &sheet, &skips).await?
    };

    audit::try_record(
        &mut *transaction,
        AuditEvent::CharacterImport,
        Some(&actor.sub),
        &format!("character:{character_id}"),
        AuditOutcome::Allowed,
        request_id,
    )
    .await
    .map_err(RunImportError::Database)?;

    transaction
        .commit()
        .await
        .map_err(RunImportError::Database)?;

    Ok(ImportOutcome {
        character: CharacterSummary {
            id: character_id,
            name: sheet.identity.name.clone(),
            level: sheet.identity.level,
            class: sheet.identity.class.clone(),
            owner: actor.sub.clone(),
            first_import,
        },
        diff,
        advisory: Advisory {
            skipped_fields: unknown_count,
        },
    })
}

/// Record a rejected attempt: one audit row naming the failure class, no
/// character touched. Best-effort on the insert (the verdict is already
/// delivered — the response carries it); failures log loudly.
pub async fn record_rejection(
    pool: &PgPool,
    actor: &Actor,
    request_id: Option<&str>,
    failure: ImportError,
) {
    audit::record(
        pool,
        AuditEvent::CharacterImport,
        Some(&actor.sub),
        &format!("import:{}", failure.code()),
        AuditOutcome::Denied,
        request_id,
    )
    .await;
}

/// First import: vitals to a session-ready baseline (FR-9).
async fn seed_vitals(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    character_id: i64,
    sheet: &crate::pbimport::transform::BaseSheet,
) -> Result<(), sqlx::Error> {
    let money = sheet.money.as_ref();
    let coin = |key: &str| {
        money
            .and_then(|money| money.get(key))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    sqlx::query(
        "INSERT INTO character_vitals (character_id, hp, money_gp, money_sp, money_cp, money_pp) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(character_id)
    .bind(sheet.hp.max_hp)
    .bind(coin("gp"))
    .bind(coin("sp"))
    .bind(coin("cp"))
    .bind(coin("pp"))
    .execute(&mut **transaction)
    .await
    .map(drop)
}

/// First import: materialize every layout position with export prep (FR-12).
async fn seed_slots(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    character_id: i64,
    sheet: &crate::pbimport::transform::BaseSheet,
) -> Result<(), sqlx::Error> {
    let prepared = PreparedMap::build(sheet);
    for (caster_key, rank, slot_index) in sheet.slot_layout() {
        sqlx::query(
            "INSERT INTO character_spell_slots \
             (character_id, caster_key, rank, slot_index, used, prepared_spell) \
             VALUES ($1, $2, $3, $4, false, $5)",
        )
        .bind(character_id)
        .bind(&caster_key)
        .bind(rank)
        .bind(slot_index)
        .bind(prepared.at(&caster_key, rank, slot_index))
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

/// Re-import: anchor live state, apply the plan, replace the stored sheet
/// and raw payload wholesale. Vitals are never read, never written (FR-10).
async fn re_import(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    character_id: i64,
    body: &str,
    sheet: &crate::pbimport::transform::BaseSheet,
    skips: &crate::pbimport::transform::SectionSkips,
) -> Result<Diff, RunImportError> {
    let rows = sqlx::query(
        "SELECT caster_key, rank, slot_index, used, prepared_spell \
         FROM character_spell_slots WHERE character_id = $1",
    )
    .bind(character_id)
    .fetch_all(&mut **transaction)
    .await?;
    let live_slots: Vec<SlotRow> = rows
        .iter()
        .map(|row| SlotRow {
            caster_key: row.get("caster_key"),
            rank: i64::from(row.get::<i32, _>("rank")),
            slot_index: i64::from(row.get::<i32, _>("slot_index")),
            used: row.get("used"),
            prepared_spell: row.get("prepared_spell"),
        })
        .collect();

    let item_rows = sqlx::query(
        "SELECT item_name, qty_delta FROM character_inventory_live WHERE character_id = $1",
    )
    .bind(character_id)
    .fetch_all(&mut **transaction)
    .await?;
    let live_items: Vec<ItemDelta> = item_rows
        .iter()
        .map(|row| ItemDelta {
            name: row.get("item_name"),
            qty_delta: i64::from(row.get::<i32, _>("qty_delta")),
        })
        .collect();

    let old_max_hp: i64 = sqlx::query_scalar(
        "SELECT COALESCE((base_sheet -> 'hp' ->> 'max_hp')::bigint, 0) FROM characters WHERE id = $1",
    )
    .bind(character_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(RunImportError::Database)?;

    let base_items: Vec<(String, i64)> = sheet
        .equipment
        .iter()
        .map(|item| (item.name.clone(), item.qty))
        .collect();
    let prepared = PreparedMap::build(sheet);
    let plan = anchor::anchor(
        &sheet.slot_layout(),
        &prepared,
        &live_slots,
        &live_items,
        &base_items,
        old_max_hp,
        sheet.hp.max_hp,
    );

    for seed in &plan.seeds {
        sqlx::query(
            "INSERT INTO character_spell_slots \
             (character_id, caster_key, rank, slot_index, used, prepared_spell) \
             VALUES ($1, $2, $3, $4, false, $5)",
        )
        .bind(character_id)
        .bind(&seed.caster_key)
        .bind(seed.rank)
        .bind(seed.slot_index)
        .bind(&seed.prepared)
        .execute(&mut **transaction)
        .await
        .map_err(RunImportError::Database)?;
    }

    // The base sheet is replaced wholesale; live rows above were the only
    // reads — nothing in vitals, slots, or inventory is updated by this.
    sqlx::query(
        "UPDATE characters SET payload_raw = $2, base_sheet = $3, updated_at = now() WHERE id = $1",
    )
    .bind(character_id)
    .bind(body)
    .bind(sqlx::types::Json(sheet))
    .execute(&mut **transaction)
    .await
    .map_err(RunImportError::Database)?;

    // The diff plus skip notices: section skips surface here (data-model §5).
    let mut diff = plan.diff;
    for section in &skips.sections {
        diff.notices
            .push(crate::pbimport::anchor::Notice::SectionSkipped {
                section: section.clone(),
            });
    }
    Ok(diff)
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;
