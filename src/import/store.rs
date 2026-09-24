//! The load layer: corpus row reads and writes against E2's
//! `corpus_entries` table.
//!
//! Lane discipline is structural: every statement in this module sets or
//! filters `lane = 'imported'` literally, and custom rows carry no
//! `source_id`, so the partial unique index on `(kind, source_id)` never
//! sees them — the importer structurally cannot touch them (FR-4). Each
//! category commits in its own transaction, opened only after fetch,
//! validation, and planning all succeeded (FR-5).

use anyhow::{Context as _, anyhow};
use serde_json::Value;
use sqlx::{PgPool, Row as _};

use crate::import::args::release_triple;
use crate::import::model::Kind;
use crate::import::transform::{CategoryPlan, ExistingRows, RowWrite};

/// Load existing imported rows for one kind, keyed by upstream id.
///
/// Rows whose `data.import` metadata is missing or malformed map to an
/// empty hash — they never skip, so the run refreshes them honestly.
///
/// # Errors
///
/// Returns an error when the query fails or a row's `source_id` is NULL
/// (which the lane contract forbids for imported rows).
pub async fn existing_rows(pool: &PgPool, kind: Kind) -> anyhow::Result<ExistingRows> {
    let rows = sqlx::query(
        "SELECT source_id, name, data->'import' AS import, \
                data->'import'->>'tier' AS tier \
         FROM corpus_entries WHERE kind = $1 AND lane = 'imported' AND source_id IS NOT NULL",
    )
    .bind(kind.as_str())
    .fetch_all(pool)
    .await
    .with_context(|| format!("failed to load existing {} rows", kind.plural()))?;
    let mut map = ExistingRows::new();
    for row in rows {
        let source_id: String = row
            .try_get("source_id")
            .context("existing imported row with NULL source_id")?;
        let name: String = row.try_get("name").context("existing row missing name")?;
        let import: Option<Value> = row.try_get("import").ok().flatten();
        let content_hash = import
            .as_ref()
            .and_then(|meta| meta.get("content_hash"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let importer_version = import
            .as_ref()
            .and_then(|meta| meta.get("importer_version"))
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let tier: Option<String> = row.try_get("tier").ok().flatten();
        map.insert(
            source_id,
            crate::import::transform::ExistingRow {
                name,
                content_hash,
                importer_version,
                tier,
            },
        );
    }
    Ok(map)
}

/// Apply one category's plan inside a single transaction.
///
/// # Errors
///
/// Returns an error when any statement fails or an UPDATE matches no row
/// (the corpus changed under us — failing loudly beats a silent fork).
pub async fn apply_category(
    pool: &PgPool,
    kind: Kind,
    release: &str,
    plan: &CategoryPlan,
) -> anyhow::Result<(u64, u64)> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open the category transaction")?;
    for write in &plan.inserts {
        insert_row(&mut tx, kind, write, release).await?;
    }
    for write in &plan.updates {
        let changed = update_row(&mut tx, kind, write, release).await?;
        if changed != 1 {
            return Err(anyhow!(
                "expected to update exactly one {} `{}` but the statement matched {changed} rows — \
                 the corpus changed during the run; refusing",
                write.source_id,
                kind.as_str()
            ));
        }
    }
    tx.commit()
        .await
        .context("failed to commit the category transaction")?;
    Ok((
        u64::try_from(plan.inserts.len()).unwrap_or(u64::MAX),
        u64::try_from(plan.updates.len()).unwrap_or(u64::MAX),
    ))
}

async fn insert_row(
    tx: &mut sqlx::PgConnection,
    kind: Kind,
    write: &RowWrite,
    release: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO corpus_entries \
             (kind, name, lane, data, modifiers, source_id, pack_version, imported_at) \
         VALUES ($1, $2, 'imported', $3, $4, $5, $6, now())",
    )
    .bind(kind.as_str())
    .bind(&write.name)
    .bind(&write.data)
    .bind(&write.modifiers)
    .bind(&write.source_id)
    .bind(release)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("failed to insert {} `{}`", kind.as_str(), write.source_id))?;
    Ok(())
}

async fn update_row(
    tx: &mut sqlx::PgConnection,
    kind: Kind,
    write: &RowWrite,
    release: &str,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        "UPDATE corpus_entries \
         SET name = $3, data = $4, modifiers = $5, pack_version = $6, \
             imported_at = now(), updated_at = now() \
         WHERE kind = $1 AND source_id = $2 AND lane = 'imported'",
    )
    .bind(kind.as_str())
    .bind(&write.source_id)
    .bind(&write.name)
    .bind(&write.data)
    .bind(&write.modifiers)
    .bind(release)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("failed to update {} `{}`", kind.as_str(), write.source_id))?;
    Ok(result.rows_affected())
}

/// Distinct publication license values across all imported rows, with row
/// counts. Rows without a stored license surface under the empty string.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn imported_license_counts(pool: &PgPool) -> anyhow::Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT COALESCE(data->'import'->'publication'->>'license', '') AS license, \
                count(*) AS n \
         FROM corpus_entries WHERE lane = 'imported' GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .context("failed to read distinct publication licenses")?;
    let mut counts = Vec::with_capacity(rows.len());
    for row in rows {
        counts.push((row.try_get::<String, _>(0)?, row.try_get::<i64, _>(1)?));
    }
    Ok(counts)
}

/// The newest pack version stamped on any imported row (None when empty).
///
/// Release order is numeric (major, minor, patch), not lexicographic —
/// text `max()` would rank `pf2e-9.9.0` above `pf2e-9.10.0` — so the
/// comparison happens in Rust over the distinct stamped versions.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn max_imported_release(pool: &PgPool) -> anyhow::Result<Option<String>> {
    let rows = sqlx::query(
        "SELECT DISTINCT pack_version FROM corpus_entries \
         WHERE lane = 'imported' AND pack_version IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the imported pack versions")?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<Option<String>, _>(0).ok().flatten())
        .max_by(|a, b| release_triple(a).cmp(&release_triple(b))))
}
