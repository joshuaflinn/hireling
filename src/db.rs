//! Database pool construction and schema migrations.
//!
//! Thin shell: builds a pool from settings and applies the embedded
//! migrations at boot, so a schema the app cannot reach or cannot apply
//! fails the startup loudly — before traffic, never at first query.
//! (Human-settled default: app-boot migrate, single binary, single replica.)
//!
//! The business logic that will sit on top of this pool (import anchoring,
//! versioned writes) belongs in pure modules owned by later epics; this file
//! stays plumbing.

use anyhow::Context as _;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Pool ceiling — deliberate headroom under the production role's
/// `CONNECTION LIMIT 20` (db/provision.sql), shared across every consumer.
const MAX_CONNECTIONS: u32 = 10;

/// The embedded migrations, checked in under `migrations/`.
#[must_use]
pub fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!("./migrations")
}

#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

/// Connect to Postgres and bring the schema to the current migration.
///
/// # Errors
///
/// Returns an error if the database cannot be reached or any migration
/// fails to apply. A failed run resumes-or-fails-loud on retry (sqlx
/// bookkeeping) — it never silently half-applies.
pub async fn connect_and_migrate(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect(database_url)
        .await
        .context("failed to connect to Postgres")?;

    migrator()
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;

    tracing::info!(
        migrations = migrator().iter().count(),
        "database schema up to date"
    );

    Ok(pool)
}
