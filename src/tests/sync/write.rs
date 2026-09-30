//! Integration tests for E7's per-field CAS write engine (plan Task 3):
//! bounds → authz → ledger dedupe → CAS → ledger insert, one transaction,
//! against a real Postgres via the shared `testing::test_pool` harness.

use serde_json::json;
use sqlx::Row as _;

use crate::auth::authz::{Actor, Role};
use crate::sync::protocol::{FieldTarget, Outcome, VitalsField};
use crate::sync::write::{ClientOp, apply_write};
use crate::testing;

/// Seed one account + one party character + its vitals row; return the
/// character id. The POC party from the migration seed is the party.
async fn seed_character(pool: &sqlx::PgPool, sub: &str) -> i64 {
    testing::seed_account(pool, sub, "player").await;
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         SELECT id, $1, '{}', '{}' FROM parties ORDER BY id LIMIT 1 RETURNING id",
    )
    .bind(sub)
    .fetch_one(pool)
    .await
    .expect("seed character");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(character_id)
        .execute(pool)
        .await
        .expect("seed vitals row");
    character_id
}

/// One scalar column of the character's vitals row.
async fn vitals_i32(pool: &sqlx::PgPool, character_id: i64, column: &str) -> i32 {
    // Column names come from the test file, never from user input — the
    // AssertSqlSafe audit is honest (same policy as src/tests/db.rs).
    let sql = format!("SELECT {column} FROM character_vitals WHERE character_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("read vitals {column}: {err}"))
}

async fn vitals_version(pool: &sqlx::PgPool, character_id: i64, column: &str) -> i64 {
    let sql = format!("SELECT {column} FROM character_vitals WHERE character_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("read vitals {column}: {err}"))
}

/// The one `client_ops` row for `op_id`, as (`outcome`, `resulting_version`).
async fn ledger_row(pool: &sqlx::PgPool, op_id: &str) -> (String, Option<i64>) {
    let row = sqlx::query("SELECT outcome, resulting_version FROM client_ops WHERE op_id = $1")
        .bind(op_id)
        .fetch_one(pool)
        .await
        .expect("ledger row present");
    let outcome: String = row.get("outcome");
    let version: Option<i64> = row.get("resulting_version");
    (outcome, version)
}

fn player(sub: &str) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role: Role::Player,
    }
}

fn hp_op(op_id: &str, character_id: i64, base_version: i64, hp: i32) -> ClientOp {
    ClientOp {
        op_id: op_id.to_owned(),
        target: FieldTarget::Vitals {
            character_id,
            field: VitalsField::Hp,
        },
        base_version,
        value: json!(hp),
    }
}

#[tokio::test]
async fn an_owner_write_at_the_current_version_applies() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;

    let result = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-1", character_id, base, 14),
    )
    .await
    .expect("write applies");

    assert_eq!(
        result.outcome,
        Outcome::Applied,
        "owner at current version wins"
    );
    // The version source is the ONE GLOBAL sequence (E2): the new version is
    // strictly newer than base, not base+1 — other columns' draws advance it.
    let applied_version = result.version.expect("applied writes carry a version");
    assert!(applied_version > base, "strictly newer than base");
    assert_eq!(
        vitals_i32(&pool, character_id, "hp").await,
        14,
        "row updated"
    );
    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        applied_version,
        "row version == returned version"
    );
    let (outcome, version) = ledger_row(&pool, "op-1").await;
    assert_eq!(outcome, "applied", "ledger records the win");
    assert_eq!(
        version,
        Some(applied_version),
        "ledger records the resulting version"
    );
    testing::drop_test_db(pool, "write_applied").await;
}

#[tokio::test]
async fn a_replayed_op_is_already_applied_with_the_original_version() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;
    let first = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-1", character_id, base, 14),
    )
    .await
    .expect("first write");
    let applied_version = first.version.expect("applied writes carry a version");

    let replay = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-1", character_id, base, 14),
    )
    .await
    .expect("replay");

    assert_eq!(replay.outcome, Outcome::AlreadyApplied, "op_id dedupes");
    assert_eq!(replay.version, first.version, "the original's version");
    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        applied_version,
        "no second version bump"
    );
    assert_eq!(
        vitals_i32(&pool, character_id, "hp").await,
        14,
        "value unchanged"
    );
    testing::drop_test_db(pool, "write_replay").await;
}

#[tokio::test]
async fn a_stale_base_write_is_superseded() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;
    let winning = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-1", character_id, base, 14),
    )
    .await
    .expect("winning write");
    let winning_version = winning.version.expect("applied writes carry a version");

    let stale = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-2", character_id, base, 20),
    )
    .await
    .expect("stale write resolves");

    assert_eq!(stale.outcome, Outcome::Superseded, "lost the CAS race");
    assert_eq!(
        stale.winning_version,
        Some(winning_version),
        "reports the winner"
    );
    assert_eq!(
        vitals_i32(&pool, character_id, "hp").await,
        14,
        "row unchanged"
    );
    let (outcome, version) = ledger_row(&pool, "op-2").await;
    assert_eq!(outcome, "superseded", "ledger records the loss");
    assert_eq!(version, None, "nothing was written");
    testing::drop_test_db(pool, "write_superseded").await;
}

#[tokio::test]
async fn out_of_bounds_values_are_rejected_never_applied() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;

    let negative = apply_write(
        &pool,
        &player("sub-owner"),
        hp_op("op-neg", character_id, base, -1),
    )
    .await
    .expect("bounds check resolves");
    assert_eq!(negative.outcome, Outcome::Rejected, "hp ≥ 0 is a bound");
    assert!(
        negative
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("hp")),
        "the reason names the bound: {:?}",
        negative.reason
    );

    let level = apply_write(
        &pool,
        &player("sub-owner"),
        ClientOp {
            op_id: "op-level".to_owned(),
            target: FieldTarget::Vitals {
                character_id,
                field: VitalsField::LevelAdjust,
            },
            base_version: vitals_version(&pool, character_id, "level_adjust_version").await,
            value: json!(25),
        },
    )
    .await
    .expect("bounds check resolves");
    assert_eq!(level.outcome, Outcome::Rejected, "level_adjust ∈ −19..=19");

    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        base,
        "rejected writes touch nothing"
    );
    let (outcome, _) = ledger_row(&pool, "op-neg").await;
    assert_eq!(outcome, "rejected", "the rejection is recorded");
    testing::drop_test_db(pool, "write_rejected").await;
}

#[tokio::test]
async fn the_gm_and_non_owners_are_forbidden() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    seed_character(&pool, "sub-other").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;

    testing::seed_account(&pool, "sub-gm", "gm").await;
    let gm = Actor {
        sub: "sub-gm".to_owned(),
        role: Role::Gm,
    };
    let gm_write = apply_write(&pool, &gm, hp_op("op-gm", character_id, base, 10))
        .await
        .expect("gm write resolves");
    assert_eq!(
        gm_write.outcome,
        Outcome::Forbidden,
        "the GM writes nothing"
    );

    let outsider = apply_write(
        &pool,
        &player("sub-other"),
        hp_op("op-out", character_id, base, 10),
    )
    .await
    .expect("non-owner write resolves");
    assert_eq!(outsider.outcome, Outcome::Forbidden, "not the owner");

    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        base,
        "forbidden writes touch nothing"
    );
    let (outcome, _) = ledger_row(&pool, "op-gm").await;
    assert_eq!(outcome, "forbidden", "the denial is recorded");
    testing::drop_test_db(pool, "write_forbidden").await;
}

#[tokio::test]
async fn concurrent_same_field_writes_produce_exactly_one_winner() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;
    let writer = player("sub-owner");

    let (a, b) = tokio::join!(
        apply_write(&pool, &writer, hp_op("op-a", character_id, base, 11)),
        apply_write(&pool, &writer, hp_op("op-b", character_id, base, 22)),
    );
    let a = a.expect("writer a resolves");
    let b = b.expect("writer b resolves");
    let final_version = vitals_version(&pool, character_id, "hp_version").await;
    assert_eq!(
        final_version,
        [a.version, b.version]
            .into_iter()
            .flatten()
            .max()
            .expect("one writer applied"),
        "the winner's version is the row's"
    );

    let outcomes = [a.outcome, b.outcome];
    let mut applied = 0;
    let mut superseded = 0;
    for outcome in outcomes {
        match outcome {
            Outcome::Applied => applied += 1,
            Outcome::Superseded => superseded += 1,
            Outcome::AlreadyApplied | Outcome::Rejected | Outcome::Forbidden => {
                panic!("race produced {outcome:?}")
            }
        }
    }
    assert_eq!(applied, 1, "exactly one winner");
    assert_eq!(superseded, 1, "exactly one loser");
    assert!(final_version > base, "the field advanced exactly once");
    testing::drop_test_db(pool, "write_race").await;
}

/// One applied write of `op`, asserting the outcome so the caller reads flat.
async fn applied(
    pool: &sqlx::PgPool,
    actor: &Actor,
    op: ClientOp,
) -> crate::sync::write::WriteResult {
    let result = apply_write(pool, actor, op).await.expect("write resolves");
    assert_eq!(
        result.outcome,
        Outcome::Applied,
        "expected an applied write"
    );
    result
}

/// Seed the Wizard rank-3 slot 0 and the Chalk row for a fresh character;
/// return (`character_id`, `slot_version`, `inv_version`).
async fn seed_slot_and_item(pool: &sqlx::PgPool) -> (i64, i64, i64) {
    let character_id = seed_character(pool, "sub-owner").await;
    sqlx::query(
        "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index) \
         VALUES ($1, 'Wizard', 3, 0)",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("seed slot");
    sqlx::query(
        "INSERT INTO character_inventory_live (character_id, item_name) VALUES ($1, 'Chalk')",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("seed item");
    let slot_version: i64 = sqlx::query_scalar(
        "SELECT version FROM character_spell_slots \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 3 AND slot_index = 0",
    )
    .bind(character_id)
    .fetch_one(pool)
    .await
    .expect("slot version");
    let inv_version: i64 = sqlx::query_scalar(
        "SELECT version FROM character_inventory_live \
         WHERE character_id = $1 AND item_name = 'Chalk'",
    )
    .bind(character_id)
    .fetch_one(pool)
    .await
    .expect("inv version");
    (character_id, slot_version, inv_version)
}

#[tokio::test]
async fn a_slot_write_cas_on_the_slot_row_version() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (character_id, slot_version, _) = seed_slot_and_item(&pool).await;
    let writer = player("sub-owner");
    applied(
        &pool,
        &writer,
        ClientOp {
            op_id: "op-slot".to_owned(),
            target: FieldTarget::Slot {
                character_id,
                caster_key: "Wizard".to_owned(),
                rank: 3,
                slot_index: 0,
            },
            base_version: slot_version,
            value: json!({"used": true}),
        },
    )
    .await;

    let (used, prepared): (bool, Option<String>) = sqlx::query_as(
        "SELECT used, prepared_spell FROM character_spell_slots \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 3 AND slot_index = 0",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .expect("slot row");
    assert!(used, "used set");
    assert_eq!(prepared, None, "whole-slot write: unmentioned field resets");
    testing::drop_test_db(pool, "write_slot").await;
}

#[tokio::test]
async fn an_inv_write_cas_on_the_item_row_version() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (character_id, _, inv_version) = seed_slot_and_item(&pool).await;
    let writer = player("sub-owner");
    applied(
        &pool,
        &writer,
        ClientOp {
            op_id: "op-inv".to_owned(),
            target: FieldTarget::Inv {
                character_id,
                item_name: "Chalk".to_owned(),
            },
            base_version: inv_version,
            value: json!({"qty_delta": -2}),
        },
    )
    .await;

    let qty: i32 = sqlx::query_scalar(
        "SELECT qty_delta FROM character_inventory_live \
         WHERE character_id = $1 AND item_name = 'Chalk'",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .expect("qty");
    assert_eq!(qty, -2, "delta set");
    testing::drop_test_db(pool, "write_inv").await;
}

#[tokio::test]
async fn a_money_write_sets_all_four_denominations_under_one_version() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let character_id = seed_character(&pool, "sub-owner").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;
    let money_version = vitals_version(&pool, character_id, "money_version").await;
    let writer = player("sub-owner");
    applied(
        &pool,
        &writer,
        ClientOp {
            op_id: "op-money".to_owned(),
            target: FieldTarget::Vitals {
                character_id,
                field: VitalsField::Money,
            },
            base_version: money_version,
            value: json!({"pp": 1, "gp": 2, "sp": 3, "cp": 4}),
        },
    )
    .await;

    assert_eq!(
        vitals_i32(&pool, character_id, "money_pp").await,
        1,
        "pp set"
    );
    assert_eq!(
        vitals_i32(&pool, character_id, "money_gp").await,
        2,
        "gp set"
    );
    assert_eq!(
        vitals_i32(&pool, character_id, "money_sp").await,
        3,
        "sp set"
    );
    assert_eq!(
        vitals_i32(&pool, character_id, "money_cp").await,
        4,
        "cp set"
    );
    assert!(
        vitals_version(&pool, character_id, "money_version").await > money_version,
        "one money version bump"
    );
    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        base,
        "hp untouched by other fields"
    );
    testing::drop_test_db(pool, "write_money").await;
}
