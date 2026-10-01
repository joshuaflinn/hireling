//! Integration tests for E5's two raised migrations (design §5) — the POC
//! party seed and the `character_import` audit kind — against a real
//! Postgres via the shared `testing::test_pool` harness (a fresh, fully
//! migrated throwaway database per test).

use sqlx::Row as _;

use crate::testing;

/// Execute one statement, panicking with context when it fails.
///
/// Every SQL string here is test-authored or read from our own checked-in
/// migration files — no user input — so the [`AssertSqlSafe`] audit is honest
/// (same policy as `src/tests/db.rs`).
async fn exec(pool: &sqlx::PgPool, sql: &str, label: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap_or_else(|err| panic!("{label}: {err}"));
}

/// Execute a multi-statement migration file verbatim (simple protocol, like
/// the sqlx migrate runner uses).
async fn exec_script(pool: &sqlx::PgPool, sql: &str, label: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap_or_else(|err| panic!("{label}: {err}"));
}

/// Count rows of a single-count select.
async fn count(pool: &sqlx::PgPool, sql: &str) -> i64 {
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("count query failed ({sql}): {err}"));
    row.get::<i64, _>(0)
}

/// The verbatim up/down SQL of a migration, read from `migrations/`.
fn migration_sql(suffix: &str) -> String {
    migration_file(&format!("2026092400000{suffix}"))
}

/// The verbatim SQL of a migration file, read from `migrations/` by name.
fn migration_file(filename: &str) -> String {
    let path = format!("{}/migrations/{filename}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("migration file {path} must be readable: {err}"))
}

#[tokio::test]
async fn fresh_boot_seeds_exactly_one_poc_party() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    assert_eq!(
        count(&pool, "SELECT count(*) FROM parties").await,
        1,
        "the seed creates the single POC party"
    );
    let name: String = sqlx::query_scalar("SELECT name FROM parties")
        .fetch_one(&pool)
        .await
        .expect("party row");
    assert_eq!(name, "POC Party", "the seeded party's name");
    testing::drop_test_db(pool, "poc_party_seed").await;
}

#[tokio::test]
async fn the_seed_guard_is_idempotent_under_re_run() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // Re-apply the seed statement verbatim (sqlx skips applied migrations;
    // the guard is what keeps a manual re-run honest).
    exec(&pool, &migration_sql("8_poc_party_seed.sql"), "seed re-run").await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM parties").await,
        1,
        "still exactly one party"
    );
    testing::drop_test_db(pool, "poc_party_seed_idempotent").await;
}

#[tokio::test]
async fn an_operator_created_party_is_never_overwritten() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    exec(
        &pool,
        "DELETE FROM parties", // clear the seed: simulates pre-existing empty
        "clear seeded party",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO parties (name) VALUES ('Tuesday Table')",
        "operator party",
    )
    .await;
    exec(&pool, &migration_sql("8_poc_party_seed.sql"), "seed re-run").await;
    let rows: i64 = count(&pool, "SELECT count(*) FROM parties").await;
    assert_eq!(rows, 1, "a populated parties table is left alone");
    let name: String = sqlx::query_scalar("SELECT name FROM parties")
        .fetch_one(&pool)
        .await
        .expect("party row");
    assert_eq!(name, "Tuesday Table", "the operator's party survives");
    testing::drop_test_db(pool, "poc_party_seed_guard").await;
}

#[tokio::test]
async fn character_import_audits_are_admitted_and_garbage_still_rejected() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    exec(
        &pool,
        "INSERT INTO audit_events (occurred_at, actor_sub, event, target, outcome) \
         VALUES (now(), 'dev-sub-josh', 'character_import', 'character:1', 'allowed')",
        "character_import audit row",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO audit_events (occurred_at, event, target, outcome) \
         VALUES (now(), 'character_import', 'import:not-pathbuilder', 'denied')",
        "denied import audit row (null actor)",
    )
    .await;
    let result = sqlx::query(
        "INSERT INTO audit_events (occurred_at, event, target, outcome) \
         VALUES (now(), 'definitely_not_an_event', 'x', 'allowed')",
    )
    .execute(&pool)
    .await;
    assert!(result.is_err(), "garbage events are still rejected");
    testing::drop_test_db(pool, "audit_import_event").await;
}

#[tokio::test]
async fn down_restores_the_seed_and_the_seven_kind_check() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // Down of the audit migration: character_import rejected again, the
    // original seven still fine.
    exec_script(
        &pool,
        &migration_sql("9_audit_import_event.down.sql"),
        "audit down",
    )
    .await;
    let rejected = sqlx::query(
        "INSERT INTO audit_events (occurred_at, event, target, outcome) \
         VALUES (now(), 'character_import', 'character:1', 'allowed')",
    )
    .execute(&pool)
    .await;
    assert!(rejected.is_err(), "character_import is closed again");
    exec(
        &pool,
        "INSERT INTO audit_events (occurred_at, event, target, outcome) \
         VALUES (now(), 'login_success', 'dev-sub-josh', 'allowed')",
        "the original kinds still audit fine",
    )
    .await;

    // Down of the seed: party removed while unoccupied…
    exec_script(
        &pool,
        &migration_sql("8_poc_party_seed.down.sql"),
        "seed down",
    )
    .await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM parties").await,
        0,
        "the unoccupied seed row is removed"
    );
    testing::drop_test_db(pool, "migrations_down").await;
}

// E7: the `client_ops` idempotency ledger (specs/007 data-model.md).
#[tokio::test]
async fn client_ops_ledger_holds_its_shape() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    exec(
        &pool,
        "INSERT INTO accounts (sub, username, display_name, role) \
         VALUES ('dev-sub-ops', 'ops', 'ops', 'player')",
        "account for ledger rows",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome, resulting_version) \
         VALUES ('op-1', 'dev-sub-ops', 'vitals:hp', '{}', 'applied', 1043)",
        "applied ledger row",
    )
    .await;

    // op_id is the dedupe key — a second row with the same id is a client bug
    // the PK catches loudly (data-model.md).
    let duplicate = sqlx::query(
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome) \
         VALUES ('op-1', 'dev-sub-ops', 'vitals:hp', '{}', 'superseded')",
    )
    .execute(&pool)
    .await;
    assert!(duplicate.is_err(), "op_id must be the primary key");

    // Outcome vocabulary is closed; `already_applied` is deliberately absent —
    // it is a read answer, never a stored outcome.
    let bogus = sqlx::query(
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome) \
         VALUES ('op-2', 'dev-sub-ops', 'vitals:hp', '{}', 'bogus')",
    )
    .execute(&pool)
    .await;
    assert!(bogus.is_err(), "outcome CHECK must reject unknown outcomes");
    let already = sqlx::query(
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome) \
         VALUES ('op-2b', 'dev-sub-ops', 'vitals:hp', '{}', 'already_applied')",
    )
    .execute(&pool)
    .await;
    assert!(already.is_err(), "already_applied is never stored");

    // FK RESTRICT: removing an account with ledger history is refused.
    let delete = sqlx::query("DELETE FROM accounts WHERE sub = 'dev-sub-ops'")
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "account delete must be RESTRICTed by ledger rows"
    );

    testing::drop_test_db(pool, "client_ops_shape").await;
}

#[tokio::test]
async fn client_ops_up_down_up_round_trips() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // Down: the table goes; nothing else references it (data-model.md).
    exec_script(
        &pool,
        &migration_file("20261001000001_client_ops.down.sql"),
        "client_ops down",
    )
    .await;
    let gone = sqlx::query("SELECT 1 FROM client_ops").execute(&pool).await;
    assert!(gone.is_err(), "the down migration must drop client_ops");

    // Up again: the table returns and accepts a row.
    exec_script(
        &pool,
        &migration_file("20261001000001_client_ops.sql"),
        "client_ops up (round trip)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO accounts (sub, username, display_name, role) \
         VALUES ('dev-sub-rt', 'rt', 'rt', 'player')",
        "account for round trip",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome) \
         VALUES ('op-rt', 'dev-sub-rt', 'inv:Chalk', '{}', 'rejected')",
        "row after round trip",
    )
    .await;

    testing::drop_test_db(pool, "client_ops_round_trip").await;
}

#[tokio::test]
async fn down_spares_an_occupied_party() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    exec(
        &pool,
        "INSERT INTO accounts (sub, username, display_name, role) \
         VALUES ('dev-sub-josh', 'josh', 'josh', 'player')",
        "account for the character",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         SELECT id, 'dev-sub-josh', '{}', '{}' FROM parties LIMIT 1",
        "character occupies the POC party",
    )
    .await;
    exec_script(
        &pool,
        &migration_sql("8_poc_party_seed.down.sql"),
        "seed down",
    )
    .await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM parties").await,
        1,
        "an occupied party survives the down"
    );
    testing::drop_test_db(pool, "migrations_down_occupied").await;
}

// E6: the spell-economy vitals fields (specs/006 design §3).
#[tokio::test]
async fn vitals_spell_economy_columns_round_trip() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    exec(
        &pool,
        "INSERT INTO accounts (sub, username, display_name, role) \
         VALUES ('dev-sub-e6', 'e6', 'e6', 'player')",
        "account for the vitals row",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         SELECT id, 'dev-sub-e6', '{}', '{}' FROM parties LIMIT 1",
        "character for the vitals row",
    )
    .await;

    // The up defaults: focus 0, hero 0, daily zeroed — each with its own
    // version drawn from the one global sequence.
    exec(
        &pool,
        "INSERT INTO character_vitals (character_id) \
         SELECT id FROM characters WHERE owner_sub = 'dev-sub-e6'",
        "vitals row with the new defaults",
    )
    .await;
    let (focus, focus_version): (i32, i64) =
        sqlx::query_as("SELECT focus_current, focus_version FROM character_vitals")
            .fetch_one(&pool)
            .await
            .expect("focus defaults");
    assert_eq!(focus, 0, "focus starts empty");
    assert!(focus_version > 0, "focus_version drawn from the sequence");
    let hero: i32 = sqlx::query_scalar("SELECT hero_points FROM character_vitals")
        .fetch_one(&pool)
        .await
        .expect("hero default");
    assert_eq!(hero, 0);
    let daily: serde_json::Value = sqlx::query_scalar("SELECT daily FROM character_vitals")
        .fetch_one(&pool)
        .await
        .expect("daily default");
    assert_eq!(
        daily,
        serde_json::json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false}),
        "the daily row starts zeroed"
    );

    // The value CHECKs hold at the database too.
    let negative_focus = sqlx::query("UPDATE character_vitals SET focus_current = -1")
        .execute(&pool)
        .await;
    assert!(negative_focus.is_err(), "focus_current >= 0 is a CHECK");

    // Down: the columns go, and a fresh up accepts a row again.
    exec_script(
        &pool,
        &migration_file("20261002000001_vitals_spell_economy.down.sql"),
        "spell economy down",
    )
    .await;
    let gone = sqlx::query("SELECT focus_current FROM character_vitals")
        .execute(&pool)
        .await;
    assert!(gone.is_err(), "the down migration must drop focus_current");
    exec_script(
        &pool,
        &migration_file("20261002000001_vitals_spell_economy.sql"),
        "spell economy up (round trip)",
    )
    .await;

    testing::drop_test_db(pool, "vitals_spell_economy").await;
}
