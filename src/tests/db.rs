//! Database schema tests — epic E2's success criteria, against a real
//! Postgres.
//!
//! Each test provisions its own throwaway database (created from the
//! maintenance connection, migrated up via the app's embedded migrator,
//! dropped at the end), so tests run in parallel and never fight over state.
//!
//! Gating: tests need a reachable Postgres at the documented local URL
//! (default `postgres://hireling:hireling@127.0.0.1:5432`, the compose
//! throwaway). The ONLY skip is a server unreachable at the transport
//! level — connection refused, no route, DNS, or no connection established
//! within the acquire budget: every test then SKIPS LOUDLY on stderr,
//! never a silent green. A server that ANSWERS never skips: a rejected
//! password, a permission failure, or a missing maintenance database is a
//! red test — and so are failed CREATE DATABASE, connect, or migration
//! runs. A misconfigured test database must never pass for a skip.
//! `just db` starts the throwaway; `just db-reset` wipes
//! anything this suite ever leaked.

use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row as _};

/// Execute a dynamically-built statement.
///
/// Every dynamic SQL string in this harness interpolates only ids and
/// literals generated inside the test itself — there is no user input — so
/// the [`AssertSqlSafe`] audit passes. Static SQL stays on plain `query`.
macro_rules! dyn_sql {
    ($sql:expr) => {
        sqlx::query(AssertSqlSafe($sql))
    };
}

/// Base URL of the local throwaway Postgres (no database component).
/// Overridable so the skip path can be exercised deliberately.
fn base_url() -> String {
    std::env::var("HIRELING_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hireling:hireling@127.0.0.1:5432".to_owned())
}

/// Outcome of the maintenance-pool connection attempt.
enum MaintenanceConnect {
    /// Connected: the server is up and answered.
    Up(PgPool),
    /// The server is confirmed unreachable at the transport level — the
    /// one legitimate loud-skip case ("no Postgres — run `just db`").
    Skip(String),
    /// The attempt was rejected by something that answered: authentication,
    /// permission, a missing maintenance database, or an unparseable URL.
    /// This is a broken environment or config and must fail RED.
    Fatal(String),
}

/// Is this connect failure proof that no server answered?
///
/// Only transport-class errors count: an OS-level I/O failure (connection
/// refused, network unreachable, DNS) or a pool that could not establish
/// any connection within the acquire budget (a firewalled, silently
/// dropping host surfaces as `PoolTimedOut`). Anything else means either
/// the server spoke (`Database` — auth/permission/missing-database) or our
/// own URL/config is broken (`Configuration`, TLS, protocol) — neither of
/// which may ride the skip path.
fn is_transport_unreachable(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut)
}

/// Connect the maintenance pool at `{base}/postgres`, classifying the
/// outcome so the suite can skip only genuine no-server environments.
async fn connect_maintenance(base: &str, test: &str) -> MaintenanceConnect {
    let url = format!("{base}/postgres");
    let attempt = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect(&url)
        .await;
    match attempt {
        Ok(pool) => MaintenanceConnect::Up(pool),
        Err(err) if is_transport_unreachable(&err) => MaintenanceConnect::Skip(format!(
            "SKIPPED (no Postgres at {base} — run `just db`): {test}: {err}"
        )),
        Err(err) => MaintenanceConnect::Fatal(format!(
            "maintenance connection to {url} failed but the server answered \
             (auth, permission, or missing database — server reachable): {err}"
        )),
    }
}

/// The same URL with a password no server should accept. Used by the
/// regression test that pins the reachable-server-rejection path.
fn corrupt_password(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("postgres", url));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let bad_authority = match authority.rsplit_once('@') {
        Some((userinfo, hostport)) => {
            let user = userinfo.split(':').next().unwrap_or("hireling");
            format!("{user}:definitely-wrong-password@{hostport}")
        }
        None => format!("hireling:definitely-wrong-password@{authority}"),
    };
    if path.is_empty() {
        format!("{scheme}://{bad_authority}")
    } else {
        format!("{scheme}://{bad_authority}/{path}")
    }
}

/// Every application table the migrations must create.
const ALL_TABLES: &[&str] = &[
    "accounts",
    "parties",
    "characters",
    "character_vitals",
    "character_spell_slots",
    "character_inventory_live",
    "effects",
    "effect_targets",
    "effect_modifiers",
    "corpus_entries",
    "party_stash",
    "claim_history",
    "party_bank",
];

/// SQLSTATE: foreign-key violation.
const FK_VIOLATION: &str = "23503";
/// SQLSTATE: CHECK / other constraint violation.
const CHECK_VIOLATION: &str = "23514";
/// SQLSTATE: unique-index violation.
const UNIQUE_VIOLATION: &str = "23505";
/// SQLSTATE: RAISE EXCEPTION — what the append-only trigger emits.
const TRIGGER_EXCEPTION: &str = "P0001";

/// A private database for one test: migrated schema, isolated from every
/// other test. Dropped best-effort at test end; a *failing* test may leak
/// its database into the throwaway instance (`just db-reset` clears those).
struct TestDb {
    name: String,
    pool: PgPool,
    admin: PgPool,
}

impl TestDb {
    /// Provision one migrated database.
    ///
    /// `Err` carries the loud skip reason when no Postgres is reachable at
    /// the transport level — each test prints it to stderr and returns,
    /// visibly, without passing any assertion. That is the only skip: on a
    /// *reachable* server, a rejected password, a failed CREATE DATABASE,
    /// connect, or migration run panics red.
    async fn provision(test: &str) -> Result<TestDb, String> {
        let base = base_url();
        let admin = match connect_maintenance(&base, test).await {
            MaintenanceConnect::Up(pool) => pool,
            MaintenanceConnect::Skip(reason) => {
                eprintln!("{reason}");
                return Err(reason);
            }
            MaintenanceConnect::Fatal(context) => panic!("{context}"),
        };

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let name = format!("hireling_e2_{test}_{}_{}", std::process::id(), nanos);

        // Past this line the server is reachable: every failure is a broken
        // environment or schema, and the test must go RED — never ride the
        // skip path (a green suite must not absorb a broken migration).
        if let Err(err) = dyn_sql!(format!("CREATE DATABASE {name}").as_str())
            .execute(&admin)
            .await
        {
            admin.close().await;
            panic!("failed to create test database {name} (server reachable): {err}");
        }

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!("{base}/{name}"))
            .await
            .unwrap_or_else(|err| {
                panic!("failed to connect to fresh database {name} (server reachable): {err}")
            });

        if let Err(err) = crate::db::migrator().run(&pool).await {
            pool.close().await;
            admin.close().await;
            panic!("migrations failed on {name} (server reachable) — schema is broken: {err}");
        }

        Ok(TestDb { name, pool, admin })
    }

    /// Drop the database: close pools first, then DROP ... FORCE.
    async fn drop_self(self) {
        self.pool.close().await;
        let dropped = dyn_sql!(format!("DROP DATABASE {} WITH (FORCE)", self.name).as_str())
            .execute(&self.admin)
            .await;
        if let Err(err) = dropped {
            eprintln!("warning: could not drop {}: {err}", self.name);
        }
        self.admin.close().await;
    }

    /// Assert one statement fails with the given SQLSTATE.
    async fn assert_fails_with(&self, sql: &str, state: &str, label: &str) {
        let result = dyn_sql!(sql).execute(&self.pool).await;
        assert!(
            result.is_err(),
            "{label}: statement unexpectedly succeeded (sql: {sql})"
        );
        let err = result.unwrap_err();
        let db_err = err
            .as_database_error()
            .unwrap_or_else(|| panic!("{label}: not a database error: {err}"));
        let code = db_err.code().map_or_else(
            || panic!("{label}: error carries no SQLSTATE: {err}"),
            |c| c.to_string(),
        );
        assert_eq!(code, state, "{label}: wrong SQLSTATE (sql: {sql})");
    }
}

/// Count rows of a single-count select.
async fn count(db: &TestDb, sql: &str) -> i64 {
    let row = dyn_sql!(sql)
        .fetch_one(&db.pool)
        .await
        .unwrap_or_else(|err| panic!("count query failed ({sql}): {err}"));
    row.get::<i64, _>(0)
}

/// Fetch the first column of the first row of a select as a `String`.
async fn one_string(db: &TestDb, sql: &str) -> String {
    let row = dyn_sql!(sql)
        .fetch_one(&db.pool)
        .await
        .unwrap_or_else(|err| panic!("query failed ({sql}): {err}"));
    row.get::<String, _>(0)
}

/// Execute one statement, panicking with context when it fails.
async fn exec(db: &TestDb, sql: &str, label: &str) {
    dyn_sql!(sql)
        .execute(&db.pool)
        .await
        .unwrap_or_else(|err| panic!("{label}: {err}"));
}

/// One account + party + character (imported, with vitals).
/// Returns `(party_id, character_id)`.
async fn seed_character(db: &TestDb, sub: &str, party_name: &str) -> (i64, i64) {
    exec(
        db,
        &format!(
            "INSERT INTO accounts (sub, username, display_name, role)
             VALUES ('{sub}', '{sub}', 'Display {sub}', 'player')"
        ),
        &format!("seed account {sub}"),
    )
    .await;

    let party = one_string(
        db,
        &format!("INSERT INTO parties (name) VALUES ('{party_name}') RETURNING id::text"),
    )
    .await
    .parse::<i64>()
    .expect("party id parses");

    let character = one_string(
        db,
        &format!(
            "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
             VALUES ({party}, '{sub}', '{{\"success\":true}}', '{{\"level\":5}}')
             RETURNING id::text"
        ),
    )
    .await
    .parse::<i64>()
    .expect("character id parses");

    exec(
        db,
        &format!(
            "INSERT INTO character_vitals (character_id, hp, temp_hp) VALUES ({character}, 25, 3)"
        ),
        &format!("seed vitals for {sub}"),
    )
    .await;

    (party, character)
}

/// Seed one effect owned by a fresh character; returns its id.
async fn seed_effect(db: &TestDb) -> i64 {
    let (party, character) = seed_character(db, "sub-fx", "Effect Anchors").await;
    one_string(
        db,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name)
             VALUES ({party}, {character}, 'Anchor') RETURNING id::text"
        ),
    )
    .await
    .parse::<i64>()
    .expect("effect id parses")
}

/// Standard skip-or-provision prologue for a test body.
macro_rules! provision_or_skip {
    ($test:literal) => {
        match TestDb::provision($test).await {
            Ok(db) => db,
            Err(reason) => {
                eprintln!("{reason}");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------
// SC-1 — migrate up → verify objects → migrate down → verify empty, 3 cycles.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc1_migrations_cycle_up_and_down_three_times() {
    let db = provision_or_skip!("sc1");

    for cycle in 1..=3 {
        // Provision already migrated up; bring it down first so every cycle
        // really exercises both directions from this code's hands.
        let down = crate::db::migrator().undo(&db.pool, 0).await;
        assert!(
            down.is_ok(),
            "cycle {cycle}: migrate down failed: {:?}",
            down.err()
        );
        assert_eq!(
            count(&db, &tables_remaining_sql()).await,
            0,
            "cycle {cycle}: application tables survived migrate-down"
        );

        let up = crate::db::migrator().run(&db.pool).await;
        assert!(
            up.is_ok(),
            "cycle {cycle}: migrate up failed: {:?}",
            up.err()
        );
        assert_all_objects_present(&db, cycle).await;
    }

    db.drop_self().await;
}

/// Assert every expected table, the version sequence, and the append-only
/// trigger exist after a migrate-up.
async fn assert_all_objects_present(db: &TestDb, cycle: u8) {
    for table in ALL_TABLES {
        assert_eq!(
            count(
                db,
                &format!(
                    "SELECT count(*) FROM pg_tables
                     WHERE schemaname = 'public' AND tablename = '{table}'"
                )
            )
            .await,
            1,
            "cycle {cycle}: table {table} missing after migrate-up"
        );
    }
    assert_eq!(
        count(
            db,
            "SELECT count(*) FROM pg_sequences WHERE schemaname = 'public'
             AND sequencename = 'field_version_seq'"
        )
        .await,
        1,
        "cycle {cycle}: field_version_seq missing after migrate-up"
    );
    assert_eq!(
        count(
            db,
            "SELECT count(*) FROM pg_trigger
             WHERE tgname = 'claim_history_append_only' AND NOT tgisinternal"
        )
        .await,
        1,
        "cycle {cycle}: append-only trigger missing after migrate-up"
    );
}

/// SQL counting surviving application tables (empty schema gives 0).
fn tables_remaining_sql() -> String {
    let wanted: Vec<String> = ALL_TABLES.iter().map(|t| format!("'{t}'")).collect();
    format!(
        "SELECT count(*) FROM pg_tables WHERE schemaname = 'public'
         AND tablename IN ({})",
        wanted.join(", ")
    )
}

// ---------------------------------------------------------------------------
// SC-2 — a second party is inserts only, and party scoping isolates.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc2_second_party_is_inserts_only_and_isolated() {
    let db = provision_or_skip!("sc2");
    let (party_a, char_a) = seed_character(&db, "sub-a", "First Party").await;

    // The second campaign: data only. No migration, no DDL.
    exec(
        &db,
        "INSERT INTO accounts (sub, username, display_name, role)
         VALUES ('sub-b', 'sub-b', 'Display sub-b', 'player')",
        "seed second account",
    )
    .await;
    let party_b = one_string(
        &db,
        "INSERT INTO parties (name) VALUES ('Second Campaign') RETURNING id::text",
    )
    .await
    .parse::<i64>()
    .expect("party id parses");
    let char_b = one_string(
        &db,
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
         VALUES ((SELECT id FROM parties WHERE name = 'Second Campaign'), 'sub-b', '{}', '{}')
         RETURNING id::text",
    )
    .await
    .parse::<i64>()
    .expect("character id parses");
    exec(
        &db,
        &format!("INSERT INTO character_vitals (character_id, hp) VALUES ({char_b}, 10)"),
        "seed second vitals",
    )
    .await;

    // Full state for both parties: live rows, an effect, P1 rows.
    seed_party_state(&db, party_a, char_a).await;
    seed_party_state(&db, party_b, char_b).await;

    // Every directly party-scoped table filters to exactly its party.
    for (table, party) in [
        ("characters", party_a),
        ("effects", party_a),
        ("party_stash", party_a),
        ("party_bank", party_a),
        ("characters", party_b),
        ("effects", party_b),
        ("party_stash", party_b),
        ("party_bank", party_b),
    ] {
        assert_eq!(
            count(
                &db,
                &format!("SELECT count(*) FROM {table} WHERE party_id = {party}")
            )
            .await,
            1,
            "{table}: party {party} should see exactly its own row"
        );
    }

    // Live tables scope through their character's party.
    for party in [party_a, party_b] {
        assert_eq!(
            count(
                &db,
                &format!(
                    "SELECT count(*) FROM character_vitals v
                     JOIN characters c ON c.id = v.character_id
                     WHERE c.party_id = {party}"
                )
            )
            .await,
            1,
            "character_vitals: party {party} should see exactly its own row"
        );
    }

    db.drop_self().await;
}

/// Live rows, one effect, and P1 rows for one party's character.
async fn seed_party_state(db: &TestDb, party: i64, character: i64) {
    for sql in [
        format!(
            "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index) VALUES ({character}, 'Wizard', 1, 0)"
        ),
        format!(
            "INSERT INTO character_inventory_live (character_id, item_name, qty_delta) VALUES ({character}, 'Chalk', -1)"
        ),
        format!(
            "INSERT INTO effects (party_id, source_character_id, name) VALUES ({party}, {character}, 'Bless')"
        ),
        format!(
            "INSERT INTO party_stash (party_id, item_name, quantity) VALUES ({party}, 'Rope', 1)"
        ),
        format!("INSERT INTO party_bank (party_id) VALUES ({party})"),
    ] {
        exec(db, &sql, &format!("seed party {party} state")).await;
    }
}

// ---------------------------------------------------------------------------
// SC-2b — cross-party links are unstorable: the schema pins every
// character-touching edge to its own party's roster (FR-4).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc2b_cross_party_links_are_rejected() {
    let db = provision_or_skip!("sc2b");
    let (party_a, char_a) = seed_character(&db, "sub-a", "Alpha").await;
    let (party_b, char_b) = seed_character(&db, "sub-b", "Beta").await;

    // Baseline: an in-party effect stores fine.
    let effect_a = one_string(
        &db,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name)\n             VALUES ({party_a}, {char_a}, 'Bless') RETURNING id::text"
        ),
    )
    .await
    .parse::<i64>()
    .expect("effect id parses");

    // An effect of party B created by party A's character: rejected.
    db.assert_fails_with(
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name)\n             VALUES ({party_b}, {char_a}, 'Cross-Created')"
        ),
        FK_VIOLATION,
        "an effect's creator must belong to the effect's party",
    )
    .await;

    // An effect of party A targeting party B's character: rejected.
    db.assert_fails_with(
        &format!(
            "INSERT INTO effect_targets (party_id, effect_id, character_id)\n             VALUES ({party_a}, {effect_a}, {char_b})"
        ),
        FK_VIOLATION,
        "an effect target must belong to the effect's party",
    )
    .await;

    // Control: the same-party target stores — the pins bind the edge, not
    // the table.
    exec(
        &db,
        &format!(
            "INSERT INTO effect_targets (party_id, effect_id, character_id)\n             VALUES ({party_a}, {effect_a}, {char_a})"
        ),
        "same-party target must store",
    )
    .await;

    // Party A naming party B's character quartermaster: rejected.
    db.assert_fails_with(
        &format!("UPDATE parties SET quartermaster_character_id = {char_b} WHERE id = {party_a}"),
        FK_VIOLATION,
        "the quartermaster must belong to the party",
    )
    .await;

    // Control: the same-party designation stores, and hard-deleting that
    // character clears the designation (SET NULL survives the composite pin).
    exec(
        &db,
        "INSERT INTO accounts (sub, username, display_name, role)\n         VALUES ('sub-c', 'sub-c', 'Display sub-c', 'player')",
        "seed third account",
    )
    .await;
    let char_c = one_string(
        &db,
        &format!(
            "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)\n             VALUES ({party_a}, 'sub-c', '{{}}', '{{}}') RETURNING id::text"
        ),
    )
    .await
    .parse::<i64>()
    .expect("third character id parses");

    exec(
        &db,
        &format!("UPDATE parties SET quartermaster_character_id = {char_c} WHERE id = {party_a}"),
        "same-party quartermaster must store",
    )
    .await;
    exec(
        &db,
        &format!("DELETE FROM characters WHERE id = {char_c}"),
        "hard delete of the quartermaster",
    )
    .await;
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT count(*) FROM parties\n                 WHERE id = {party_a} AND quartermaster_character_id IS NULL"
            )
        )
        .await,
        1,
        "deleting the quartermaster must clear the designation (SET NULL), not touch the party"
    );

    db.drop_self().await;
}

// ---------------------------------------------------------------------------
// SC-3 — every join path rejects orphaned references at the database level.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc3_every_join_path_rejects_orphans() {
    let db = provision_or_skip!("sc3");
    let (party, character) = seed_character(&db, "sub-a", "Anchors").await;
    let effect = seed_effect(&db).await;
    // A second account with no character: the orphan-party insert below uses
    // it as owner so the FK fires before the one-char-per-account UNIQUE.
    exec(
        &db,
        "INSERT INTO accounts (sub, username, display_name, role)
         VALUES ('sub-charless', 'sub-charless', 'Charless', 'player')",
        "seed charless account",
    )
    .await;

    for (label, sql) in orphan_cases(party, character, effect) {
        db.assert_fails_with(&sql, FK_VIOLATION, &format!("orphan insert: {label}"))
            .await;
    }

    db.drop_self().await;
}

/// One case per FK join path: inserting a child with a missing parent.
fn orphan_cases(party: i64, character: i64, effect: i64) -> Vec<(&'static str, String)> {
    vec![
        (
            "character → party",
            "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
                 VALUES (999999, 'sub-charless', '{}', '{}')"
                .into(),
        ),
        (
            "character → account",
            format!(
                "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
                 VALUES ({party}, 'sub-nobody', '{{}}', '{{}}')"
            ),
        ),
        (
            "vitals → character",
            "INSERT INTO character_vitals (character_id) VALUES (999999)".into(),
        ),
        (
            "spell slot → character",
            "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index)
             VALUES (999999, 'Wizard', 1, 0)"
                .into(),
        ),
        (
            "inventory → character",
            "INSERT INTO character_inventory_live (character_id, item_name)
             VALUES (999999, 'Chalk')"
                .into(),
        ),
        (
            "effect → party",
            format!(
                "INSERT INTO effects (party_id, source_character_id, name)
                 VALUES (999999, {character}, 'Bless')"
            ),
        ),
        (
            "effect → creator",
            format!(
                "INSERT INTO effects (party_id, source_character_id, name)
                 VALUES ({party}, 999999, 'Bless')"
            ),
        ),
        (
            "effect target → effect",
            format!(
                "INSERT INTO effect_targets (party_id, effect_id, character_id)\n                 VALUES ({party}, 999999, {character})"
            ),
        ),
        (
            "effect target → character",
            format!(
                "INSERT INTO effect_targets (party_id, effect_id, character_id)\n                 VALUES ({party}, {effect}, 999999)"
            ),
        ),
        (
            "effect modifier → effect",
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
             VALUES (999999, 0, 'status', 'ac', 1)"
                .into(),
        ),
        (
            "stash → party",
            "INSERT INTO party_stash (party_id, item_name, quantity) VALUES (999999, 'Rope', 1)"
                .into(),
        ),
        (
            "claim → party",
            "INSERT INTO claim_history (party_id, item_name, quantity, origin, destination, actor_sub)
                 VALUES (999999, 'Rope', 1, 'stash', 'character:X', 'sub-a')"
                .into(),
        ),
        (
            "claim → actor",
            format!(
                "INSERT INTO claim_history (party_id, item_name, quantity, origin, destination, actor_sub)
                 VALUES ({party}, 'Rope', 1, 'stash', 'character:X', 'sub-nobody')"
            ),
        ),
        (
            "bank → party",
            "INSERT INTO party_bank (party_id) VALUES (999999)".into(),
        ),
        (
            "corpus → creator",
            "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub)
             VALUES ('condition', 'Frightened', 'custom', '{}', 'sub-nobody')"
                .into(),
        ),
        (
            "party quartermaster → character",
            format!("UPDATE parties SET quartermaster_character_id = 999999 WHERE id = {party}"),
        ),
    ]
}

// ---------------------------------------------------------------------------
// SC-4 — every entity table timestamps its rows; a write round-trips.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc4_every_entity_table_carries_timestamps() {
    let db = provision_or_skip!("sc4");

    for table in ALL_TABLES {
        assert_table_timestamps(&db, table).await;
    }

    // Round-trip: an app-style UPDATE moves `updated_at` forward.
    let (_, party) = seed_character(&db, "sub-a", "Clockers").await;
    let before = party_epoch(&db, party).await;
    // `now()` is transaction-start time; a real gap needs real time.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    exec(
        &db,
        &format!("UPDATE parties SET name = 'Renamed', updated_at = now() WHERE id = {party}"),
        "round-trip update",
    )
    .await;
    let after = party_epoch(&db, party).await;
    assert!(
        after > before,
        "updated_at must move forward on app-style update (before {before}, after {after})"
    );

    db.drop_self().await;
}

/// `created_at` (and `updated_at`, except `claim_history`) must be
/// `NOT NULL DEFAULT now()`.
async fn assert_table_timestamps(db: &TestDb, table: &str) {
    let column_check = |column: &str| {
        format!(
            "SELECT count(*) FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = '{table}'
             AND column_name = '{column}'
             AND is_nullable = 'NO' AND column_default LIKE 'now()%'"
        )
    };
    assert_eq!(
        count(db, &column_check("created_at")).await,
        1,
        "{table}: created_at must be NOT NULL DEFAULT now()"
    );
    let wants_updated_at = i64::from(*table != *"claim_history");
    assert_eq!(
        count(db, &column_check("updated_at")).await,
        wants_updated_at,
        "{table}: updated_at presence wrong (only claim_history lacks it)"
    );
}

/// A party row's `updated_at` as epoch microseconds (avoids pulling in a
/// timestamptz decoder feature just for tests).
async fn party_epoch(db: &TestDb, party: i64) -> i64 {
    one_string(
        db,
        &format!(
            "SELECT ((EXTRACT(EPOCH FROM updated_at) * 1000000)::bigint)::text
             FROM parties WHERE id = {party}"
        ),
    )
    .await
    .parse::<i64>()
    .expect("epoch parses")
}

// ---------------------------------------------------------------------------
// SC-5 — wholesale re-import preserves 100% of live state; orphans are kept.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc5_reimport_preserves_live_state() {
    let db = provision_or_skip!("sc5");
    let (party, character) = seed_character(&db, "sub-a", "Importers").await;

    // Live state across all three anchor schemes.
    for sql in [
        format!("UPDATE character_vitals SET hp = 17, temp_hp = 3, money_cp = 42, level_adjust = 1 WHERE character_id = {character}"),
        format!(
            "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index, used, prepared_spell)
             VALUES ({character}, 'Wizard', 1, 0, true, 'Magic Missile'),
                    ({character}, 'Wizard', 2, 0, false, NULL),
                    ({character}, 'Wellspring Gnome', 1, 0, true, NULL)"
        ),
        format!(
            "INSERT INTO character_inventory_live (character_id, item_name, qty_delta)
             VALUES ({character}, 'Chalk', -1), ({character}, 'Bedroll', 2)"
        ),
        format!(
            "INSERT INTO effects (party_id, source_character_id, name)
             VALUES ({party}, {character}, 'Bless')"
        ),
    ] {
        exec(&db, &sql, "seed live state").await;
    }

    let live_before = live_snapshot(&db, character).await;

    // Simulated re-import: payload + base sheet replaced wholesale — the new
    // base sheet does not even contain 'Chalk' anymore.
    let new_payload = r#"{"success":true,"v":2}"#;
    let new_base = r#"{"level":6,"items":["Bedroll"]}"#;
    exec(
        &db,
        &format!(
            "UPDATE characters SET payload_raw = '{new_payload}', base_sheet = '{new_base}'
             WHERE id = {character}"
        ),
        "simulated re-import",
    )
    .await;

    // Guard against a false pass: the replacement really happened.
    let stored = one_string(
        &db,
        &format!("SELECT payload_raw FROM characters WHERE id = {character}"),
    )
    .await;
    assert_eq!(stored, new_payload, "payload must actually be replaced");

    // Every live row survives, byte-for-byte, untouched.
    assert_eq!(
        live_before,
        live_snapshot(&db, character).await,
        "re-import must not touch a single live-state row (SC-5)"
    );

    // The orphan case, structurally: 'Chalk' vanished from the base sheet,
    // yet its live row survives for E5's post-import diff.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT count(*) FROM character_inventory_live
                 WHERE character_id = {character} AND item_name = 'Chalk'"
            )
        )
        .await,
        1,
        "orphaned inventory anchor must be kept, never dropped"
    );

    db.drop_self().await;
}

/// Deterministic snapshot of a character's live rows (vitals, slots,
/// inventory), stable-ordered, as text.
async fn live_snapshot(db: &TestDb, character: i64) -> String {
    one_string(
        db,
        &format!(
            "SELECT coalesce(string_agg(t.x, '|' ORDER BY t.x), '') FROM (
                SELECT 'v' || character_id || ':' || hp || ':' || temp_hp || ':'
                       || money_cp || ':' || level_adjust AS x
                FROM character_vitals WHERE character_id = {character}
                UNION ALL
                SELECT 's' || character_id || ':' || caster_key || ':' || rank || ':'
                       || slot_index || ':' || used || ':' || coalesce(prepared_spell, '')
                FROM character_spell_slots WHERE character_id = {character}
                UNION ALL
                SELECT 'i' || character_id || ':' || item_name || ':' || qty_delta
                FROM character_inventory_live WHERE character_id = {character}
             ) t"
        ),
    )
    .await
}

// ---------------------------------------------------------------------------
// SC-6 — receipt-order field writes get strictly increasing server versions.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc6_field_writes_resolve_by_receipt_order_with_increasing_versions() {
    let db = provision_or_skip!("sc6");
    let (_, character) = seed_character(&db, "sub-a", "Receivers").await;

    // Two writes to the same field, in receipt order — each its own
    // statement/transaction, so no lock is held across any client
    // round-trip between them.
    write_hp(&db, character, 10).await;
    let first = vitals_version(&db, character, "hp_version").await;
    write_hp(&db, character, 20).await;
    let second = vitals_version(&db, character, "hp_version").await;

    let hp = one_string(
        &db,
        &format!("SELECT hp::text FROM character_vitals WHERE character_id = {character}"),
    )
    .await
    .parse::<i32>()
    .expect("hp parses");
    assert_eq!(hp, 20, "the later-received write must win");
    assert!(
        second > first,
        "versions must strictly increase per field (first {first}, second {second})"
    );

    // The single global sequence gives a total order across fields: a slot
    // write stamped after the hp write carries a higher version.
    exec(
        &db,
        &format!(
            "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index, version)
             VALUES ({character}, 'Wizard', 1, 0, nextval('field_version_seq'))"
        ),
        "slot write",
    )
    .await;
    let slot = one_string(
        &db,
        &format!(
            "SELECT version::text FROM character_spell_slots
             WHERE character_id = {character} AND caster_key = 'Wizard'"
        ),
    )
    .await
    .parse::<i64>()
    .expect("slot version parses");
    assert!(
        slot > second,
        "the one sequence must order writes across fields (hp {second}, slot {slot})"
    );

    db.drop_self().await;
}

/// One receipt-order hp write: value + server-assigned version, one
/// statement, no transaction spanning a client interaction.
async fn write_hp(db: &TestDb, character: i64, hp: i32) {
    exec(
        db,
        &format!(
            "UPDATE character_vitals SET hp = {hp}, hp_version = nextval('field_version_seq'),
             updated_at = now() WHERE character_id = {character}"
        ),
        &format!("hp write {hp}"),
    )
    .await;
}

/// One vitals version column's current value.
async fn vitals_version(db: &TestDb, character: i64, column: &str) -> i64 {
    one_string(
        db,
        &format!("SELECT {column}::text FROM character_vitals WHERE character_id = {character}"),
    )
    .await
    .parse::<i64>()
    .expect("version parses")
}

// ---------------------------------------------------------------------------
// SC-7 — claim history accepts appends and nothing else.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sc7_claim_history_is_append_only() {
    let db = provision_or_skip!("sc7");
    let (party, _) = seed_character(&db, "sub-a", "Claimers").await;

    exec(
        &db,
        &format!(
            "INSERT INTO claim_history (party_id, item_name, quantity, origin, destination, actor_sub)
             VALUES ({party}, 'Rope', 1, 'stash', 'character:Hero', 'sub-a')"
        ),
        "claim insert must succeed",
    )
    .await;

    db.assert_fails_with(
        &format!("UPDATE claim_history SET quantity = 2 WHERE party_id = {party}"),
        TRIGGER_EXCEPTION,
        "claim UPDATE must be rejected",
    )
    .await;
    db.assert_fails_with(
        &format!("DELETE FROM claim_history WHERE party_id = {party}"),
        TRIGGER_EXCEPTION,
        "claim DELETE must be rejected",
    )
    .await;
    assert_eq!(
        count(&db, "SELECT count(*) FROM claim_history").await,
        1,
        "the appended claim row must still be there untouched"
    );

    db.drop_self().await;
}

// ---------------------------------------------------------------------------
// US5 — corpus lanes, provenance, and the display-only tier test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn us5_corpus_lanes_and_provenance() {
    let db = provision_or_skip!("us5");

    // Exactly three lanes are storable.
    db.assert_fails_with(
        "INSERT INTO corpus_entries (kind, name, lane, data)
         VALUES ('condition', 'Weird', 'banana', '{}')",
        CHECK_VIOLATION,
        "a fourth lane must be unstorable",
    )
    .await;

    seed_corpus_rows(&db).await;
    assert_upsert_scope_around_custom_rows(&db).await;

    // Display-only rows are distinguishable without parsing prose: the tier
    // test is `modifiers IS NULL`, applied per row.
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM corpus_entries
             WHERE lane = 'custom' AND name = '500 Toads' AND modifiers IS NULL"
        )
        .await,
        1,
        "the custom row is display-only (modifiers IS NULL)"
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM corpus_entries WHERE modifiers IS NOT NULL"
        )
        .await,
        1,
        "the imported row carries structured modifiers"
    );

    db.drop_self().await;
}

/// The imported row (with provenance + modifiers) and a custom row (creator,
/// no source id), plus the idempotency-key collision check.
async fn seed_corpus_rows(db: &TestDb) {
    exec(
        db,
        "INSERT INTO corpus_entries (kind, name, lane, data, modifiers, source_id, pack_version, imported_at)
         VALUES ('condition', 'Frightened', 'imported', '{}',
                 '[{\"type\":\"status\",\"stat\":\"all_checks_and_dcs\",\"value\":-2}]'::jsonb,
                 'cond-frightened', '7.5.1', now())",
        "imported row insert",
    )
    .await;

    exec(
        db,
        "INSERT INTO accounts (sub, username, display_name, role)
         VALUES ('sub-dave', 'sub-dave', 'Dave', 'player')",
        "custom creator account",
    )
    .await;
    exec(
        db,
        "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub)
         VALUES ('spell', '500 Toads', 'custom', '{}', 'sub-dave')",
        "custom row insert",
    )
    .await;

    // The idempotency key collides on (kind, source_id).
    db.assert_fails_with(
        "INSERT INTO corpus_entries (kind, name, lane, data, source_id, pack_version, imported_at)
         VALUES ('condition', 'Frightened II', 'imported', '{}', 'cond-frightened', '7.5.1', now())",
        UNIQUE_VIOLATION,
        "same (kind, source_id) must collide on the idempotency key",
    )
    .await;
}

/// A re-run upserting a pack row cannot land on the custom row: it has no
/// `source_id`, so it sits outside the partial index's scope by construction.
async fn assert_upsert_scope_around_custom_rows(db: &TestDb) {
    exec(
        db,
        "INSERT INTO corpus_entries (kind, name, lane, data, source_id, pack_version, imported_at)
         VALUES ('spell', '500 Toads (pack)', 'imported', '{}', 'spell-500-toads', '7.5.1', now())
         ON CONFLICT (kind, source_id) WHERE source_id IS NOT NULL
         DO UPDATE SET data = EXCLUDED.data",
        "pack upsert beside custom row",
    )
    .await;

    assert_eq!(
        count(
            db,
            "SELECT count(*) FROM corpus_entries WHERE lane = 'custom' AND name = '500 Toads'"
        )
        .await,
        1,
        "re-runs must not touch custom rows"
    );
    let creator = one_string(
        db,
        "SELECT created_by_sub FROM corpus_entries WHERE lane = 'custom' AND name = '500 Toads'",
    )
    .await;
    assert_eq!(creator, "sub-dave", "custom rows must record their creator");
}

// ---------------------------------------------------------------------------
// FR-7 + constraint spot checks — identity anchors and domain CHECKs.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_anchors_and_domain_checks_hold() {
    let db = provision_or_skip!("identity");
    let (party, character) = seed_character(&db, "sub-a", "Anchors").await;

    // One character per account — a second collides on owner_sub.
    db.assert_fails_with(
        &format!(
            "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
             SELECT party_id, owner_sub, '{{}}', '{{}}' FROM characters WHERE id = {character}"
        ),
        UNIQUE_VIOLATION,
        "a second character for one account must be rejected",
    )
    .await;

    // Bogus roles are unstorable.
    db.assert_fails_with(
        "INSERT INTO accounts (sub, username, display_name, role)
         VALUES ('sub-x', 'sub-x', 'X', 'archmage')",
        CHECK_VIOLATION,
        "account role is closed to player|gm",
    )
    .await;

    // Level adjust is a delta clamped to ±19 (effective 1–20 is app logic).
    db.assert_fails_with(
        "INSERT INTO character_vitals (character_id, level_adjust) VALUES (999999, 20)",
        CHECK_VIOLATION,
        "level_adjust beyond ±19 must be rejected",
    )
    .await;

    // Slot ranks follow the fixture's 11-element perDay (0 = cantrip).
    db.assert_fails_with(
        &format!(
            "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index)
             VALUES ({character}, 'Wizard', 11, 0)"
        ),
        CHECK_VIOLATION,
        "rank 11 must be rejected",
    )
    .await;

    // Negative hp is unstorable.
    db.assert_fails_with(
        &format!("UPDATE character_vitals SET hp = -1 WHERE character_id = {character}"),
        CHECK_VIOLATION,
        "negative hp must be rejected",
    )
    .await;

    assert_modifier_vocabulary(&db).await;
    let _ = party;

    db.drop_self().await;
}

/// Modifier vocabulary: types closed, stat non-empty, signed value free.
async fn assert_modifier_vocabulary(db: &TestDb) {
    let effect = seed_effect(db).await;
    db.assert_fails_with(
        &format!(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
             VALUES ({effect}, 0, 'morale', 'ac', 1)"
        ),
        CHECK_VIOLATION,
        "modifier type is closed to the stacking types",
    )
    .await;
    db.assert_fails_with(
        &format!(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
             VALUES ({effect}, 0, 'status', '', 1)"
        ),
        CHECK_VIOLATION,
        "empty stat must be rejected",
    )
    .await;
    // Penalties are negative values, not a type — a signed value stores fine.
    exec(
        db,
        &format!(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
             VALUES ({effect}, 0, 'status', 'skill:stealth', -2)"
        ),
        "negative modifier value must store",
    )
    .await;
}

// ---------------------------------------------------------------------------
// Suite gating — a reachable server must never be skippable: wrong
// credentials, permission failures, or a missing maintenance
// database are RED; only transport-level unreachability skips.
// ---------------------------------------------------------------------------

/// Live regression: with a reachable server, connecting with a wrong
/// password must classify Fatal (red), never Skip. If the good URL cannot
/// connect at all, there is no server here — skip loudly like every test.
#[tokio::test]
async fn setup_wrong_password_fails_red_on_reachable_server() {
    let base = base_url();

    // Prove the environment: correct credentials must connect.
    match connect_maintenance(&base, "setup-auth").await {
        MaintenanceConnect::Up(pool) => pool.close().await,
        MaintenanceConnect::Skip(reason) => {
            eprintln!("{reason}");
            return;
        }
        MaintenanceConnect::Fatal(context) => {
            panic!("correct credentials must connect; got a fatal classification: {context}")
        }
    }

    // Same host and port, wrong password: the server answers with an
    // authentication error, which must be Fatal — a red — and never Skip.
    let bad = corrupt_password(&base);
    match connect_maintenance(&bad, "setup-auth").await {
        MaintenanceConnect::Fatal(_) => {} // the required outcome
        MaintenanceConnect::Skip(reason) => panic!(
            "a reachable server's password rejection classified as a skip — \
             a misconfigured database would pass the suite: {reason}"
        ),
        MaintenanceConnect::Up(pool) => {
            pool.close().await;
            // The host accepts any password (trust auth), so there is no
            // server rejection to classify here. Say so — never fake a pass.
            eprintln!(
                "note: server at {base} accepted a wrong password (trust auth); \
                 the auth-rejection classification is pinned by the unit tests below"
            );
        }
    }
}

#[test]
fn transport_failures_classify_as_skip() {
    assert!(is_transport_unreachable(&sqlx::Error::Io(
        std::io::Error::from(std::io::ErrorKind::ConnectionRefused)
    )));
    assert!(is_transport_unreachable(&sqlx::Error::Io(
        std::io::Error::from(std::io::ErrorKind::TimedOut)
    )));
    // A firewalled host that silently drops SYNs surfaces as PoolTimedOut.
    assert!(is_transport_unreachable(&sqlx::Error::PoolTimedOut));
}

#[test]
fn server_answered_failures_classify_as_fatal() {
    // A password rejection (28P01) is the server SPEAKING — reachable.
    let auth = sqlx::Error::Database(Box::new(MockDbError::auth_failed()));
    assert!(
        !is_transport_unreachable(&auth),
        "an authentication rejection must never ride the skip path"
    );
    // A broken URL is our config bug, not the server's absence.
    let config = sqlx::Error::Configuration("no host in url".into());
    assert!(
        !is_transport_unreachable(&config),
        "a configuration error must never ride the skip path"
    );
}

#[test]
fn corrupt_password_swaps_only_the_password() {
    assert_eq!(
        corrupt_password("postgres://hireling:hireling@127.0.0.1:5432"),
        "postgres://hireling:definitely-wrong-password@127.0.0.1:5432"
    );
    // No password in the URL: one is injected, so password-verifying hosts
    // reject the connection even when the operator URL omitted credentials.
    assert_eq!(
        corrupt_password("postgres://127.0.0.1:5432"),
        "postgres://hireling:definitely-wrong-password@127.0.0.1:5432"
    );
}

/// Stand-in for a Postgres server error: `PgDatabaseError` cannot be
/// constructed outside the driver, so the classifier's Database branch is
/// pinned with the trait's minimal implementation carrying a real SQLSTATE.
#[derive(Debug)]
struct MockDbError {
    code: &'static str,
    message: String,
}

impl MockDbError {
    fn auth_failed() -> Self {
        Self {
            code: "28P01",
            message: "password authentication failed for user \"hireling\"".to_owned(),
        }
    }
}

impl std::fmt::Display for MockDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MockDbError {}

impl sqlx::error::DatabaseError for MockDbError {
    fn message(&self) -> &str {
        &self.message
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some(self.code.into())
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }
}
