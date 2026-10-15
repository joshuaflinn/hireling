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
//! The migrator runs **once per test process**, into a template database;
//! every test then clones the template (`CREATE DATABASE … TEMPLATE`). Each
//! test still owns a private, fully migrated database that no other test can
//! see — only the per-test migration run is gone: pure setup cost, paid once
//! per process instead of once per test (MOR-213). A migration
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
/// policy (gate-measured delta: none; kept as correct posture for a
/// throwaway, not as a speed lever).
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

/// Build an in-memory release archive from named JSON packs — shared test
/// support for running the REAL import pipeline (`import_from_bytes`)
/// without the network. Each entry is `(archive path, documents)`; the
/// importer v2 reads every `Kind::pack_files` entry; missing ones are filled.
///
/// # Panics
///
/// Panics if a zip entry fails to write — a broken test fixture, not a
/// condition to recover from.
pub(crate) fn pack_zip(packs: &[(&str, serde_json::Value)]) -> Vec<u8> {
    use std::io::Write as _;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default();
        for (name, docs) in packs {
            zip.start_file(*name, options).expect("zip entry starts");
            zip.write_all(docs.to_string().as_bytes())
                .expect("zip entry writes");
        }
        // Importer v2 requires every declared pack file; suites written for
        // v1 name only conditions + equipment. Fill the rest with one
        // minimal valid doc each (zero docs would trip FR-12), so those
        // suites keep exercising exactly what they always did.
        for kind in crate::import::model::ALL_KINDS {
            for file in kind.pack_files() {
                if packs.iter().any(|(name, _)| name == file) {
                    continue;
                }
                let filler = filler_doc(kind, file);
                zip.start_file(*file, options).expect("zip entry starts");
                zip.write_all(filler.to_string().as_bytes())
                    .expect("zip entry writes");
            }
        }
        zip.finish().expect("zip finishes");
    }
    buffer.into_inner()
}

/// One minimal valid document for a v2 pack file a test did not supply.
/// Creature packs beyond the first get an empty array (a kind needs one doc
/// in total, not one per file).
fn filler_doc(kind: crate::import::model::Kind, file: &str) -> serde_json::Value {
    use crate::import::model::Kind;
    let publication =
        serde_json::json!({"license": "ORC", "title": "Pathfinder Player Core", "remaster": true});
    let (doc_type, system) = match kind {
        Kind::Spell => (
            "spell",
            serde_json::json!({"description": {"value": ""}, "publication": publication}),
        ),
        Kind::Feat | Kind::ClassFeature => (
            "feat",
            serde_json::json!({"description": {"value": ""}, "publication": publication}),
        ),
        Kind::Action => (
            "action",
            serde_json::json!({"description": {"value": ""}, "publication": publication}),
        ),
        Kind::Creature => {
            if file != "packs/pathfinder-monster-core.json" {
                return serde_json::json!([]);
            }
            (
                "npc",
                serde_json::json!({"details": {"publicNotes": "", "publication": publication}}),
            )
        }
        Kind::Condition | Kind::Item => return serde_json::json!([]),
    };
    serde_json::json!([{
        "_id": format!("filler-{}", kind.as_str()),
        "name": format!("Filler {}", kind.as_str()),
        "type": doc_type,
        "img": "systems/pf2e/icons/default.webp",
        "system": system
    }])
}

/// The conditions pack every corpus-seeding archive must carry: the
/// importer refuses a zero-document pack of either kind (FR-12), and this
/// Frightened row (id verbatim from the seed JSON, `condition-tiers.json`)
/// is the mapping target that keeps the run clean. Body identical to the
/// doc the `custom_rows` suite has always shipped.
fn fright_condition_pack() -> serde_json::Value {
    serde_json::json!([{
        "_id": "TBSHQspnbcqxsmjL",
        "name": "Frightened",
        "type": "condition",
        "img": "systems/pf2e/icons/conditions/frightened.webp",
        "system": {
            "description": {"value": "Status penalty equal to the value."},
            "publication": {"license": "ORC", "title": "Pathfinder Player Core", "remaster": true},
            "value": {"isValued": true}
        }
    }])
}

/// An items release archive seeded through the importer's real pipeline
/// (gh#62 FR-2): one equipment document per item, carrying
/// `system.bulk.value` and `system.traits.value` in the pack's real key
/// form. Seeding through `import_from_bytes` stores the importer's actual
/// document shape — the old hand-written `{"system": …}` top-level shape
/// the importer never produced, which let the dead bootstrap read pass
/// green. The Frightened condition rides along because a zero-document
/// conditions pack fails the run (FR-12).
///
/// # Panics
///
/// Panics if a zip entry fails to write — a broken test fixture.
pub(crate) fn corpus_seed_zip(items: &[(&str, f64, &[&str])]) -> Vec<u8> {
    let equipment: Vec<serde_json::Value> = items
        .iter()
        .map(|(name, bulk, traits)| {
            serde_json::json!({
                "_id": format!("up-item-{}", name.to_lowercase()),
                "name": name,
                "type": "equipment",
                "img": "systems/pf2e/icons/equipment/test.webp",
                "system": {
                    "description": {"value": "Corpus seed item."},
                    "publication": {
                        "license": "ORC",
                        "title": "Pathfinder Player Core",
                        "remaster": true
                    },
                    "bulk": {"value": bulk},
                    "traits": {"value": traits}
                }
            })
        })
        .collect();
    pack_zip(&[
        ("packs/conditions.json", fright_condition_pack()),
        ("packs/equipment.json", serde_json::Value::Array(equipment)),
    ])
}
