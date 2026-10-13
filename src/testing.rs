//! Test-only support: settings fixtures, isolated per-test databases, and
//! seeded sessions. Compiled only under `cfg(test)`; declared in `lib.rs`.
//!
//! Database-backed tests need a Postgres reachable at
//! `HIRELING_TEST_DATABASE_URL` (default: the local dev instance). When it is
//! unreachable AND the variable was not set, `test_pool` returns `None` and
//! the calling test prints a skip notice and passes — the pure-matrix suite
//! still runs everywhere, and the database suite runs in any environment
//! with a Postgres (locally: `just db`, or any Postgres 16+). When the
//! variable WAS set, an unreachable Postgres is a hard error: an operator
//! who names a database wants a database, and a skip would silently drop
//! every database-backed test out of the run — exactly what ate 21 of the
//! gate's 25 minutes before the gate grew a Postgres (gh#125). The probe
//! pool's `acquire_timeout` is capped so that error lands in seconds, not
//! sqlx's 30s retry loop per pool.
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

/// Was the test database URL set explicitly (→ unreachable is a hard error,
/// never a skip)? Same rule as `tests/import_db.rs`: a requested database
/// must not quietly become an absent one.
fn explicitly_requested() -> bool {
    std::env::var_os("HIRELING_TEST_DATABASE_URL").is_some()
}

/// Cap on the first probe pool's connection wait. sqlx's default is 30s of
/// connect-retry loop per pool; 152 probes against the gate's dead port
/// were ~1260s of sleep wearing a green badge (gh#125). Five seconds is a
/// red — or a loud skip — either way.
const PROBE_ACQUIRE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, migrated database for one test. Returns `None` (after a loud
/// notice) only when no Postgres is reachable at the default URL and none
/// was explicitly requested — see the module docs.
///
/// # Panics
///
/// Panics when `HIRELING_TEST_DATABASE_URL` is set but no Postgres answers
/// it (fail closed — a requested database must never become a skip), and
/// when the test Postgres **is** reachable but the fresh test database
/// cannot be created, connected to, or migrated — a reachable-but-broken
/// schema is a defect to fail on, never to skip.
pub async fn test_pool() -> Option<PgPool> {
    test_pool_from(maintenance_url(), explicitly_requested()).await
}

/// The probe + provision with the URL and the "explicitly requested" flag
/// injected, so the fail-closed contract has in-repo tests without mutating
/// the process environment (it is global; the suite's DB-backed tests read
/// it concurrently). `test_pool` is a thin env-read over this.
pub(crate) async fn test_pool_from(url: String, requested: bool) -> Option<PgPool> {
    let options: PgConnectOptions = match url.parse() {
        Ok(options) => options,
        Err(err) if requested => {
            panic!(
                "HIRELING_TEST_DATABASE_URL is set but {url} does not parse: {err} — a \
                    misconfigured test database fails red, never skips"
            )
        }
        Err(err) => {
            eprintln!(
                "skipping database-backed test: test database URL {url} does not parse ({err})"
            );
            return None;
        }
    };
    let maintenance = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(PROBE_ACQUIRE_TIMEOUT)
        .connect_with(options.clone())
        .await
    {
        Ok(pool) => pool,
        Err(err) if !requested => {
            eprintln!(
                "skipping database-backed test: no test Postgres at {url} ({err}); \
                 run `just db` or set HIRELING_TEST_DATABASE_URL"
            );
            return None;
        }
        Err(err) => panic!(
            "HIRELING_TEST_DATABASE_URL is set but no test Postgres answered at {url} within \
             {}s: {err} — the run asked for a database; fix the environment, never skip \
             (gh#125)",
            PROBE_ACQUIRE_TIMEOUT.as_secs(),
        ),
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
