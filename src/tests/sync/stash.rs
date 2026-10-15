//! E12 lane L2 (plan Task 2, design D2): the four party field kinds and
//! their apply semantics — additive `qty`/`bank` (never `superseded`),
//! CAS `meta`/`party`, the `stash_new` mint with FR-1's duplicate refusal,
//! party-scoped authz, and the audited denials. Every test rides the
//! production write path (`apply_write`), real Postgres.

use serde_json::{Value as JsonValue, json};
use sqlx::Row as _;

use crate::auth::authz::{Actor, Role};
use crate::sync::protocol::{FieldTarget, Outcome, ServerFrame};
use crate::sync::test_helpers;
use crate::sync::write::{ClientOp, apply_write};
use crate::testing;

/// Seed one member (account + roster character + vitals) in the POC party;
/// return (`party_id`, `character_id`).
async fn seed_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
    test_helpers::seed_member(pool, sub).await
}

fn player(sub: &str) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role: Role::Player,
    }
}

/// One stash row as (`quantity`, `bulk`, `notes`, `version`) — bulk is
/// nullable (the schema's negligible/unknown convention).
async fn stash_row(
    pool: &sqlx::PgPool,
    party_id: i64,
    item_name: &str,
) -> Option<(i32, Option<f64>, String, i64)> {
    let row = sqlx::query(
        "SELECT quantity, bulk::float8 AS bulk, notes, version \
         FROM party_stash WHERE party_id = $1 AND item_name = $2",
    )
    .bind(party_id)
    .bind(item_name)
    .fetch_optional(pool)
    .await
    .expect("stash row read");
    row.map(|row| {
        (
            row.get("quantity"),
            row.get("bulk"),
            row.get("notes"),
            row.get("version"),
        )
    })
}

/// How many stash rows carry this exact name in the party.
async fn stash_row_count(pool: &sqlx::PgPool, party_id: i64, item_name: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM party_stash WHERE party_id = $1 AND item_name = $2")
        .bind(party_id)
        .bind(item_name)
        .fetch_one(pool)
        .await
        .expect("stash row count")
}

/// The party bank as (`gp`, `sp`, `cp`, `version`).
async fn bank_row(pool: &sqlx::PgPool, party_id: i64) -> (i32, i32, i32, i64) {
    let row = sqlx::query("SELECT gp, sp, cp, version FROM party_bank WHERE party_id = $1")
        .bind(party_id)
        .fetch_one(pool)
        .await
        .expect("bank row read");
    (
        row.get("gp"),
        row.get("sp"),
        row.get("cp"),
        row.get("version"),
    )
}

/// Claim rows for one item label.
async fn claim_count(pool: &sqlx::PgPool, party_id: i64, item_name: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM claim_history WHERE party_id = $1 AND item_name = $2")
        .bind(party_id)
        .bind(item_name)
        .fetch_one(pool)
        .await
        .expect("claim count")
}

/// The party's quartermaster designation and its version.
async fn quartermaster(pool: &sqlx::PgPool, party_id: i64) -> (Option<i64>, i64) {
    let row = sqlx::query(
        "SELECT quartermaster_character_id, quartermaster_version \
         FROM parties WHERE id = $1",
    )
    .bind(party_id)
    .fetch_one(pool)
    .await
    .expect("party read");
    (
        row.get("quartermaster_character_id"),
        row.get("quartermaster_version"),
    )
}

async fn designate_quartermaster(pool: &sqlx::PgPool, party_id: i64, character_id: i64) {
    sqlx::query("UPDATE parties SET quartermaster_character_id = $1 WHERE id = $2")
        .bind(character_id)
        .bind(party_id)
        .execute(pool)
        .await
        .expect("designate quartermaster");
}

/// One `client_ops` row as (`outcome`, `resulting_version`).
async fn ledger_row(pool: &sqlx::PgPool, op_id: &str) -> (String, Option<i64>) {
    let row = sqlx::query("SELECT outcome, resulting_version FROM client_ops WHERE op_id = $1")
        .bind(op_id)
        .fetch_one(pool)
        .await
        .expect("ledger row present");
    let outcome: String = row.get("outcome");
    (outcome, row.get("resulting_version"))
}

fn op(op_id: &str, target: FieldTarget, base_version: i64, value: JsonValue) -> ClientOp {
    ClientOp {
        op_id: op_id.to_owned(),
        target,
        base_version,
        value,
    }
}

fn stash_new(party_id: i64, name: &str, quantity: i64) -> FieldTargetWithValue {
    FieldTargetWithValue {
        target: FieldTarget::StashNew { party_id },
        value: json!({"item_name": name, "quantity": quantity}),
    }
}

fn stash_qty(party_id: i64, name: &str, qty_delta: i64) -> FieldTargetWithValue {
    FieldTargetWithValue {
        target: FieldTarget::Stash {
            party_id,
            item_name: name.to_owned(),
        },
        value: json!({"op": "qty", "qty_delta": qty_delta}),
    }
}

fn stash_meta(
    party_id: i64,
    name: &str,
    bulk: Option<f64>,
    notes: Option<&str>,
) -> FieldTargetWithValue {
    let mut value = serde_json::Map::new();
    value.insert("op".to_owned(), json!("meta"));
    if let Some(bulk) = bulk {
        value.insert("bulk".to_owned(), json!(bulk));
    }
    if let Some(notes) = notes {
        value.insert("notes".to_owned(), json!(notes));
    }
    FieldTargetWithValue {
        target: FieldTarget::Stash {
            party_id,
            item_name: name.to_owned(),
        },
        value: JsonValue::Object(value),
    }
}

fn bank_delta(party_id: i64, gp: i64, sp: i64, cp: i64) -> FieldTargetWithValue {
    FieldTargetWithValue {
        target: FieldTarget::Bank { party_id },
        value: json!({"gp_delta": gp, "sp_delta": sp, "cp_delta": cp}),
    }
}

fn quartermaster_flip(party_id: i64, character_id: Option<i64>) -> FieldTargetWithValue {
    FieldTargetWithValue {
        target: FieldTarget::Party {
            party_id,
            field: crate::sync::protocol::PartyField::Quartermaster,
        },
        value: json!({"character_id": character_id}),
    }
}

struct FieldTargetWithValue {
    target: FieldTarget,
    value: JsonValue,
}

/// The bank's first movement mints the balance row (the shipped table starts
/// empty); `base_version` 0 is the additive class's "no base" value — ignored
/// by the apply, uniform on the wire.
async fn first_bank_credit(pool: &sqlx::PgPool, party_id: i64, character_sub: &str) {
    let result = apply_write(
        pool,
        &player(character_sub),
        party_id,
        op(
            "bank-mint",
            bank_delta(party_id, 10, 5, 1).target,
            0,
            bank_delta(party_id, 10, 5, 1).value,
        ),
    )
    .await
    .expect("first bank credit applies");
    assert_eq!(result.outcome, Outcome::Applied);
}

// --- The additive class: stale-base writes apply, never supersede ----------

#[tokio::test]
async fn two_stash_qty_deltas_at_one_stale_base_both_apply_and_sum() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-stash-a").await;
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Rope', 2)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed stash row");
    let seeded = stash_row(&pool, party_id, "Rope")
        .await
        .expect("seeded row");

    // Both frames carry the SAME base_version — stale for the second by
    // construction, since the first applied. Additive applies ignore it.
    let first = apply_write(
        &pool,
        &player("sub-stash-a"),
        party_id,
        op(
            "qty-1",
            stash_qty(party_id, "Rope", 0).target,
            seeded.3,
            stash_qty(party_id, "Rope", 2).value,
        ),
    )
    .await
    .expect("first qty applies");
    let second = apply_write(
        &pool,
        &player("sub-stash-a"),
        party_id,
        op(
            "qty-2",
            stash_qty(party_id, "Rope", 0).target,
            seeded.3,
            stash_qty(party_id, "Rope", 3).value,
        ),
    )
    .await
    .expect("second qty applies");

    // The outcome enum, directly: additive writes are never superseded.
    assert_eq!(first.outcome, Outcome::Applied);
    assert_eq!(second.outcome, Outcome::Applied);
    assert_ne!(first.outcome, Outcome::Superseded);
    assert_ne!(second.outcome, Outcome::Superseded);
    let row = stash_row(&pool, party_id, "Rope")
        .await
        .expect("row stands");
    assert_eq!(row.0, 2 + 2 + 3, "the deltas sum: 2 seeded + 2 + 3");
    assert!(
        second.version.unwrap_or_default() > first.version.unwrap_or_default(),
        "versions strictly increase"
    );
    testing::drop_test_db(pool, "stash_qty_additive").await;
}

#[tokio::test]
async fn two_bank_deltas_at_one_stale_base_both_credit_and_sum() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "sub-bank-a").await;
    designate_quartermaster(&pool, party_id, character_id).await;
    first_bank_credit(&pool, party_id, "sub-bank-a").await;
    let seeded = bank_row(&pool, party_id).await;

    let first = apply_write(
        &pool,
        &player("sub-bank-a"),
        party_id,
        op(
            "bank-1",
            bank_delta(party_id, 0, 0, 0).target,
            seeded.3,
            bank_delta(party_id, 25, 0, 0).value,
        ),
    )
    .await
    .expect("first credit applies");
    let second = apply_write(
        &pool,
        &player("sub-bank-a"),
        party_id,
        op(
            "bank-2",
            bank_delta(party_id, 0, 0, 0).target,
            seeded.3,
            bank_delta(party_id, 0, 7, 0).value,
        ),
    )
    .await
    .expect("second credit applies");

    assert_eq!(first.outcome, Outcome::Applied);
    assert_eq!(second.outcome, Outcome::Applied);
    assert_ne!(
        second.outcome,
        Outcome::Superseded,
        "additive money is never superseded"
    );
    let (gp, sp, _cp, _version) = bank_row(&pool, party_id).await;
    assert_eq!(gp, 10 + 25, "gp deltas sum");
    assert_eq!(sp, 5 + 7, "sp deltas sum");
    testing::drop_test_db(pool, "stash_bank_additive").await;
}

// --- The CAS pair: meta and party supersede on a stale base ----------------

#[tokio::test]
async fn a_stale_meta_edit_supersedes_and_a_current_one_applies() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-meta-a").await;
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Rope', 2)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed stash row");
    let seeded = stash_row(&pool, party_id, "Rope")
        .await
        .expect("seeded row");

    // Stale base: the field moved (the seed's version is already behind after
    // any other draw — use base - 1 as the stale side).
    let stale = apply_write(
        &pool,
        &player("sub-meta-a"),
        party_id,
        op(
            "meta-stale",
            stash_meta(party_id, "Rope", None, Some("climbing")).target,
            seeded.3 - 1,
            stash_meta(party_id, "Rope", None, Some("climbing")).value,
        ),
    )
    .await
    .expect("meta runs");
    assert_eq!(
        stale.outcome,
        Outcome::Superseded,
        "meta is CAS: a stale base loses"
    );
    assert_eq!(
        stale.winning_version,
        Some(seeded.3),
        "the ack names the winning version"
    );
    assert_eq!(
        stash_row(&pool, party_id, "Rope").await.expect("row").2,
        "",
        "a superseded meta touched nothing"
    );

    // Current base: the same edit applies.
    let current = apply_write(
        &pool,
        &player("sub-meta-a"),
        party_id,
        op(
            "meta-now",
            stash_meta(party_id, "Rope", Some(0.5), Some("climbing")).target,
            seeded.3,
            stash_meta(party_id, "Rope", Some(0.5), Some("climbing")).value,
        ),
    )
    .await
    .expect("current meta applies");
    assert_eq!(current.outcome, Outcome::Applied);
    let row = stash_row(&pool, party_id, "Rope").await.expect("row");
    assert_eq!(row.1, Some(0.5), "bulk set");
    assert_eq!(row.2, "climbing", "notes set");
    testing::drop_test_db(pool, "stash_meta_cas").await;
}

#[tokio::test]
async fn a_stale_quartermaster_flip_supersedes_and_a_current_one_applies() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "sub-qm-a").await;
    let (_party_id_b, character_b) = seed_member(&pool, "sub-qm-b").await;
    let (_qm_version, base) = quartermaster(&pool, party_id).await;

    let stale = apply_write(
        &pool,
        &player("sub-qm-a"),
        party_id,
        op(
            "qm-stale",
            quartermaster_flip(party_id, Some(character_id)).target,
            base - 1,
            quartermaster_flip(party_id, Some(character_id)).value,
        ),
    )
    .await
    .expect("flip runs");
    assert_eq!(
        stale.outcome,
        Outcome::Superseded,
        "party is CAS: a stale base loses"
    );
    assert_eq!(
        quartermaster(&pool, party_id).await.0,
        None,
        "a superseded flip touched nothing"
    );

    let current = apply_write(
        &pool,
        &player("sub-qm-a"),
        party_id,
        op(
            "qm-now",
            quartermaster_flip(party_id, Some(character_b)).target,
            base,
            quartermaster_flip(party_id, Some(character_b)).value,
        ),
    )
    .await
    .expect("current flip applies");
    assert_eq!(current.outcome, Outcome::Applied);
    assert_eq!(
        quartermaster(&pool, party_id).await.0,
        Some(character_b),
        "the designation moved"
    );
    testing::drop_test_db(pool, "stash_party_cas").await;
}

// --- Underflow: refuse whole, nothing partial, no claim row ----------------

#[tokio::test]
async fn a_qty_overdraw_refuses_whole_with_no_claim_row() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-under-a").await;
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Rope', 2)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed stash row");
    let seeded = stash_row(&pool, party_id, "Rope")
        .await
        .expect("seeded row");

    let result = apply_write(
        &pool,
        &player("sub-under-a"),
        party_id,
        op(
            "qty-over",
            stash_qty(party_id, "Rope", 0).target,
            seeded.3,
            stash_qty(party_id, "Rope", -5).value,
        ),
    )
    .await
    .expect("overdraw runs");

    assert_eq!(result.outcome, Outcome::Rejected);
    let reason = result.reason.expect("a refusal carries its reason");
    assert!(reason.contains("below zero"), "human-readable: {reason}");
    assert_eq!(
        stash_row(&pool, party_id, "Rope").await.expect("row").0,
        2,
        "quantity unchanged"
    );
    assert_eq!(
        claim_count(&pool, party_id, "Rope").await,
        0,
        "no claim row"
    );
    let (outcome, _) = ledger_row(&pool, "qty-over").await;
    assert_eq!(outcome, "rejected", "the refusal is durably recorded");
    testing::drop_test_db(pool, "stash_qty_underflow").await;
}

#[tokio::test]
async fn a_qty_on_an_unknown_row_is_rejected() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-under-b").await;

    let result = apply_write(
        &pool,
        &player("sub-under-b"),
        party_id,
        op(
            "qty-missing",
            stash_qty(party_id, "Ghost item", 0).target,
            0,
            stash_qty(party_id, "Ghost item", 1).value,
        ),
    )
    .await
    .expect("unknown-row qty runs");
    assert_eq!(result.outcome, Outcome::Rejected);
    assert_eq!(
        result.reason.as_deref(),
        Some("unknown target"),
        "contract §4's reason, verbatim"
    );
    testing::drop_test_db(pool, "stash_qty_unknown_row").await;
}

#[tokio::test]
async fn a_qty_draw_to_exactly_zero_deletes_the_row_and_fans_a_removal_diff() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-zero-a").await;
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Rope', 3)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed stash row");
    let seeded = stash_row(&pool, party_id, "Rope")
        .await
        .expect("seeded row");

    // The boundary pair (house rule): -(quantity) applies-and-removes,
    // -(quantity + 1) refuses (FR-1: a zero-quantity row is deleted, not
    // retained).
    let removal = apply_write(
        &pool,
        &player("sub-zero-a"),
        party_id,
        op(
            "qty-zero",
            stash_qty(party_id, "Rope", 0).target,
            seeded.3,
            stash_qty(party_id, "Rope", -3).value,
        ),
    )
    .await
    .expect("draw-to-zero runs");

    assert_eq!(removal.outcome, Outcome::Applied);
    assert_eq!(
        stash_row_count(&pool, party_id, "Rope").await,
        0,
        "the row is deleted, not retained at zero"
    );
    // The removal fans a tombstone diff: value null on the stash target,
    // a fresh version so every seat drops the row under the strictly-newer
    // rule.
    let (target, value, version) = removal
        .broadcast
        .first()
        .expect("the removal fans one diff");
    assert!(
        matches!(target, FieldTarget::Stash { item_name, .. } if item_name.as_str() == "Rope"),
        "the diff addresses the removed row: {target:?}"
    );
    assert!(
        value.is_null(),
        "the removal diff value is null (tombstone): {value:?}"
    );
    assert!(
        *version > seeded.3,
        "the tombstone version is strictly newer than the row's"
    );

    // The other side of the boundary: -(quantity + 1) still refuses whole.
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Chain', 2)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed second row");
    let below = apply_write(
        &pool,
        &player("sub-zero-a"),
        party_id,
        op(
            "qty-below",
            stash_qty(party_id, "Chain", 0).target,
            0,
            stash_qty(party_id, "Chain", -3).value,
        ),
    )
    .await
    .expect("draw-below-zero runs");
    assert_eq!(below.outcome, Outcome::Rejected, "-(quantity + 1) refuses");
    let reason = below.reason.expect("reason");
    assert!(reason.contains("below zero"), "names the fault: {reason}");
    assert_eq!(
        stash_row_count(&pool, party_id, "Chain").await,
        1,
        "the row is unchanged"
    );

    testing::drop_test_db(pool, "stash_qty_delete_at_zero").await;
}

#[tokio::test]
async fn a_qty_delta_at_the_int4_ceiling_is_rejected_not_an_error() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-overflow-a").await;
    sqlx::query("INSERT INTO party_stash (party_id, item_name, quantity) VALUES ($1, 'Rope', 1)")
        .bind(party_id)
        .execute(&pool)
        .await
        .expect("seed stash row");

    let result = apply_write(
        &pool,
        &player("sub-overflow-a"),
        party_id,
        op(
            "qty-overflow",
            stash_qty(party_id, "Rope", 0).target,
            0,
            stash_qty(party_id, "Rope", i64::from(i32::MAX)).value,
        ),
    )
    .await
    .expect("ceiling delta runs");

    assert_eq!(
        result.outcome,
        Outcome::Rejected,
        "an i32::MAX delta refuses, not a driver error"
    );
    let reason = result.reason.expect("reason");
    assert!(reason.contains("overflow"), "names the fault: {reason}");
    assert_eq!(
        stash_row(&pool, party_id, "Rope").await.expect("row").0,
        1,
        "quantity unchanged"
    );
    testing::drop_test_db(pool, "stash_qty_overflow").await;
}

#[tokio::test]
async fn a_bank_delta_driving_a_denomination_negative_refuses_whole() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "sub-under-c").await;
    designate_quartermaster(&pool, party_id, character_id).await;
    first_bank_credit(&pool, party_id, "sub-under-c").await;
    let seeded = bank_row(&pool, party_id).await;

    let result = apply_write(
        &pool,
        &player("sub-under-c"),
        party_id,
        op(
            "bank-over",
            bank_delta(party_id, 0, 0, 0).target,
            seeded.3,
            bank_delta(party_id, -50, 0, 0).value,
        ),
    )
    .await
    .expect("overdraw runs");
    assert_eq!(result.outcome, Outcome::Rejected);
    let reason = result.reason.expect("a refusal carries its reason");
    assert!(
        reason.contains("gp"),
        "the reason names the denomination: {reason}"
    );
    assert!(reason.contains("below zero"), "human-readable: {reason}");
    assert_eq!(bank_row(&pool, party_id).await.0, 10, "balance unchanged");
    assert_eq!(
        claim_count(&pool, party_id, "bank adjustment").await,
        0,
        "no claim row"
    );
    testing::drop_test_db(pool, "stash_bank_underflow").await;
}

#[tokio::test]
async fn a_bank_delta_at_the_int4_ceiling_is_rejected_not_an_error() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "sub-overflow-b").await;
    designate_quartermaster(&pool, party_id, character_id).await;
    first_bank_credit(&pool, party_id, "sub-overflow-b").await;
    let seeded = bank_row(&pool, party_id).await;

    let result = apply_write(
        &pool,
        &player("sub-overflow-b"),
        party_id,
        op(
            "bank-overflow",
            bank_delta(party_id, 0, 0, 0).target,
            seeded.3,
            bank_delta(party_id, i64::from(i32::MAX), 0, 0).value,
        ),
    )
    .await
    .expect("ceiling delta runs");

    assert_eq!(
        result.outcome,
        Outcome::Rejected,
        "an i32::MAX delta refuses, not a driver error"
    );
    let reason = result.reason.expect("reason");
    assert!(reason.contains("overflow"), "names the fault: {reason}");
    assert_eq!(bank_row(&pool, party_id).await.0, 10, "balance unchanged");
    testing::drop_test_db(pool, "stash_bank_overflow").await;
}

// --- stash_new: the mint, its refusal, its boundaries ----------------------

#[tokio::test]
async fn a_stash_new_mints_the_row_and_the_outside_to_stash_claim() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-mint-a").await;

    let result = apply_write(
        &pool,
        &player("sub-mint-a"),
        party_id,
        op(
            "mint-1",
            stash_new(party_id, "Rope", 0).target,
            0,
            stash_new(party_id, "Rope", 4).value,
        ),
    )
    .await
    .expect("mint applies");
    assert_eq!(result.outcome, Outcome::Applied);
    let row = stash_row(&pool, party_id, "Rope")
        .await
        .expect("row minted");
    assert_eq!(row.0, 4);
    assert_eq!(
        result.version,
        Some(row.3),
        "the ack carries the minted row's version"
    );
    // The claim row: outside → stash, the minting member's identity.
    let claim = sqlx::query(
        "SELECT origin, destination, actor_sub, quantity FROM claim_history \
         WHERE party_id = $1 AND item_name = 'Rope'",
    )
    .bind(party_id)
    .fetch_one(&pool)
    .await
    .expect("claim row");
    let origin: String = claim.get("origin");
    let destination: String = claim.get("destination");
    let actor_sub: String = claim.get("actor_sub");
    let quantity: i32 = claim.get("quantity");
    assert_eq!(
        (origin.as_str(), destination.as_str()),
        ("outside", "stash")
    );
    assert_eq!(actor_sub, "sub-mint-a");
    assert_eq!(quantity, 4);
    testing::drop_test_db(pool, "stash_new_mints").await;
}

#[tokio::test]
async fn a_stash_new_on_an_existing_exact_name_refuses_with_the_fr1_reason() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-mint-b").await;
    let first = apply_write(
        &pool,
        &player("sub-mint-b"),
        party_id,
        op(
            "mint-2",
            stash_new(party_id, "Rope", 0).target,
            0,
            stash_new(party_id, "Rope", 2).value,
        ),
    )
    .await
    .expect("first mint applies");

    // Byte-equality identity: the exact name again is refused; a different
    // case is a different row (FR-1's edge case).
    let duplicate = apply_write(
        &pool,
        &player("sub-mint-b"),
        party_id,
        op(
            "mint-3",
            stash_new(party_id, "Rope", 0).target,
            0,
            stash_new(party_id, "Rope", 3).value,
        ),
    )
    .await
    .expect("duplicate mint runs");
    let other_case = apply_write(
        &pool,
        &player("sub-mint-b"),
        party_id,
        op(
            "mint-4",
            stash_new(party_id, "rope", 0).target,
            0,
            stash_new(party_id, "rope", 1).value,
        ),
    )
    .await
    .expect("case-different mint runs");

    assert_eq!(first.outcome, Outcome::Applied);
    assert_eq!(duplicate.outcome, Outcome::Rejected, "exact name refused");
    let reason = duplicate.reason.expect("the refusal carries FR-1's reason");
    assert!(
        reason.contains("already in the stash"),
        "names the conflict: {reason}"
    );
    assert!(reason.contains("adjust"), "points at the remedy: {reason}");
    assert_eq!(
        other_case.outcome,
        Outcome::Applied,
        "byte equality, case included"
    );
    assert_eq!(
        stash_row_count(&pool, party_id, "Rope").await,
        1,
        "one row, not minted twice"
    );
    assert_eq!(
        stash_row(&pool, party_id, "Rope").await.expect("row").0,
        2,
        "quantity unchanged by the refusal"
    );
    testing::drop_test_db(pool, "stash_new_duplicate").await;
}

#[tokio::test]
async fn a_stash_new_of_quantity_zero_is_refused_and_one_is_allowed() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-mint-c").await;

    let zero = apply_write(
        &pool,
        &player("sub-mint-c"),
        party_id,
        op(
            "mint-zero",
            stash_new(party_id, "Torch", 0).target,
            0,
            stash_new(party_id, "Torch", 0).value,
        ),
    )
    .await
    .expect("zero mint runs");
    let one = apply_write(
        &pool,
        &player("sub-mint-c"),
        party_id,
        op(
            "mint-one",
            stash_new(party_id, "Torch", 0).target,
            0,
            stash_new(party_id, "Torch", 1).value,
        ),
    )
    .await
    .expect("one mint applies");

    assert_eq!(
        zero.outcome,
        Outcome::Rejected,
        "the CHECK's lower side: quantity > 0"
    );
    assert_eq!(
        one.outcome,
        Outcome::Applied,
        "the CHECK's upper side: 1 is a legal mint"
    );
    testing::drop_test_db(pool, "stash_new_boundary").await;
}

// --- SC-1's seed: a committed mint fans to the other seat live -------------

#[tokio::test]
async fn a_stash_new_fans_the_row_and_the_claim_to_a_second_seat() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "dev-sub-josh").await;
    let (_p, _c2) = seed_member(&pool, "dev-sub-becky").await;
    let session_a = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let session_b = testing::seed_session(&pool, "dev-sub-becky", chrono::Utc::now()).await;
    let (addr, shutdown) = test_helpers::spawn_server(pool.clone()).await;

    let mut a = test_helpers::connect(addr, party_id, &session_a).await;
    let mut b = test_helpers::connect(addr, party_id, &session_b).await;
    for (client, what) in [(&mut a, "a"), (&mut b, "b")] {
        let hello = test_helpers::read_frame(&mut client.stream, &format!("{what} hello")).await;
        assert!(
            matches!(hello, ServerFrame::Hello { .. }),
            "{what}: {hello:?}"
        );
        let snapshot =
            test_helpers::read_frame(&mut client.stream, &format!("{what} snapshot")).await;
        assert!(
            matches!(snapshot, ServerFrame::Snapshot { .. }),
            "{what}: {snapshot:?}"
        );
    }

    let frame = json!({
        "t": "write",
        "op_id": "fan-1",
        "target": {"kind": "stash_new", "party_id": party_id},
        "base_version": 0,
        "value": {"item_name": "Rope", "quantity": 4}
    });
    test_helpers::send_raw(&mut a.sink, &frame.to_string()).await;

    // The watcher seat sees the resolved row first, then the claim append —
    // D7's two riders of one committed mint, TCP-ordered.
    let row_diff = test_helpers::read_frame(&mut b.stream, "stash row diff").await;
    let ServerFrame::Diff { field, value, .. } = row_diff else {
        panic!("expected the stash row diff");
    };
    assert!(
        matches!(field, FieldTarget::Stash { ref item_name, .. } if item_name == "Rope"),
        "the row diff addresses the minted row: {field:?}"
    );
    assert_eq!(
        value.get("quantity"),
        Some(&json!(4)),
        "the resolved row shape"
    );
    let claim_diff = test_helpers::read_frame(&mut b.stream, "claim diff").await;
    let ServerFrame::Diff {
        field: claim_field,
        value: claim_value,
        ..
    } = claim_diff
    else {
        panic!("expected the claim diff");
    };
    assert!(
        matches!(claim_field, FieldTarget::Claim { .. }),
        "the append rides the claim target"
    );
    assert_eq!(claim_value.get("origin"), Some(&json!("outside")));
    assert_eq!(claim_value.get("destination"), Some(&json!("stash")));
    assert_eq!(claim_value.get("actor_sub"), Some(&json!("dev-sub-josh")));

    // The writer's own socket: its ack first, then the same two diffs.
    // The contract permits either ack/diff order on the writer's socket;
    // in practice the direct ack send wins the race with the registry's
    // pump (pinned by `an_applied_write_acks_the_writer_and_diffs_every_socket`).
    let ack = test_helpers::read_frame(&mut a.stream, "ack").await;
    assert!(
        matches!(&ack, ServerFrame::Ack(ack) if ack.op_id == "fan-1" && ack.outcome == Outcome::Applied),
        "the mint's ack: {ack:?}"
    );
    let first = test_helpers::read_frame(&mut a.stream, "writer row diff").await;
    assert!(matches!(first, ServerFrame::Diff { .. }), "{first:?}");
    let second = test_helpers::read_frame(&mut a.stream, "writer claim diff").await;
    assert!(matches!(second, ServerFrame::Diff { .. }), "{second:?}");

    shutdown.send(()).ok();
    testing::drop_test_db(pool, "stash_fanout").await;
}

#[tokio::test]
async fn a_non_member_stash_write_is_forbidden_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-auth-a").await;
    testing::seed_account(&pool, "sub-outsider", "player").await;

    let result = apply_write(
        &pool,
        &player("sub-outsider"),
        party_id,
        op(
            "stash-outsider",
            stash_new(party_id, "Rope", 0).target,
            0,
            stash_new(party_id, "Rope", 1).value,
        ),
    )
    .await
    .expect("denial runs");
    assert_eq!(result.outcome, Outcome::Forbidden);
    assert_eq!(
        stash_row_count(&pool, party_id, "Rope").await,
        0,
        "nothing touched"
    );
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, _target, outcome)| {
            event == "forbidden_stash_write"
                && actor.as_deref() == Some("sub-outsider")
                && outcome == "denied"
        }),
        "the refusal is audited under the new value: {audits:?}"
    );
    testing::drop_test_db(pool, "stash_authz_nonmember").await;
}

#[tokio::test]
async fn a_non_member_party_write_is_forbidden_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-auth-party").await;
    testing::seed_account(&pool, "sub-outsider-party", "player").await;

    let result = apply_write(
        &pool,
        &player("sub-outsider-party"),
        party_id,
        op(
            "party-outsider",
            quartermaster_flip(party_id, None).target,
            0,
            quartermaster_flip(party_id, None).value,
        ),
    )
    .await
    .expect("denial runs");
    assert_eq!(result.outcome, Outcome::Forbidden);
    assert_eq!(
        quartermaster(&pool, party_id).await.0,
        None,
        "nothing touched"
    );
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, _target, outcome)| {
            event == "forbidden_party_write"
                && actor.as_deref() == Some("sub-outsider-party")
                && outcome == "denied"
        }),
        "the refusal is audited under the new value: {audits:?}"
    );
    testing::drop_test_db(pool, "stash_authz_party_nonmember").await;
}

#[tokio::test]
async fn the_gm_is_refused_and_audited_on_every_party_target() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "sub-auth-b").await;
    let gm = Actor {
        sub: "sub-auth-b".to_owned(),
        role: Role::Gm,
    };

    let mint = apply_write(
        &pool,
        &gm,
        party_id,
        op(
            "gm-mint",
            stash_new(party_id, "Rope", 0).target,
            0,
            stash_new(party_id, "Rope", 1).value,
        ),
    )
    .await
    .expect("gm mint runs");
    let qty = apply_write(
        &pool,
        &gm,
        party_id,
        op(
            "gm-qty",
            stash_qty(party_id, "Rope", 0).target,
            0,
            stash_qty(party_id, "Rope", 1).value,
        ),
    )
    .await
    .expect("gm qty runs");
    let bank = apply_write(
        &pool,
        &gm,
        party_id,
        op(
            "gm-bank",
            bank_delta(party_id, 1, 0, 0).target,
            0,
            bank_delta(party_id, 1, 0, 0).value,
        ),
    )
    .await
    .expect("gm bank runs");
    let flip = apply_write(
        &pool,
        &gm,
        party_id,
        op(
            "gm-flip",
            quartermaster_flip(party_id, Some(character_id)).target,
            0,
            quartermaster_flip(party_id, Some(character_id)).value,
        ),
    )
    .await
    .expect("gm flip runs");

    for (name, result) in [
        ("mint", &mint),
        ("qty", &qty),
        ("bank", &bank),
        ("flip", &flip),
    ] {
        assert_eq!(result.outcome, Outcome::Forbidden, "gm {name} denied");
        assert_eq!(
            result.reason.as_deref(),
            Some("gm is read-only"),
            "gm {name} reason"
        );
    }
    // D4: the GM is denied first by the role check, and the denial audits
    // under the existing value (Task 3's acceptance: gm write →
    // forbidden_gm_write, audited).
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits
            .iter()
            .filter(|(e, _actor, _t, outcome)| e == "forbidden_gm_write" && outcome == "denied")
            .count()
            >= 4,
        "each gm denial audited: {audits:?}"
    );
    assert_eq!(
        quartermaster(&pool, party_id).await.0,
        None,
        "nothing touched"
    );
    testing::drop_test_db(pool, "stash_authz_gm").await;
}

#[tokio::test]
async fn an_undesignated_party_refuses_bank_writes_and_audits() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-auth-c").await;

    let result = apply_write(
        &pool,
        &player("sub-auth-c"),
        party_id,
        op(
            "bank-undesignated",
            bank_delta(party_id, 1, 0, 0).target,
            0,
            bank_delta(party_id, 1, 0, 0).value,
        ),
    )
    .await
    .expect("undesignated bank runs");
    assert_eq!(
        result.outcome,
        Outcome::Forbidden,
        "no quartermaster, no bank writes"
    );
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(
            |(event, _actor, _t, outcome)| event == "forbidden_bank_write" && outcome == "denied"
        ),
        "audited: {audits:?}"
    );
    testing::drop_test_db(pool, "stash_authz_undesignated").await;
}

#[tokio::test]
async fn a_non_quartermaster_member_is_refused_at_the_bank_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_a) = seed_member(&pool, "sub-auth-d").await;
    let (_p, _character_b) = seed_member(&pool, "sub-auth-e").await;
    designate_quartermaster(&pool, party_id, character_a).await;

    let result = apply_write(
        &pool,
        &player("sub-auth-e"),
        party_id,
        op(
            "bank-not-qm",
            bank_delta(party_id, 1, 0, 0).target,
            0,
            bank_delta(party_id, 1, 0, 0).value,
        ),
    )
    .await
    .expect("non-QM bank runs");
    assert_eq!(
        result.outcome,
        Outcome::Forbidden,
        "members without the designation cannot move coin"
    );
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, _t, outcome)| {
            event == "forbidden_bank_write"
                && actor.as_deref() == Some("sub-auth-e")
                && outcome == "denied"
        }),
        "audited with the actor's identity: {audits:?}"
    );
    testing::drop_test_db(pool, "stash_authz_bank_nonqm").await;
}

#[tokio::test]
async fn any_member_may_flip_the_quartermaster_and_clear_it() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_a) = seed_member(&pool, "sub-flip-a").await;
    let (_p, character_b) = seed_member(&pool, "sub-flip-b").await;

    // FR-5's flip rule: any character owner, designated or not, sets and
    // clears. B (never designated) flips to B, then A clears. Each write
    // rides the CURRENT quartermaster_version — it is CAS.
    let base = quartermaster(&pool, party_id).await.1;
    let first = apply_write(
        &pool,
        &player("sub-flip-b"),
        party_id,
        op(
            "flip-1",
            quartermaster_flip(party_id, Some(character_b)).target,
            base,
            quartermaster_flip(party_id, Some(character_b)).value,
        ),
    )
    .await
    .expect("b flips");
    let base_after_b = first.version.expect("applied flips carry the new version");
    let second = apply_write(
        &pool,
        &player("sub-flip-a"),
        party_id,
        op(
            "flip-2",
            quartermaster_flip(party_id, Some(character_a)).target,
            base_after_b,
            quartermaster_flip(party_id, Some(character_a)).value,
        ),
    )
    .await
    .expect("a flips");
    let base_after_a = second.version.expect("applied flips carry the new version");
    let clear = apply_write(
        &pool,
        &player("sub-flip-a"),
        party_id,
        op(
            "flip-3",
            quartermaster_flip(party_id, None).target,
            base_after_a,
            quartermaster_flip(party_id, None).value,
        ),
    )
    .await
    .expect("a clears");

    assert_eq!(first.outcome, Outcome::Applied);
    assert_eq!(second.outcome, Outcome::Applied);
    assert_eq!(clear.outcome, Outcome::Applied);
    assert_eq!(quartermaster(&pool, party_id).await.0, None, "cleared");
    testing::drop_test_db(pool, "stash_flip_rule").await;
}

#[tokio::test]
async fn a_quartermaster_write_to_a_non_roster_character_is_rejected() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-flip-c").await;
    // A character of ANOTHER party: the roster the designation may name.
    let other_party: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('flip foreign') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");
    testing::seed_account(&pool, "sub-flip-d", "player").await;
    let other_character: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, 'sub-flip-d', '{}', '{}') RETURNING id",
    )
    .bind(other_party)
    .fetch_one(&pool)
    .await
    .expect("foreign character");

    let result = apply_write(
        &pool,
        &player("sub-flip-c"),
        party_id,
        op(
            "flip-foreign",
            quartermaster_flip(party_id, Some(other_character)).target,
            0,
            quartermaster_flip(party_id, Some(other_character)).value,
        ),
    )
    .await
    .expect("foreign flip runs");
    assert_eq!(result.outcome, Outcome::Rejected);
    let reason = result.reason.expect("reason");
    assert!(reason.contains("roster"), "names the fault: {reason}");
    assert_eq!(
        quartermaster(&pool, party_id).await.0,
        None,
        "nothing touched"
    );
    testing::drop_test_db(pool, "stash_flip_foreign").await;
}

#[tokio::test]
async fn a_party_write_addressing_another_party_is_forbidden() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _character_id) = seed_member(&pool, "sub-scope-a").await;
    let other_party: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('other party') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");

    let result = apply_write(
        &pool,
        &player("sub-scope-a"),
        party_id,
        op(
            "scope-stash",
            stash_qty(other_party, "Rope", 0).target,
            0,
            stash_qty(other_party, "Rope", 1).value,
        ),
    )
    .await
    .expect("cross-party runs");
    assert_eq!(
        result.outcome,
        Outcome::Forbidden,
        "the socket's party scopes every write"
    );
    let mint_result = apply_write(
        &pool,
        &player("sub-scope-a"),
        party_id,
        op(
            "scope-mint",
            stash_new(other_party, "Rope", 0).target,
            0,
            stash_new(other_party, "Rope", 1).value,
        ),
    )
    .await
    .expect("cross-party mint runs");
    assert_eq!(mint_result.outcome, Outcome::Forbidden);
    testing::drop_test_db(pool, "stash_party_scope").await;
}
