//! Integration tests for E8's effect writes (plan Task 7): real router,
//! real listener, real sockets — the PR #30 house rule. Create→applied→diff,
//! retarget CAS supersession, op replay, creator-only authz, loud
//! rejections, and corpus-condition resolution through `engine_host::apply`.

use serde_json::{Value, json};
use sqlx::Row as _;

use crate::auth::authz::{Actor, Role};
use crate::sync::protocol::{FieldTarget, Outcome, ServerFrame};
use crate::sync::test_helpers::{connect, read_frame, seed_member, send_raw, spawn_server};
use crate::sync::write::{ClientOp, apply_write};
use crate::testing;

/// Seed a second member (the first `seed_member` call makes the party row
/// via the POC seed; both members join the same POC party).
async fn seed_second_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party");
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', '{}') RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .fetch_one(pool)
    .await
    .expect("seed member");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(character_id)
        .execute(pool)
        .await
        .expect("seed vitals");
    (party_id, character_id)
}

fn player(sub: &str) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role: Role::Player,
    }
}

fn gm(sub: &str) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role: Role::Gm,
    }
}

/// A create op on the POC party: hand-built Bless, one modifier.
fn create_op(op_id: &str, source: i64, targets: &[i64]) -> ClientOp {
    ClientOp {
        op_id: op_id.to_owned(),
        target: FieldTarget::EffectNew { party_id: 0 },
        base_version: 0,
        value: json!({
            "op": "create",
            "name": "Bless",
            "source_character_id": source,
            "targets": targets,
            "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
            "duration_note": "10 rounds",
            "corpus_entry_id": null,
            "condition_value": null,
        }),
    }
}

/// Point the op at the party's real id (test helper — the party exists by
/// the time ops are built).
fn on_party(mut op: ClientOp, party_id: i64) -> ClientOp {
    op.target = FieldTarget::EffectNew { party_id };
    op
}

fn retarget_op(op_id: &str, effect_id: i64, base_version: i64, targets: &[i64]) -> ClientOp {
    ClientOp {
        op_id: op_id.to_owned(),
        target: FieldTarget::Effect { effect_id },
        base_version,
        value: json!({"op": "update", "targets": targets}),
    }
}

async fn effect_row(pool: &sqlx::PgPool, effect_id: i64) -> (bool, bool, Option<i64>) {
    let row =
        sqlx::query("SELECT active, tracked_manually, corpus_entry_id FROM effects WHERE id = $1")
            .bind(effect_id)
            .fetch_one(pool)
            .await
            .expect("effect row");
    (
        row.get("active"),
        row.get("tracked_manually"),
        row.get("corpus_entry_id"),
    )
}

async fn stored_modifiers(pool: &sqlx::PgPool, effect_id: i64) -> Vec<(String, String, i32)> {
    sqlx::query_as(
        "SELECT type, stat, value FROM effect_modifiers WHERE effect_id = $1 ORDER BY ord",
    )
    .bind(effect_id)
    .fetch_all(pool)
    .await
    .expect("modifiers")
}

async fn effect_version(pool: &sqlx::PgPool, effect_id: i64) -> i64 {
    sqlx::query_scalar("SELECT version FROM effects WHERE id = $1")
        .bind(effect_id)
        .fetch_one(pool)
        .await
        .expect("effect version")
}

/// One corpus condition row; `engine_math` carries the frightened mapping,
/// `display_only` none. Returns the corpus row id.
async fn seed_corpus_condition(pool: &sqlx::PgPool, tier: &str) -> i64 {
    let (data, modifiers) = if tier == "engine_math" {
        (
            json!({"import": {"tier": "engine_math"}}),
            json!([{"type": "status", "stat": "all_checks_and_dcs", "value": Value::Null,
                    "value_kind": "condition_value", "polarity": "negative"}]),
        )
    } else {
        (json!({"import": {"tier": "display_only"}}), Value::Null)
    };
    sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, modifiers) \
         VALUES ('condition', 'Frightened', 'core', $1, $2) RETURNING id",
    )
    .bind(data)
    .bind(modifiers)
    .fetch_one(pool)
    .await
    .expect("corpus row")
}

// --- write-engine level (E7's write.rs pattern) ---------------------------

#[tokio::test]
async fn a_create_applies_and_the_row_is_whole() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let (_, _beta) = seed_second_member(&pool, "sub-beta").await;

    let result = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        on_party(create_op("op-create-1", alpha, &[alpha]), party_id),
    )
    .await
    .expect("create applies");

    assert_eq!(result.outcome, Outcome::Applied);
    let effect_id = match &result.broadcast {
        Some((FieldTarget::Effect { effect_id }, value)) => {
            assert_eq!(
                value
                    .get("modifiers")
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(1),
                "the diff carries the stored modifiers: {value}"
            );
            *effect_id
        }
        other => panic!("create broadcasts the new effect row, got {other:?}"),
    };
    let (active, tracked_manually, corpus) = effect_row(&pool, effect_id).await;
    assert!(active);
    assert!(!tracked_manually, "hand-built: plain row");
    assert_eq!(corpus, None);
    assert_eq!(
        stored_modifiers(&pool, effect_id).await,
        vec![("status".to_owned(), "attack".to_owned(), 1)],
        "modifiers stored in ord order"
    );
    let (outcome, version): (String, Option<i64>) = sqlx::query_as(
        "SELECT outcome, resulting_version FROM client_ops WHERE op_id = 'op-create-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("ledger row");
    assert_eq!(outcome, "applied");
    assert_eq!(version, result.version);
    testing::drop_test_db(pool, "e8_create_applies").await;
}

#[tokio::test]
async fn a_retarget_at_a_stale_base_is_superseded() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let (_, beta) = seed_second_member(&pool, "sub-beta").await;
    let created = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        on_party(create_op("op-rt-0", alpha, &[alpha]), party_id),
    )
    .await
    .expect("create");
    let Some((FieldTarget::Effect { effect_id }, _)) = created.broadcast else {
        panic!("create broadcasts");
    };
    let base = effect_version(&pool, effect_id).await;

    let winner = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        retarget_op("op-rt-1", effect_id, base, &[alpha, beta]),
    )
    .await
    .expect("winning retarget");
    assert_eq!(winner.outcome, Outcome::Applied);

    let loser = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        retarget_op("op-rt-2", effect_id, base, &[]),
    )
    .await
    .expect("stale retarget resolves");
    assert_eq!(loser.outcome, Outcome::Superseded);
    assert_eq!(
        loser.winning_version, winner.version,
        "reports the version that won"
    );
    let targets: Vec<i64> = sqlx::query_scalar(
        "SELECT character_id FROM effect_targets WHERE effect_id = $1 ORDER BY character_id",
    )
    .bind(effect_id)
    .fetch_all(&pool)
    .await
    .expect("targets");
    assert_eq!(targets, vec![alpha, beta], "the loser changed nothing");
    testing::drop_test_db(pool, "e8_retarget_superseded").await;
}

#[tokio::test]
async fn a_replayed_create_is_already_applied_with_one_row() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let first = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        on_party(create_op("op-replay-1", alpha, &[alpha]), party_id),
    )
    .await
    .expect("first create");
    assert_eq!(first.outcome, Outcome::Applied);

    let replay = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        on_party(create_op("op-replay-1", alpha, &[alpha]), party_id),
    )
    .await
    .expect("replay");
    assert_eq!(replay.outcome, Outcome::AlreadyApplied);
    assert_eq!(replay.version, first.version, "the original's version");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM effects WHERE party_id = $1 AND name = 'Bless'")
            .bind(party_id)
            .fetch_one(&pool)
            .await
            .expect("effect count");
    assert_eq!(rows, 1, "exactly-once: the replay committed nothing");
    testing::drop_test_db(pool, "e8_replay").await;
}

#[tokio::test]
async fn only_the_creator_mutates_and_the_gm_is_denied_everywhere() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let _ = seed_second_member(&pool, "sub-other").await;
    testing::seed_account(&pool, "sub-gm", "gm").await;
    let created = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        on_party(create_op("op-auth-0", alpha, &[alpha]), party_id),
    )
    .await
    .expect("create");
    let Some((FieldTarget::Effect { effect_id }, _)) = created.broadcast else {
        panic!("create broadcasts");
    };
    let version = effect_version(&pool, effect_id).await;

    // A fellow player who is NOT the creator: forbidden, row untouched.
    let stranger = apply_write(
        &pool,
        &player("sub-other"),
        party_id,
        retarget_op("op-auth-1", effect_id, version, &[]),
    )
    .await
    .expect("stranger retarget resolves");
    assert_eq!(stranger.outcome, Outcome::Forbidden);
    assert!(
        stranger
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("creator")),
        "the denial names the rule: {stranger:?}"
    );

    // The GM is denied on mutation…
    let gm_retarget = apply_write(
        &pool,
        &gm("sub-gm"),
        party_id,
        retarget_op("op-auth-2", effect_id, version, &[]),
    )
    .await
    .expect("gm retarget resolves");
    assert_eq!(gm_retarget.outcome, Outcome::Forbidden);
    assert_eq!(
        gm_retarget.reason.as_deref(),
        Some("gm is read-only"),
        "E3's GM denial, verbatim"
    );
    // …and on create.
    let gm_create = apply_write(
        &pool,
        &gm("sub-gm"),
        party_id,
        on_party(create_op("op-auth-3", alpha, &[alpha]), party_id),
    )
    .await
    .expect("gm create resolves");
    assert_eq!(gm_create.outcome, Outcome::Forbidden);

    // The creator still wins at the CURRENT version.
    let current = effect_version(&pool, effect_id).await;
    let creator = apply_write(
        &pool,
        &player("sub-alpha"),
        party_id,
        retarget_op("op-auth-4", effect_id, current, &[alpha]),
    )
    .await
    .expect("creator retarget");
    assert_eq!(creator.outcome, Outcome::Applied);
    testing::drop_test_db(pool, "e8_creator_only").await;
}

#[tokio::test]
async fn an_out_of_party_target_or_source_is_rejected() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_a, alpha) = seed_member(&pool, "sub-alpha").await;
    // A second party with its own member.
    testing::seed_account(&pool, "sub-elsewhere", "player").await;
    let party_b: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Elsewhere') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");
    let outsider: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, 'sub-elsewhere', '{}', '{}') RETURNING id",
    )
    .bind(party_b)
    .fetch_one(&pool)
    .await
    .expect("outsider");

    let result = apply_write(
        &pool,
        &player("sub-alpha"),
        party_a,
        on_party(create_op("op-scope-1", alpha, &[outsider]), party_a),
    )
    .await
    .expect("resolves");
    assert_eq!(result.outcome, Outcome::Rejected);
    assert!(
        result
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("roster")),
        "names the roster rule: {result:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM effects")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows, 0, "nothing was created");
    testing::drop_test_db(pool, "e8_party_scope").await;
}

#[tokio::test]
async fn a_corpus_condition_create_resolves_signed_modifiers() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let corpus_id = seed_corpus_condition(&pool, "engine_math").await;

    let op = on_party(
        ClientOp {
            value: json!({
                "op": "create",
                "name": "Frightened",
                "source_character_id": alpha,
                "targets": [alpha],
                "modifiers": [],
                "duration_note": "",
                "corpus_entry_id": corpus_id,
                "condition_value": 2,
            }),
            ..create_op("op-corpus-1", alpha, &[alpha])
        },
        party_id,
    );
    let result = apply_write(&pool, &player("sub-alpha"), party_id, op)
        .await
        .expect("corpus create");
    assert_eq!(
        result.outcome,
        Outcome::Applied,
        "reason: {:?}",
        result.reason
    );
    let Some((FieldTarget::Effect { effect_id }, value)) = result.broadcast else {
        panic!("corpus create broadcasts the resolved row");
    };
    assert_eq!(
        value.get("modifiers"),
        Some(&json!([{"type": "status", "stat": "all_checks_and_dcs", "value": -2}])),
        "frightened 2 resolves to −2, the corpus polarity applied (WEx-12)"
    );
    assert_eq!(
        stored_modifiers(&pool, effect_id).await,
        vec![("status".to_owned(), "all_checks_and_dcs".to_owned(), -2)],
        "the RESOLVED signed value is what gets stored (D7)"
    );
    let (_, tracked_manually, corpus) = effect_row(&pool, effect_id).await;
    assert!(!tracked_manually);
    assert_eq!(corpus, Some(corpus_id), "the provenance link is frozen");
    testing::drop_test_db(pool, "e8_corpus_valued").await;
}

#[tokio::test]
async fn a_display_only_corpus_condition_is_a_badge_chip() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "sub-alpha").await;
    let corpus_id = seed_corpus_condition(&pool, "display_only").await;

    let op = on_party(
        ClientOp {
            value: json!({
                "op": "create",
                "name": "Concealed",
                "source_character_id": alpha,
                "targets": [alpha],
                "modifiers": [],
                "duration_note": "",
                "corpus_entry_id": corpus_id,
                "condition_value": null,
            }),
            ..create_op("op-corpus-2", alpha, &[alpha])
        },
        party_id,
    );
    let result = apply_write(&pool, &player("sub-alpha"), party_id, op)
        .await
        .expect("display-only create");
    assert_eq!(
        result.outcome,
        Outcome::Applied,
        "reason: {:?}",
        result.reason
    );
    let Some((FieldTarget::Effect { effect_id }, value)) = result.broadcast else {
        panic!("display-only create broadcasts");
    };
    assert_eq!(
        value
            .get("modifiers")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
        "zero math — the engine never fabricates modifiers (US-3)"
    );
    let (active, tracked_manually, _) = effect_row(&pool, effect_id).await;
    assert!(active);
    assert!(tracked_manually, "the badge chip flag freezes at apply");
    testing::drop_test_db(pool, "e8_corpus_display_only").await;
}

// --- socket level (E7's session.rs pattern) -------------------------------

#[tokio::test]
async fn a_ws_create_acks_applied_and_diffs_a_second_client() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "dev-sub-josh").await;
    let (_, _beta_char) = seed_second_member(&pool, "dev-sub-bear").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let other_session = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    let mut watcher = connect(addr, party_id, &other_session).await;
    for client in [&mut writer, &mut watcher] {
        read_frame(&mut client.stream, "hello").await;
        read_frame(&mut client.stream, "snapshot").await;
    }

    let create = json!({
        "t": "write",
        "op_id": "ws-create-1",
        "target": {"kind": "effect_new", "party_id": party_id},
        "base_version": 0,
        "value": {"op": "create", "name": "Bless", "source_character_id": alpha,
                  "targets": [alpha],
                  "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
                  "duration_note": "10 rounds", "corpus_entry_id": null,
                  "condition_value": null}
    });
    send_raw(&mut writer.sink, &create.to_string()).await;

    // The watcher sees the diff FIRST is not required — but it sees the
    // resolved whole row either way.
    let diff = read_frame(&mut watcher.stream, "effect diff").await;
    let ServerFrame::Diff {
        field: FieldTarget::Effect { effect_id },
        value,
        version,
        actor_sub,
        op_id,
    } = diff
    else {
        panic!("the create fans out an effect diff, got {diff:?}")
    };
    assert_eq!(actor_sub, "dev-sub-josh");
    assert_eq!(op_id.as_deref(), Some("ws-create-1"));
    assert_eq!(
        value.get("name").and_then(Value::as_str),
        Some("Bless"),
        "the diff is the resolved row, not the raw op value"
    );

    // The writer gets the same news plus its ack.
    let ack = read_frame(&mut writer.stream, "create ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.op_id, "ws-create-1");
    assert_eq!(ack.outcome, Outcome::Applied);
    assert_eq!(
        ack.version,
        Some(version),
        "the ack carries the row version"
    );

    // And the row exists, exactly once.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM effects WHERE id = $1")
        .bind(effect_id)
        .fetch_one(&pool)
        .await
        .expect("effect row");
    assert_eq!(rows, 1);
    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "e8_ws_create_diff").await;
}

#[tokio::test]
async fn a_ws_invalid_stat_is_rejected_with_a_readable_reason() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    let create = json!({
        "t": "write",
        "op_id": "ws-bad-stat",
        "target": {"kind": "effect_new", "party_id": party_id},
        "base_version": 0,
        "value": {"op": "create", "name": "Initiative Buff", "source_character_id": alpha,
                  "targets": [alpha],
                  "modifiers": [{"type": "status", "stat": "initiative", "value": 2}],
                  "corpus_entry_id": null, "condition_value": null}
    });
    send_raw(&mut writer.sink, &create.to_string()).await;

    let ack = read_frame(&mut writer.stream, "rejection ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Rejected);
    assert!(
        ack.reason
            .as_deref()
            .is_some_and(|reason| reason.contains("initiative")),
        "the rejection names the offending stat: {ack:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM effects")
        .fetch_one(&pool)
        .await
        .expect("effect count");
    assert_eq!(rows, 0, "a rejected create commits nothing");
    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "e8_ws_bad_stat").await;
}

#[tokio::test]
async fn a_ws_end_at_the_current_version_ends_and_retains() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let created = apply_write(
        &pool,
        &player("dev-sub-josh"),
        party_id,
        on_party(create_op("ws-end-0", alpha, &[alpha]), party_id),
    )
    .await
    .expect("create");
    let Some((FieldTarget::Effect { effect_id }, _)) = created.broadcast else {
        panic!("create broadcasts");
    };
    let base = effect_version(&pool, effect_id).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    let end = json!({
        "t": "write",
        "op_id": "ws-end-1",
        "target": {"kind": "effect", "effect_id": effect_id},
        "base_version": base,
        "value": {"op": "end"}
    });
    send_raw(&mut writer.sink, &end.to_string()).await;

    let ack = read_frame(&mut writer.stream, "end ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Applied);
    let (active, _, _) = effect_row(&pool, effect_id).await;
    assert!(!active, "ended");
    // The row is retained (FR-15): still queryable, still counted.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM effects WHERE id = $1")
        .bind(effect_id)
        .fetch_one(&pool)
        .await
        .expect("row retained");
    assert_eq!(rows, 1);
    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "e8_ws_end").await;
}
