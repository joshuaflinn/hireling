//! Integration tests for E8's derived fan-out (plan Task 8): an applied
//! effect op is followed — in TCP order — by one `derived` frame per
//! affected character; the retarget moves exactly the delta; end restores
//! the priors; a display-only condition is a chip with zero numeric delta;
//! the reconnect snapshot carries the current derived array.

use std::time::Instant;

use serde_json::json;

use crate::engine_host::recompute::recompute_character;
use crate::pbimport::{model, transform};
use crate::sync::protocol::{FieldTarget, Outcome, ServerFrame};
use crate::sync::test_helpers::{connect, read_frame, send_raw, spawn_server};
use crate::testing;

/// The prototype's own export, verbatim — the sheet every character here
/// carries, so the derived math is the golden fixture's math.
const REFERENCE: &str = include_str!("../../../tests/data/pb_export_reference.json");

/// Seed a member with a REAL `base_sheet` (the E5 pipeline over the reference
/// export) so the extractor has honest inputs.
async fn seed_real_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
    let export = model::parse_and_validate(REFERENCE).expect("reference export is valid");
    let (sheet, skips) = transform::transform(&export);
    assert!(
        skips.sections.is_empty(),
        "the reference export transforms without skips"
    );
    let sheet_json = serde_json::to_value(&sheet).expect("sheet serializes");
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party");
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', $3) RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .bind(sheet_json)
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

/// A WS create of a named whole-sheet status effect.
fn create_frame(op_id: &str, party_id: i64, source: i64, targets: &[i64], value: i32) -> String {
    json!({
        "t": "write",
        "op_id": op_id,
        "target": {"kind": "effect_new", "party_id": party_id},
        "base_version": 0,
        "value": {"op": "create", "name": "Gloom", "source_character_id": source,
                  "targets": targets,
                  "modifiers": [{"type": "status", "stat": "all_checks_and_dcs", "value": value}],
                  "duration_note": "", "corpus_entry_id": null, "condition_value": null}
    })
    .to_string()
}

fn end_frame(op_id: &str, effect_id: i64, base_version: i64) -> String {
    json!({
        "t": "write",
        "op_id": op_id,
        "target": {"kind": "effect", "effect_id": effect_id},
        "base_version": base_version,
        "value": {"op": "end"}
    })
    .to_string()
}

fn retarget_frame(op_id: &str, effect_id: i64, base_version: i64, targets: &[i64]) -> String {
    json!({
        "t": "write",
        "op_id": op_id,
        "target": {"kind": "effect", "effect_id": effect_id},
        "base_version": base_version,
        "value": {"op": "update", "targets": targets}
    })
    .to_string()
}

/// The ac total of one derived output.
fn ac_total(output: &hireling_engine::model::EngineOutput) -> i32 {
    output.derived.ac.total
}

/// The base ac of one derived output (the sheet's own number).
fn ac_base(output: &hireling_engine::model::EngineOutput) -> i32 {
    output.derived.ac.base
}

#[tokio::test]
async fn an_applied_effect_diffs_then_derives_in_order() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let (_, _beta) = seed_real_member(&pool, "dev-sub-bear").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let other = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    let mut watcher = connect(addr, party_id, &other).await;
    for client in [&mut writer, &mut watcher] {
        read_frame(&mut client.stream, "hello").await;
        read_frame(&mut client.stream, "snapshot").await;
    }

    send_raw(
        &mut writer.sink,
        &create_frame("d-1", party_id, alpha, &[alpha], -2),
    )
    .await;

    // The watcher sees the effect diff first, THEN the derived consequence.
    let diff = read_frame(&mut watcher.stream, "effect diff").await;
    let ServerFrame::Diff {
        field: FieldTarget::Effect { effect_id },
        ..
    } = diff
    else {
        panic!("diff first, got {diff:?}")
    };
    let derived = read_frame(&mut watcher.stream, "derived frame").await;
    let ServerFrame::Derived {
        character_id,
        output,
    } = derived
    else {
        panic!("derived second, got {derived:?}")
    };
    assert_eq!(character_id, alpha, "the affected character's frame");
    assert_eq!(
        ac_total(&output),
        ac_base(&output) - 2,
        "the −2 status penalty is IN the derived total"
    );
    assert!(
        output
            .effects
            .iter()
            .any(|chip| chip.effect_id == effect_id && !chip.tracked_manually),
        "the effect rides the chip row: {:?}",
        output.effects
    );

    // The writer's ack closes the sequence.
    let ack = read_frame(&mut writer.stream, "create ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Applied);
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_diff_then_derived").await;
}

#[tokio::test]
async fn a_retarget_moves_exactly_the_delta() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let (_, beta) = seed_real_member(&pool, "dev-sub-bear").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    let snapshot = read_frame(&mut writer.stream, "snapshot").await;
    let ServerFrame::Snapshot { derived, .. } = snapshot else {
        panic!("snapshot first")
    };
    let alpha_before = derived
        .iter()
        .find(|output| output.character_id == alpha)
        .expect("alpha in snapshot");
    let alpha_base = ac_base(alpha_before);

    send_raw(
        &mut writer.sink,
        &create_frame("rt-0", party_id, alpha, &[alpha], -2),
    )
    .await;
    // Writer's own socket: ack precedes the queued diff + derived.
    let ack = read_frame(&mut writer.stream, "create ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    let frame = read_frame(&mut writer.stream, "create diff").await;
    let ServerFrame::Diff {
        field: FieldTarget::Effect { effect_id },
        version,
        ..
    } = frame
    else {
        panic!("then the diff, got {frame:?}")
    };
    let _ = read_derived(&mut writer.stream).await;
    let _ = ack;

    // Retarget: alpha departs, beta arrives.
    send_raw(
        &mut writer.sink,
        &retarget_frame("rt-1", effect_id, version, &[beta]),
    )
    .await;
    let _ = read_frame(&mut writer.stream, "retarget ack").await;
    let ServerFrame::Diff { .. } = read_frame(&mut writer.stream, "retarget diff").await else {
        panic!("retarget diffs after its ack")
    };
    let (first_id, first_out) = read_derived(&mut writer.stream).await;
    let (second_id, second_out) = read_derived(&mut writer.stream).await;
    assert_ne!(first_id, second_id, "one frame per affected character");
    let (reverted, risen) = if first_id == alpha {
        (&first_out, &second_out)
    } else {
        (&second_out, &first_out)
    };
    assert_eq!(
        ac_total(reverted),
        alpha_base,
        "the departing sheet reverts exactly"
    );
    assert_eq!(
        ac_total(risen),
        alpha_base - 2,
        "the arriving sheet falls exactly"
    );
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_retarget_delta").await;
}

/// Read one derived frame (skipping nothing else — the caller sequences).
async fn read_derived(
    stream: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> (i64, hireling_engine::model::EngineOutput) {
    let frame = read_frame(stream, "derived").await;
    match frame {
        ServerFrame::Derived {
            character_id,
            output,
        } => (character_id, *output),
        ServerFrame::Hello { .. }
        | ServerFrame::Snapshot { .. }
        | ServerFrame::Diff { .. }
        | ServerFrame::Ack(_)
        | ServerFrame::Ping
        | ServerFrame::Pong
        | ServerFrame::Bye { .. } => {
            panic!("expected a derived frame")
        }
    }
}

#[tokio::test]
async fn an_end_restores_the_priors() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let base_before = recompute_character(&pool, alpha).await.expect("baseline");
    let base_ac = ac_total(&base_before);

    let (addr, shutdown) = spawn_server(pool.clone()).await;
    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    send_raw(
        &mut writer.sink,
        &create_frame("end-0", party_id, alpha, &[alpha], -3),
    )
    .await;
    // The writer's own socket: the ack is a direct send, so it precedes the
    // party's queued diff + derived (wire contract: either order on the
    // writer's socket).
    let ack = read_frame(&mut writer.stream, "create ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    let frame = read_frame(&mut writer.stream, "create diff").await;
    let ServerFrame::Diff {
        field: FieldTarget::Effect { effect_id },
        version,
        ..
    } = frame
    else {
        panic!("then the diff, got {frame:?}")
    };
    let _ = read_frame(&mut writer.stream, "create derived").await;
    assert_eq!(version, effect_version(&pool, effect_id).await);
    let _ = ack;

    send_raw(&mut writer.sink, &end_frame("end-1", effect_id, version)).await;
    let _ = read_frame(&mut writer.stream, "end ack").await;
    let ServerFrame::Diff { .. } = read_frame(&mut writer.stream, "end diff").await else {
        panic!("end diffs after its ack")
    };
    let (_, output) = read_derived(&mut writer.stream).await;
    assert_eq!(
        ac_total(&output),
        base_ac,
        "ending the effect restores every prior"
    );
    assert!(
        output.effects.iter().all(|chip| !chip.active),
        "the ended effect's chip carries active=false"
    );
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_end_restores").await;
}

async fn effect_version(pool: &sqlx::PgPool, effect_id: i64) -> i64 {
    sqlx::query_scalar("SELECT version FROM effects WHERE id = $1")
        .bind(effect_id)
        .fetch_one(pool)
        .await
        .expect("effect version")
}

#[tokio::test]
async fn a_display_only_condition_is_a_chip_without_math() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let corpus_id: i64 = sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, modifiers) \
         VALUES ('condition', 'Concealed', 'core', \
                 '{\"import\": {\"tier\": \"display_only\"}}', NULL) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("corpus row");
    let base_before = recompute_character(&pool, alpha).await.expect("baseline");
    let base_ac = ac_total(&base_before);

    let (addr, shutdown) = spawn_server(pool.clone()).await;
    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    let create = json!({
        "t": "write",
        "op_id": "chip-1",
        "target": {"kind": "effect_new", "party_id": party_id},
        "base_version": 0,
        "value": {"op": "create", "name": "Concealed", "source_character_id": alpha,
                  "targets": [alpha], "modifiers": [], "duration_note": "",
                  "corpus_entry_id": corpus_id, "condition_value": null}
    });
    send_raw(&mut writer.sink, &create.to_string()).await;
    let _ = read_frame(&mut writer.stream, "chip ack").await;
    let ServerFrame::Diff { .. } = read_frame(&mut writer.stream, "chip diff").await else {
        panic!("chip create diffs after its ack")
    };
    let (_, output) = read_derived(&mut writer.stream).await;
    assert_eq!(
        ac_total(&output),
        base_ac,
        "a display-only condition moves no number"
    );
    let chip = output
        .effects
        .iter()
        .find(|chip| chip.name == "Concealed")
        .expect("the badge chip rides the derived output");
    assert!(chip.tracked_manually, "the chip says tracked manually");
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_display_only_chip").await;
}

#[tokio::test]
async fn a_reconnect_snapshot_carries_the_current_derived() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let (_, beta) = seed_real_member(&pool, "dev-sub-bear").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    // First connection applies an effect and disconnects.
    {
        let mut writer = connect(addr, party_id, &session).await;
        read_frame(&mut writer.stream, "hello").await;
        read_frame(&mut writer.stream, "snapshot").await;
        send_raw(
            &mut writer.sink,
            &create_frame("snap-1", party_id, alpha, &[alpha], -2),
        )
        .await;
        let _ = read_frame(&mut writer.stream, "diff").await;
        let _ = read_frame(&mut writer.stream, "derived").await;
        let _ = read_frame(&mut writer.stream, "ack").await;
    }

    // The reconnect gets the derived picture in the snapshot itself.
    let mut returning = connect(addr, party_id, &session).await;
    read_frame(&mut returning.stream, "hello").await;
    let snapshot = read_frame(&mut returning.stream, "snapshot").await;
    let ServerFrame::Snapshot { derived, .. } = snapshot else {
        panic!("snapshot on reconnect")
    };
    let alpha_out = derived
        .iter()
        .find(|output| output.character_id == alpha)
        .expect("alpha's derived rides the snapshot");
    assert_eq!(ac_total(alpha_out), ac_base(alpha_out) - 2);
    assert!(alpha_out.effects.iter().any(|chip| chip.active));
    // And the untouched member is in the array too, clean.
    let beta_out = derived
        .iter()
        .find(|output| output.character_id == beta)
        .expect("beta's derived rides the snapshot");
    assert_eq!(ac_total(beta_out), ac_base(beta_out));
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_snapshot_derived").await;
}

#[tokio::test]
async fn a_non_targeted_character_derives_byte_identical() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let (_, beta) = seed_real_member(&pool, "dev-sub-bear").await;
    let beta_before = recompute_character(&pool, beta)
        .await
        .expect("beta baseline");
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;
    send_raw(
        &mut writer.sink,
        &create_frame("iso-1", party_id, alpha, &[alpha], -5),
    )
    .await;
    let _ = read_frame(&mut writer.stream, "create ack").await;
    let _ = read_frame(&mut writer.stream, "diff").await;
    let (derived_character, _) = read_derived(&mut writer.stream).await;
    assert_eq!(derived_character, alpha, "only the target derives");

    let beta_after = recompute_character(&pool, beta).await.expect("beta after");
    let before = serde_json::to_string(&beta_before).expect("serialize before");
    let after = serde_json::to_string(&beta_after).expect("serialize after");
    assert_eq!(before, after, "the non-targeted output is byte-identical");
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_isolated").await;
}

#[tokio::test]
async fn the_derived_fan_out_stays_within_the_smoke_budget() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    // 20 create/end cycles; the smoke budget is the diff→derived gap on the
    // writer's own socket (the recompute pass), p95 < 50 ms at POC load —
    // ON TOP of E7's 100 ms dispatch budget, not inside it.
    let mut gaps: Vec<f64> = Vec::new();
    for n in 0..20 {
        send_raw(
            &mut writer.sink,
            &create_frame(&format!("smoke-{n}"), party_id, alpha, &[alpha], -1),
        )
        .await;
        let _ = read_frame(&mut writer.stream, "smoke ack").await;
        let diff_at = Instant::now();
        let ServerFrame::Diff {
            field: FieldTarget::Effect { effect_id },
            version,
            ..
        } = read_frame(&mut writer.stream, "smoke diff").await
        else {
            panic!("diff after the ack")
        };
        let _ = read_derived(&mut writer.stream).await;
        gaps.push(diff_at.elapsed().as_secs_f64() * 1000.0);
        send_raw(
            &mut writer.sink,
            &end_frame(&format!("smoke-end-{n}"), effect_id, version),
        )
        .await;
        let _ = read_frame(&mut writer.stream, "end ack").await;
        let _ = read_frame(&mut writer.stream, "end diff").await;
        let _ = read_derived(&mut writer.stream).await;
    }
    gaps.sort_by(f64::total_cmp);
    // 20 samples: the 95th percentile is the 19th order statistic — the max.
    let p95 = *gaps.last().expect("non-empty gaps");
    assert!(
        p95 < 50.0,
        "derived fan-out p95 {p95:.1} ms exceeds the 50 ms smoke budget: {gaps:?}"
    );
    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_smoke_budget").await;
}

/// A committed `level_adjust` write fans out the derived consequence —
/// ack, then the diff, then one `derived` frame carrying the adjusted
/// numbers (E6 spec §4: the level adjust visibly re-derives; the sheet
/// renders from the wire post-swap, so nothing may go stale until
/// reconnect). A superseded write fans out nothing.
#[tokio::test]
async fn a_level_adjust_write_diffs_then_derives() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;

    // The CAS anchors to the stored version, not zero (the seed consumed
    // sequence values) — the same anchor the vitals write tests use.
    let base = sqlx::query_scalar::<_, i64>(
        "SELECT level_adjust_version FROM character_vitals WHERE character_id = $1",
    )
    .bind(alpha)
    .fetch_one(&pool)
    .await
    .expect("level_adjust_version");

    send_raw(
        &mut writer.sink,
        &json!({
            "t": "write",
            "op_id": "la-applied",
            "target": {"kind": "vitals", "character_id": alpha, "field": "level_adjust"},
            "base_version": base,
            "value": 1
        })
        .to_string(),
    )
    .await;

    // Applied: ack, then the diff, then the derived consequence — in TCP
    // order, so every client applies the change and its math in sequence.
    let ack = read_frame(&mut writer.stream, "level_adjust ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Applied);
    assert!(
        ack.version.is_some(),
        "the ack carries the committed version"
    );

    let diff = read_frame(&mut writer.stream, "level_adjust diff").await;
    let ServerFrame::Diff {
        field: FieldTarget::Vitals { field, .. },
        value,
        ..
    } = &diff
    else {
        panic!("expected the level_adjust diff, got {diff:?}")
    };
    assert_eq!(*field, crate::sync::protocol::VitalsField::LevelAdjust);
    assert_eq!(*value, json!(1));
    let ServerFrame::Diff { version, .. } = &diff else {
        unreachable!()
    };
    assert_eq!(
        Some(*version),
        ack.version,
        "the diff echoes the committed version"
    );

    let derived = read_frame(&mut writer.stream, "level_adjust derived").await;
    let ServerFrame::Derived {
        character_id,
        output,
    } = derived
    else {
        panic!("expected the derived consequence, got {derived:?}")
    };
    assert_eq!(character_id, alpha, "the adjusted character's frame");
    // eff_level 4: fortitude = con 2 + trained 2+4 (7 at the export's own
    // level) — the extractor's math moved with the write, on the wire.
    assert_eq!(
        output.derived.fort.total, 8,
        "the +1 level_adjust is IN the derived total"
    );

    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_level_adjust_derives").await;
}

/// A superseded `level_adjust` write commits nothing, echoes nothing, and
/// fans out no derived frame — the next frame on the wire is the next
/// write's ack, not a recompute. (The applied twin is
/// [`a_level_adjust_write_diffs_then_derives`].)
#[tokio::test]
async fn a_superseded_level_adjust_fans_out_nothing() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    read_frame(&mut writer.stream, "hello").await;
    read_frame(&mut writer.stream, "snapshot").await;
    let base = sqlx::query_scalar::<_, i64>(
        "SELECT level_adjust_version FROM character_vitals WHERE character_id = $1",
    )
    .bind(alpha)
    .fetch_one(&pool)
    .await
    .expect("level_adjust_version");

    // Land +1 first so the second write's stale base_version misses.
    send_raw(
        &mut writer.sink,
        &json!({
            "t": "write",
            "op_id": "la-land",
            "target": {"kind": "vitals", "character_id": alpha, "field": "level_adjust"},
            "base_version": base,
            "value": 1
        })
        .to_string(),
    )
    .await;
    let ack = read_frame(&mut writer.stream, "land ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("expected the ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Applied);
    let diff = read_frame(&mut writer.stream, "land diff").await;
    let ServerFrame::Diff { .. } = diff else {
        panic!("expected the land diff, got {diff:?}")
    };
    let derived = read_frame(&mut writer.stream, "land derived").await;
    let ServerFrame::Derived { .. } = derived else {
        panic!("expected the derived consequence, got {derived:?}")
    };

    // Superseded: a second write at the stale base_version commits
    // nothing, echoes nothing, and fans out no derived frame — the next
    // frame on the wire is the next write's ack, not a recompute.
    send_raw(
        &mut writer.sink,
        &json!({
            "t": "write",
            "op_id": "la-superseded",
            "target": {"kind": "vitals", "character_id": alpha, "field": "level_adjust"},
            "base_version": base,
            "value": 2
        })
        .to_string(),
    )
    .await;
    let hp_base = sqlx::query_scalar::<_, i64>(
        "SELECT hp_version FROM character_vitals WHERE character_id = $1",
    )
    .bind(alpha)
    .fetch_one(&pool)
    .await
    .expect("hp_version");
    send_raw(
        &mut writer.sink,
        &json!({
            "t": "write",
            "op_id": "hp-after",
            "target": {"kind": "vitals", "character_id": alpha, "field": "hp"},
            "base_version": hp_base,
            "value": 20
        })
        .to_string(),
    )
    .await;
    let superseded = read_frame(&mut writer.stream, "superseded ack").await;
    let ServerFrame::Ack(superseded) = superseded else {
        panic!("expected the superseded ack, got {superseded:?}")
    };
    assert_eq!(superseded.outcome, Outcome::Superseded);
    let hp_ack = read_frame(&mut writer.stream, "hp ack").await;
    let ServerFrame::Ack(hp_ack) = hp_ack else {
        panic!("a superseded level_adjust fanned out; expected the hp ack next, got {hp_ack:?}")
    };
    assert_eq!(hp_ack.outcome, Outcome::Applied);

    shutdown.send(()).expect("test server alive");
    testing::drop_test_db(pool, "e8_level_adjust_superseded").await;
}
