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
//! The migrator runs **once per test process**, into a template database;
//! every test then clones the template (`CREATE DATABASE … TEMPLATE`). Each
//! test still owns a private, fully migrated database that no other test can
//! see — only the per-test migration run is gone, which is pure setup cost
//! paid 137 times per process on the gate's clock (MOR-213). A migration
//! failure still panics red, at template build, under the same fresh-boot
//! contract as before.
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
use tokio::sync::OnceCell;

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

async fn build_template() -> String {
    let url = maintenance_url();
    let options: PgConnectOptions = url.parse().unwrap_or_else(|err| {
        panic!("template build: test database URL {url} does not parse: {err}")
    });
    let maintenance = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .unwrap_or_else(|err| {
            panic!("template build lost the maintenance connection to {url}: {err}")
        });

    let name = format!("hireling_tpl_{}", std::process::id());
    // Defensive only: the name is unique to this live process, but a
    // long-lived Postgres can still hold one from a dead pid.
    if let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
    )))
    .execute(&maintenance)
    .await
    {
        eprintln!("warning: could not drop stale template {name}: {err}");
    }

    if let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&maintenance)
        .await
    {
        panic!("template database {name} could not be created: {err}");
    }

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options.database(&name))
        .await
        .unwrap_or_else(|err| {
            panic!("template database {name} was created but could not be connected to: {err}")
        });
    if let Err(err) = db::migrator().run(&pool).await {
        pool.close().await;
        maintenance.close().await;
        panic!(
            "migrations failed on a FRESH template database ({name}): {err:?} — the \
             migrator holds {} migrations; a fresh boot is broken, fix the \
             migrations instead of skipping the test",
            db::migrator().iter().count()
        );
    }
    pool.close().await;

    // pool.close() returns before the server-side backends are all gone, and
    // a template with live connections cannot be cloned (55006). Terminate
    // whatever lingers so the first clone never has to wait out the drain.
    if let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{name}' AND pid <> pg_backend_pid()"
    )))
    .execute(&maintenance)
    .await
    {
        eprintln!("warning: could not drain template {name} backends: {err}");
    }
    maintenance.close().await;
    name
}

/// The template database's name, building it once per process on first use.
async fn template_name() -> String {
    TEMPLATE.get_or_init(build_template).await.clone()
}

/// Clone the template into a fresh test database owned by one test.
/// Concurrent clones serialize on the template inside Postgres; a departing
/// template backend (55006) retries briefly. Any other failure is a harness
/// defect. The clone runs with `synchronous_commit = off`: the throwaway
/// database is dropped at test end, so its crash durability is not under
/// test — the suite proves the app's write path, replay and
/// read-your-writes semantics, which do not depend on the WAL flush
/// policy (MOR-213: the gate's Postgres pays a per-commit flush cost that
/// dominated rust:test).
/// Panics when the clone itself fails — the server is reachable, so that
/// is a defect, never a skip. Panics too when the template cannot be
/// built: that is where the migrator runs, and a broken fresh boot must
/// fail red.
pub(crate) async fn create_from_template(maintenance: &PgPool, name: &str) {
    let template = template_name().await;
    for attempt in 0..40_u32 {
        match sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE {name} TEMPLATE {template}"
        )))
        .execute(maintenance)
        .await
        {
            Ok(_) => {
                if let Err(err) = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "ALTER DATABASE {name} SET synchronous_commit = off"
                )))
                .execute(maintenance)
                .await
                {
                    panic!(
                        "test database {name} could not be switched to \
                         synchronous_commit = off: {err}"
                    );
                }
                return;
            }
            Err(err)
                if err
                    .as_database_error()
                    .is_some_and(|db| db.code().is_some_and(|code| code == TEMPLATE_BUSY)) =>
            {
                assert!(
                    attempt != 39,
                    "template {template} still busy (55006) after 10s of retries: {err}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err(err) => panic!(
                "test Postgres is reachable but the test database {name} could not \
                 be cloned from template {template}: {err}"
            ),
        }
    }
}

static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// SQLSTATE 55006 — the template database still has a backend attached.
const TEMPLATE_BUSY: &str = "55006";

/// The per-process template database's name, built on first use.
static TEMPLATE: OnceCell<String> = OnceCell::const_new();

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
    // test, never skip it. The template carries the migrator run; the clone
    // is a private, fully migrated database.
    create_from_template(&maintenance, &name).await;

    let options = options.database(&name);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .unwrap_or_else(|err| {
            panic!("test database {name} was created but could not be connected to: {err}")
        });
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
        dev_sessions: false,
    }
}

/// Settings with OIDC pointed at the given stub endpoints.
#[must_use]
pub fn auth_settings_with_oidc(mut base: AuthSettings, oidc: OidcSettings) -> AuthSettings {
    base.oidc = Some(oidc);
    base
}

/// Settings with the dev-session legs explicitly opted in.
#[must_use]
pub fn auth_settings_with_dev_sessions(mut base: AuthSettings) -> AuthSettings {
    base.dev_sessions = true;
    base
}

/// Stamp the loopback peer the dev-session gate expects. Real servers get
/// this from the socket; `oneshot` requests carry no peer, so tests that
/// exercise the gate add it by hand. Pass `None` to leave the request
/// peerless (reads as a remote client).
pub fn with_peer(request: &mut axum::extract::Request, peer: Option<std::net::SocketAddr>) {
    if let Some(peer) = peer {
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(peer));
    }
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

/// Router wired to a pool and settings: default sync state on a drain
/// channel no test ever fires (the sender lives for the process).
///
/// # Panics
///
/// Panics if the auth state cannot be built — in tests, that is a bug, not
/// a condition to handle.
pub fn router_for(pool: PgPool, auth: &AuthSettings) -> Router {
    router_for_with_sync(pool, auth, crate::sync::SyncState::new(never_drain()))
}

/// Router wired to a caller-supplied sync state — the session tests' hook
/// for shortened liveness timers and a live drain channel. Production
/// wiring lives in `http::serve`.
///
/// # Panics
///
/// Panics if the auth state cannot be built — in tests, that is a bug, not
/// a condition to handle.
pub fn router_for_with_sync(
    pool: PgPool,
    auth: &AuthSettings,
    sync: crate::sync::SyncState,
) -> Router {
    let auth_state = AuthState::new(pool, auth).expect("test auth state builds");
    crate::http::router(auth_state, std::path::Path::new("web/dist"), sync)
}

/// A drain receiver whose sender lives for the whole process — "no signal",
/// never fired, no busy-loop risk from a dropped sender.
pub fn never_drain() -> tokio::sync::watch::Receiver<bool> {
    static NEVER: std::sync::OnceLock<tokio::sync::watch::Sender<bool>> =
        std::sync::OnceLock::new();
    let (tx, _rx) = tokio::sync::watch::channel(false);
    NEVER.get_or_init(|| tx).subscribe()
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
