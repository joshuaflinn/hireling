//! Latency harness (plan Task 14): the measured numbers against the real
//! server — dispatch p95 at POC load read from the same ring the writes
//! fed, and the shaped send→applied budgets (FR-5). The budgets are the
//! spec's: a failing number is a stop-and-report, never a threshold edit.

use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

use serde_json::json;

use crate::sync::SyncSettings;
use crate::sync::protocol::{Outcome, ServerFrame, VitalsField};
use crate::sync::shaper::{self, Profile};
use crate::sync::test_helpers::{
    PumpClient, captured_log, connect_pump, install_log_capture, pump_intro, pump_pong, pump_send,
    read_frame, read_frame_within, read_metrics_summary, seed_member, spawn_server_with,
    spawn_shared_app,
};
use crate::testing;

/// The dispatch budget (contract §7): p95 ≤ 100 ms at POC load.
const DISPATCH_BUDGET_MS: f64 = 100.0;
/// The shaped budgets (FR-5): e2e send→applied p95.
const WIFI_BUDGET_MS: f64 = 1_000.0;
const CELLULAR_BUDGET_MS: f64 = 3_000.0;
/// The plan's window (60 s at 1/s + 10 s bursts at 5/s) compressed on the
/// clock with the shape kept: five writers on the steady cadence is the
/// same instantaneous POC load, and 2 500 samples bound the p95 tighter
/// than the plan's ~70 would. Recorded in the PR body.
const LOAD_WRITERS: usize = 5;
const OPS_PER_LOAD_WRITER: u32 = 500;
/// The shaped runs: 1 000 samples each, in flight across five writers
/// (one per party member — the schema caps one character per account),
/// four vitals fields each = 20 targets. Per-target writes are
/// CAS-sequential, so every sample is an `applied` ack.
const SHAPED_SAMPLES: u32 = 1_000;
const SHAPED_WRITERS: usize = 5;
const VITALS_FIELDS: usize = 4;

/// Nearest-rank p95 over the test's own samples — the same definition the
/// server-side ring uses.
fn p95(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let rank = (95 * samples.len()).div_ceil(100);
    samples
        .get(rank.clamp(1, samples.len()) - 1)
        .copied()
        .unwrap_or_else(|| panic!("p95 of an empty sample set"))
}

/// The version column backing one vitals field (test-literal column names,
/// same policy as `vitals_version`).
fn version_column(field: VitalsField) -> &'static str {
    match field {
        VitalsField::Hp => "hp_version",
        VitalsField::TempHp => "temp_hp_version",
        VitalsField::Money => "money_version",
        VitalsField::LevelAdjust => "level_adjust_version",
        VitalsField::FocusCurrent => "focus_version",
        VitalsField::HeroPoints => "hero_points_version",
        VitalsField::Daily => "daily_version",
    }
}

/// The shaped writer: `per_target` writes per vitals field of its own
/// character, one op in flight per field (the next goes only after the
/// previous ack, so every write applies), timing each frame from send to
/// applied ack.
async fn shaped_writer(
    mut client: PumpClient,
    character_id: i64,
    bases: Vec<i64>,
    per_target: u32,
) -> Vec<f64> {
    const FIELDS: [VitalsField; VITALS_FIELDS] = [
        VitalsField::Hp,
        VitalsField::TempHp,
        VitalsField::Money,
        VitalsField::LevelAdjust,
    ];
    let mut latest: Vec<i64> = bases;
    let mut sent: Vec<u32> = vec![0; VITALS_FIELDS];
    let mut pending: HashMap<String, (usize, Instant)> = HashMap::new();
    let mut samples: Vec<f64> = Vec::new();
    let expected = usize::try_from(per_target).expect("per_target fits usize") * VITALS_FIELDS;

    for idx in 0..VITALS_FIELDS {
        send_target(
            &client,
            character_id,
            &FIELDS,
            &latest,
            &sent,
            &mut pending,
            idx,
        )
        .await;
    }
    while samples.len() < expected {
        let frame =
            read_frame_within(&mut client.stream, Duration::from_secs(30), "shaped ack").await;
        match frame {
            ServerFrame::Ack(ack) => {
                let (idx, sent_at) = pending
                    .remove(&ack.op_id)
                    .unwrap_or_else(|| panic!("ack for an op never sent: {}", ack.op_id));
                assert_eq!(
                    ack.outcome,
                    Outcome::Applied,
                    "sequenced writes must apply: {ack:?}"
                );
                let version = ack
                    .version
                    .unwrap_or_else(|| panic!("applied ack without a version"));
                if let Some(slot) = latest.get_mut(idx) {
                    *slot = version;
                }
                if let Some(count) = sent.get_mut(idx) {
                    *count += 1;
                }
                samples.push(sent_at.elapsed().as_secs_f64() * 1000.0);
                if sent.get(idx).copied().unwrap_or(0) < per_target {
                    send_target(
                        &client,
                        character_id,
                        &FIELDS,
                        &latest,
                        &sent,
                        &mut pending,
                        idx,
                    )
                    .await;
                }
            }
            ServerFrame::Ping => pump_pong(&client.sink).await,
            ServerFrame::Diff { .. }
            | ServerFrame::Hello { .. }
            | ServerFrame::Snapshot { .. }
            | ServerFrame::Pong
            | ServerFrame::Bye { .. } => {}
        }
    }
    samples
}

/// Send the next write for field `idx` and register it as pending. The
/// clock starts before the frame leaves: send→applied is the client's
/// story, wire to wire.
async fn send_target(
    client: &PumpClient,
    character_id: i64,
    fields: &[VitalsField],
    latest: &[i64],
    sent: &[u32],
    pending: &mut HashMap<String, (usize, Instant)>,
    idx: usize,
) {
    let field = fields.get(idx).copied().expect("field index");
    let n = sent.get(idx).copied().unwrap_or(0);
    let value = match field {
        VitalsField::Money => json!({"pp": 1, "gp": n, "sp": 2, "cp": 3}),
        VitalsField::LevelAdjust => json!(i64::from(n % 38) - 19),
        VitalsField::Hp | VitalsField::TempHp => json!(i64::from(n) + 1),
        VitalsField::FocusCurrent | VitalsField::HeroPoints => json!(i64::from(n)),
        VitalsField::Daily => {
            json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false})
        }
    };
    let op_id = format!("op-{character_id}-{}-{n}", field.as_str());
    let base = latest.get(idx).copied().unwrap_or(0);
    let write = json!({
        "t": "write",
        "op_id": op_id,
        "target": {"kind": "vitals", "character_id": character_id,
                   "field": field.as_str()},
        "base_version": base,
        "value": value
    });
    let sent_at = Instant::now();
    pump_send(&client.sink, &write.to_string()).await;
    pending.insert(op_id, (idx, sent_at));
}

/// The second client: consume every diff through the same shaped link —
/// the fan-out path carries load while the budgets are measured.
async fn shaped_reader(mut client: PumpClient, expected: usize) -> usize {
    let mut diffs = 0_usize;
    while diffs < expected {
        match read_frame_within(
            &mut client.stream,
            Duration::from_secs(30),
            "shaped reader's diff",
        )
        .await
        {
            ServerFrame::Diff { .. } => diffs += 1,
            ServerFrame::Ping => pump_pong(&client.sink).await,
            ServerFrame::Ack(_)
            | ServerFrame::Hello { .. }
            | ServerFrame::Snapshot { .. }
            | ServerFrame::Pong
            | ServerFrame::Bye { .. } => {}
        }
    }
    diffs
}

/// Wire one shaped run: real server, both clients through the proxy, the
/// writer measuring `SHAPED_SAMPLES` send→applied times. `None` skips when
/// no test Postgres is reachable.
async fn shaped_send_to_applied(profile: Profile, seed: u64) -> Option<f64> {
    const SUBS: [&str; SHAPED_WRITERS] = [
        "dev-sub-josh",
        "dev-sub-bear",
        "dev-sub-dave",
        "dev-sub-becky",
        "dev-sub-jake",
    ];
    let pool = testing::test_pool().await?;
    let mut characters = Vec::new();
    let mut party_seen = None;
    for sub in SUBS {
        let (party, character_id) = seed_member(&pool, sub).await;
        match party_seen {
            Some(first) => assert_eq!(party, first, "one POC party"),
            None => party_seen = Some(party),
        }
        characters.push(character_id);
    }
    let party_id = party_seen.expect("the POC party is seeded");

    let settings = SyncSettings {
        ping_interval: Duration::from_secs(10),
        pong_timeout: Duration::from_secs(5),
    };
    let (server_addr, shutdown) = spawn_server_with(pool.clone(), settings).await;
    let shaped_addr = shaper::spawn(server_addr, profile, seed).await;

    // The GM's socket rides the same shaped link and proves the fan-out
    // delivered every diff while the budgets were measured.
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let session_r = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let mut reader = connect_pump(shaped_addr, party_id, &session_r).await;
    pump_intro(&mut reader, "shaped reader").await;

    let expected = usize::try_from(SHAPED_SAMPLES).expect("samples fit usize");
    let reader_task = tokio::spawn(shaped_reader(reader, expected));

    let per_target =
        SHAPED_SAMPLES / u32::try_from(SHAPED_WRITERS * VITALS_FIELDS).expect("targets fit u32");
    let mut tasks = Vec::new();
    for (idx, &character_id) in characters.iter().enumerate() {
        let sub = SUBS.get(idx).copied().expect("writers and subs line up");
        let session = testing::seed_session(&pool, sub, chrono::Utc::now()).await;
        let mut writer = connect_pump(shaped_addr, party_id, &session).await;
        pump_intro(&mut writer, "shaped writer").await;
        let mut bases = Vec::new();
        for field in [
            VitalsField::Hp,
            VitalsField::TempHp,
            VitalsField::Money,
            VitalsField::LevelAdjust,
        ] {
            let column = version_column(field);
            bases
                .push(crate::sync::test_helpers::vitals_version(&pool, character_id, column).await);
        }
        tasks.push(tokio::spawn(shaped_writer(
            writer,
            character_id,
            bases,
            per_target,
        )));
    }
    let mut samples: Vec<f64> = Vec::new();
    for task in tasks {
        samples.extend(task.await.expect("shaped writer joins"));
    }
    let reader_diffs = reader_task.await.expect("shaped reader joins");
    assert_eq!(
        reader_diffs, expected,
        "the shaped fan-out delivered every diff"
    );
    assert_eq!(samples.len(), expected, "every writer produced its samples");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    let measured = p95(&mut samples.clone());
    testing::drop_test_db(pool, "sync_latency_shaped").await;
    Some(measured)
}

/// Dispatch p95 at POC load, read from the ring the writes fed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dispatch_p95_at_poc_load_stays_within_100ms() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let subs = [
        "dev-sub-josh",
        "dev-sub-bear",
        "dev-sub-dave",
        "dev-sub-becky",
        "dev-sub-jake",
    ];
    let mut characters = Vec::new();
    let mut sessions = Vec::new();
    let mut party_seen = None;
    for sub in subs {
        let (party, character_id) = seed_member(&pool, sub).await;
        match party_seen {
            Some(first) => assert_eq!(party, first, "one POC party"),
            None => party_seen = Some(party),
        }
        characters.push(character_id);
        sessions.push(testing::seed_session(&pool, sub, chrono::Utc::now()).await);
    }
    let party_id = party_seen.expect("the POC party is seeded");
    let (app, addr, shutdown) = spawn_shared_app(pool.clone()).await;
    let first_session = sessions.first().expect("non-empty sessions");
    let cookie = format!("__Host-hireling_session={first_session}");
    let before = read_metrics_summary(&app, &cookie).await;
    assert_eq!(
        before.get("count"),
        Some(&json!(0)),
        "a fresh window dispatches nothing: {before}"
    );

    let mut tasks = Vec::new();
    for (&character_id, session) in characters.iter().zip(sessions.iter()) {
        let base =
            crate::sync::test_helpers::vitals_version(&pool, character_id, "hp_version").await;
        let mut client = connect_pump(addr, party_id, session).await;
        pump_intro(&mut client, "load writer").await;
        tasks.push(tokio::spawn(run_load_writer(client, character_id, base)));
    }
    for task in tasks {
        task.await.expect("load writer joins");
    }

    let summary = read_metrics_summary(&app, &cookie).await;
    let expected =
        i64::try_from(LOAD_WRITERS).expect("writers fit i64") * i64::from(OPS_PER_LOAD_WRITER);
    assert_eq!(
        summary.get("count"),
        Some(&json!(expected)),
        "every applied write sampled the ring: {summary}"
    );
    let measured = summary
        .get("p95_ms")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(f64::INFINITY);
    assert!(
        measured <= DISPATCH_BUDGET_MS,
        "dispatch p95 {measured} ms exceeds the {DISPATCH_BUDGET_MS} ms budget: {summary}"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "sync_latency_dispatch").await;
}

/// One load writer: `OPS_PER_LOAD_WRITER` steady-cadence hp writes to its
/// own character, ack-sequential.
async fn run_load_writer(mut client: PumpClient, character_id: i64, base: i64) {
    let mut last_version = base;
    for n in 0..OPS_PER_LOAD_WRITER {
        tokio::time::sleep(Duration::from_millis(2)).await;
        let op_id = format!("op-load-{character_id}-{n}");
        let write = json!({
            "t": "write",
            "op_id": op_id,
            "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
            "base_version": last_version,
            "value": i64::from(n) + 1
        });
        pump_send(&client.sink, &write.to_string()).await;
        loop {
            let frame =
                read_frame_within(&mut client.stream, Duration::from_secs(20), "load ack").await;
            match frame {
                ServerFrame::Ack(ack) if ack.op_id == op_id => {
                    assert_eq!(
                        ack.outcome,
                        Outcome::Applied,
                        "uncontended load write must apply: {ack:?}"
                    );
                    last_version = ack
                        .version
                        .unwrap_or_else(|| panic!("applied ack without a version"));
                    break;
                }
                ServerFrame::Ack(unexpected) => panic!("unexpected load ack: {unexpected:?}"),
                ServerFrame::Ping => pump_pong(&client.sink).await,
                ServerFrame::Diff { .. }
                | ServerFrame::Hello { .. }
                | ServerFrame::Snapshot { .. }
                | ServerFrame::Pong
                | ServerFrame::Bye { .. } => {}
            }
        }
    }
}

/// The wifi budget (FR-5): send→applied p95 < 1 s through a shaped link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shaped_wifi_send_to_applied_p95_stays_under_1s() {
    match shaped_send_to_applied(shaper::WIFI, 0x5EED_0000_0001).await {
        Some(measured) => assert!(
            measured < WIFI_BUDGET_MS,
            "wifi send→applied p95 {measured} ms exceeds the {WIFI_BUDGET_MS} ms budget"
        ),
        None => eprintln!("skipping shaped wifi budget: no test Postgres"),
    }
}

/// The cellular budget (FR-5): send→applied p95 < 3 s through a shaped link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shaped_cellular_send_to_applied_p95_stays_under_3s() {
    match shaped_send_to_applied(shaper::CELLULAR, 0x5EED_0000_0002).await {
        Some(measured) => assert!(
            measured < CELLULAR_BUDGET_MS,
            "cellular send→applied p95 {measured} ms exceeds the {CELLULAR_BUDGET_MS} ms budget"
        ),
        None => eprintln!("skipping shaped cellular budget: no test Postgres"),
    }
}

/// The snapshot size is logged on the production path (contract §7): a
/// real connect emits the structured event with the byte count.
#[tokio::test]
async fn the_snapshot_size_is_logged_on_the_production_path() {
    install_log_capture();
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server_with(pool.clone(), SyncSettings::default()).await;

    let mut client = crate::sync::test_helpers::connect(addr, party_id, &session).await;
    let hello = read_frame(&mut client.stream, "hello").await;
    assert!(
        matches!(hello, ServerFrame::Hello { .. }),
        "the greeting comes first: {hello:?}"
    );
    let snapshot = read_frame(&mut client.stream, "snapshot").await;
    let bytes = match &snapshot {
        ServerFrame::Snapshot { snapshot_bytes, .. } => *snapshot_bytes,
        ServerFrame::Hello { .. }
        | ServerFrame::Diff { .. }
        | ServerFrame::Ack(_)
        | ServerFrame::Ping
        | ServerFrame::Pong
        | ServerFrame::Bye { .. } => {
            panic!("expected a snapshot, got {snapshot:?}")
        }
    };
    assert!(bytes > 0, "the frame carries its size: {bytes}");

    let log = captured_log().lock().expect("capture lock").join("\n");
    assert!(
        log.contains("snapshot sent"),
        "the production snapshot log is captured: {log}"
    );
    assert!(
        log.contains("snapshot_bytes="),
        "the log carries the byte count: {log}"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "sync_snapshot_bytes_log").await;
}
