//! Test-only support: settings fixtures, isolated per-test databases, and
//! seeded sessions. Compiled only under `cfg(test)`; declared in `lib.rs`.
//!
//! Database-backed tests need a Postgres reachable at
//! `HIRELING_TEST_DATABASE_URL` (default: the local dev instance). When it is
//! unreachable, `test_pool` returns `None` and the calling test prints a skip
//! notice and passes — the pure-matrix suite still runs everywhere, and the
//! database suite runs in any environment with a Postgres (locally: `just db`,
//! or any Postgres 16+).
//!
//! Reachable but broken is a different case: a fresh test database that
//! cannot be created, connected to, or migrated panics. A skipped test that
//! reports green proves nothing about the schema — that is how a
//! fresh-boot-breaking migration once slipped through as "1 passed".

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use axum::Router;
use sqlx::PgPool;
use sqlx::postgres::PgConnectOptions;

use crate::auth::AuthState;
use crate::config::AuthSettings;
use crate::config::OidcSettings;
use crate::config::Settings;
use crate::db;

/// The test seats, mirroring the dev-session seat names and their subs.
pub const PLAYER_SUB: &str = "dev-sub-josh";
pub const GM_SUB: &str = "dev-sub-gm";

/// Maintenance URL for creating isolated per-test databases.
fn maintenance_url() -> String {
    std::env::var("HIRELING_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hireling:hireling@127.0.0.1:5432/postgres".to_owned())
}

static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, migrated database for one test. Returns `None` (after a loud
/// notice) when no Postgres is reachable — see the module docs.
///
/// # Panics
///
/// Panics when the test Postgres **is** reachable but the fresh test
/// database cannot be created, connected to, or migrated — a reachable-but-
/// broken schema is a defect to fail on, never to skip.
pub async fn test_pool() -> Option<PgPool> {
    let url = maintenance_url();
    let options: PgConnectOptions = url.parse().ok()?;
    let maintenance = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
    {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!(
                "skipping database-backed test: no test Postgres at {url} ({err}); \
                 run `just db` or set HIRELING_TEST_DATABASE_URL"
            );
            return None;
        }
    };

    let name = format!(
        "hireling_test_{}_{}",
        std::process::id(),
        DB_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    // The database name comes from our own generator, not user input — the
    // SQL-safety assertion is honest here. From here on the Postgres is
    // reachable, so every failure below is a real defect and must fail the
    // test, never skip it.
    if let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&maintenance)
        .await
    {
        panic!(
            "test Postgres is reachable but the test database {name} could not \
             be created: {err}"
        );
    }

    let options = options.database(&name);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .unwrap_or_else(|err| {
            panic!("test database {name} was created but could not be connected to: {err}")
        });
    if let Err(err) = db::migrator().run(&pool).await {
        if let Err(drop_err) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {name} WITH (FORCE)"
        )))
        .execute(&maintenance)
        .await
        {
            eprintln!("warning: could not drop {name} after a failed run: {drop_err}");
        }
        panic!(
            "migrations failed on a FRESH test database ({name}): {err:?} — the \
             migrator holds {} migrations; a fresh boot is broken, fix the \
             migrations instead of skipping the test",
            db::migrator().iter().count()
        );
    }
    Some(pool)
}

/// Drop the per-test database. Best effort — a leaked test database on a dev
/// machine is harmless, but the attempt is loud on failure.
pub async fn drop_test_db(pool: PgPool, test_name: &str) {
    let name = pool
        .connect_options()
        .get_database()
        .unwrap_or("unknown")
        .to_owned();
    let options = {
        let mut options = (*pool.connect_options()).clone();
        options = options.database("postgres");
        options
    };
    drop(pool);
    if let Ok(maintenance) = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        && let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {name} WITH (FORCE)"
        )))
        .execute(&maintenance)
        .await
    {
        eprintln!("warning: could not drop {name} after {test_name}: {err}");
    }
}

/// Base settings for tests: allowlist with the dev seats, GM seat configured,
/// 32-byte cookie key. Callers add OIDC settings for provider round trips.
#[must_use]
pub fn auth_settings() -> AuthSettings {
    let subs: &[&str] = &[
        "dev-sub-josh",
        "dev-sub-bear",
        "dev-sub-dave",
        "dev-sub-becky",
        "dev-sub-jake",
        "dev-sub-gm",
    ];
    AuthSettings {
        oidc: None,
        cookie_key: vec![0x2a; 32],
        allowlist: subs
            .iter()
            .map(|sub| (*sub).to_owned())
            .collect::<HashSet<_>>(),
        seat_names: HashMap::new(),
        gm_sub: "dev-sub-gm".to_owned(),
        session_idle_secs: 86_400,
        session_absolute_secs: 7 * 86_400,
    }
}

/// Settings with OIDC pointed at the given stub endpoints.
#[must_use]
pub fn auth_settings_with_oidc(mut base: AuthSettings, oidc: OidcSettings) -> AuthSettings {
    base.oidc = Some(oidc);
    base
}

#[must_use]
pub fn settings_from(auth: AuthSettings) -> Settings {
    Settings {
        log_level: "info".to_owned(),
        port: 3000,
        database_url: "unused-in-router-tests".to_owned(),
        static_dir: std::path::PathBuf::from("web/dist"),
        auth,
    }
}

/// Router wired to a pool and settings.
///
/// # Panics
///
/// Panics if the auth state cannot be built — in tests, that is a bug, not
/// a condition to handle.
pub fn router_for(pool: PgPool, auth: &AuthSettings) -> Router {
    let auth_state = AuthState::new(pool, auth).expect("test auth state builds");
    crate::http::router(auth_state, std::path::Path::new("web/dist"))
}

/// Auth state without a router, for store-level tests.
///
/// # Panics
///
/// Panics if the auth state cannot be built — in tests, that is a bug, not
/// a condition to handle.
#[must_use]
pub fn auth_state(pool: PgPool, auth: &AuthSettings) -> std::sync::Arc<AuthState> {
    AuthState::new(pool, auth).expect("test auth state builds")
}

/// Insert an account row directly.
///
/// # Panics
///
/// Panics if the insert fails — the seeding contract of every calling test.
pub async fn seed_account(pool: &PgPool, sub: &str, role: &str) {
    sqlx::query("INSERT INTO accounts (sub, username, display_name, role) VALUES ($1, $1, $1, $2)")
        .bind(sub)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed account");
}

/// Insert a live session row and return its id. Upserts the account row for
/// `sub` first (player role) — production sessions always hang off an
/// upserted login, and the sessions FK enforces the same invariant in
/// tests. For a GM-seat session, seed the account yourself first.
///
/// # Panics
///
/// Panics if token generation or the insert fails — the seeding contract of
/// every calling test.
pub async fn seed_session(pool: &PgPool, sub: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    sqlx::query(
        "INSERT INTO accounts (sub, username, display_name, role) VALUES ($1, $1, $1, 'player') \
         ON CONFLICT (sub) DO NOTHING",
    )
    .bind(sub)
    .execute(pool)
    .await
    .expect("seed session's account");
    let id = crate::auth::oidc::random_token().expect("random token");
    crate::auth::session::insert(pool, &id, sub, now, 86_400, 7 * 86_400)
        .await
        .expect("seed session");
    id
}

/// All audit rows as (event, actor, target, outcome), oldest first.
///
/// # Panics
///
/// Panics if the query fails — a broken schema is a failing test.
pub async fn audit_rows(pool: &PgPool) -> Vec<(String, Option<String>, String, String)> {
    sqlx::query_as::<_, (String, Option<String>, String, String)>(
        "SELECT event, actor_sub, target, outcome FROM audit_events ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("audit rows")
}
