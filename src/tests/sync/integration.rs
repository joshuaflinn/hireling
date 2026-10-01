//! Integration suite (plan Task 13): the spec's proof over real sockets —
//! the offline-queue replay story, ack-loss exactly-once, the 10 000-op
//! soak across a server restart, and Task 5's stalled-reader eviction on a
//! live wire. Production code is expected to already satisfy every
//! assertion here; a failure is a finding, never a test to loosen.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::StreamExt as _;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use crate::sync::SyncSettings;
use crate::sync::protocol::{FieldTarget, Outcome, ServerFrame, VitalsField};
use crate::sync::test_helpers::{
    Client, PumpClient, WsSink, WsStream, connect, connect_pump, pump_intro, pump_pong, pump_send,
    read_frame, read_frame_within, read_until_close, seed_member, seed_spell_slot, send_raw,
    slot_version, spawn_server, spawn_server_with, vitals_version,
};
use crate::testing;

// --- small shared assertions ----------------------------------------------

/// Drain one client's `hello` → `snapshot` intro; returns the snapshot.
async fn intro(client: &mut Client, what: &str) -> ServerFrame {
    let hello = read_frame(&mut client.stream, &format!("{what} hello")).await;
    assert!(
        matches!(hello, ServerFrame::Hello { .. }),
        "{what} must be greeted: {hello:?}"
    );
    let snapshot = read_frame(&mut client.stream, &format!("{what} snapshot")).await;
    assert!(
        matches!(snapshot, ServerFrame::Snapshot { .. }),
        "{what} must receive a snapshot: {snapshot:?}"
    );
    snapshot
}

/// Send one hp write frame through a plain client.
async fn send_hp_write(
    client: &mut Client,
    op_id: &str,
    character_id: i64,
    base_version: i64,
    hp: i64,
) {
    let write = json!({
        "t": "write",
        "op_id": op_id,
        "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
        "base_version": base_version,
        "value": hp
    });
    send_raw(&mut client.sink, &write.to_string()).await;
}

/// The applied ack's version, asserted against the op it answers.
fn expect_applied_ack(frame: ServerFrame, op_id: &str) -> i64 {
    let ServerFrame::Ack(ack) = frame else {
        panic!("expected the ack for {op_id}, got {frame:?}");
    };
    assert_eq!(ack.op_id, op_id, "the ack answers this op");
    assert_eq!(ack.outcome, Outcome::Applied, "expected applied: {ack:?}");
    ack.version
        .unwrap_or_else(|| panic!("applied ack for {op_id} carries no version"))
}

/// One row of a snapshot: the hp value and version for the character.
fn hp_row(snapshot: &ServerFrame, character_id: i64, what: &str) -> (serde_json::Value, i64) {
    let ServerFrame::Snapshot { fields, .. } = snapshot else {
        panic!("{what} must be a snapshot, got {snapshot:?}");
    };
    for row in fields {
        if let FieldTarget::Vitals {
            character_id: row_character,
            field: VitalsField::Hp,
        } = row.field
            && row_character == character_id
        {
            return (row.value.clone(), row.version);
        }
    }
    panic!("{what} has no hp row for character {character_id}");
}

/// The ledger row for one op: `(outcome, resulting_version)`.
async fn ledger_row(pool: &sqlx::PgPool, op_id: &str) -> (String, Option<i64>) {
    sqlx::query_as("SELECT outcome, resulting_version FROM client_ops WHERE op_id = $1")
        .bind(op_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("ledger row for {op_id}: {err}"))
}

// --- (a) the offline-queue story over the wire (spec FR-8) -----------------

/// Device A1 writes, is severed, and the world moves on: client B moves its
/// own field, A's second device wins A's field. On return A1 merges the
/// snapshot, replays its queued op against a stale base, is told
/// `superseded` with the winning version, and *no diff reaches any socket*
/// for it (UI-silent, contract §4). Every step leaves a ledger row.
#[tokio::test]
async fn an_offline_queue_replays_into_a_moved_field_as_superseded_and_stays_ui_silent() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, char_a) = seed_member(&pool, "dev-sub-josh").await;
    let (party_b, char_b) = seed_member(&pool, "dev-sub-bear").await;
    assert_eq!(party_id, party_b, "both members share the POC party");
    let session_a1 = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let session_a2 = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let session_b = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    // Device A1's last live write; the queued op below drafts against v1.
    let mut device_a1 = connect(addr, party_id, &session_a1).await;
    intro(&mut device_a1, "device a1").await;
    let base_a = vitals_version(&pool, char_a, "hp_version").await;
    send_hp_write(&mut device_a1, "op-queued-1", char_a, base_a, 10).await;
    let v1 = expect_applied_ack(
        read_frame(&mut device_a1.stream, "ack for op-queued-1").await,
        "op-queued-1",
    );

    // Client B moves its own hp: the server state A must merge on return.
    let mut client_b = connect(addr, party_id, &session_b).await;
    intro(&mut client_b, "client b").await;
    let base_b = vitals_version(&pool, char_b, "hp_version").await;
    send_hp_write(&mut client_b, "op-b-live", char_b, base_b, 33).await;
    let v_b = expect_applied_ack(
        read_frame(&mut client_b.stream, "ack for op-b-live").await,
        "op-b-live",
    );
    drop(device_a1); // severed: no close handshake, the socket just dies.

    // A's second device wins the field while the queue holds the stale op.
    let mut device_a2 = connect(addr, party_id, &session_a2).await;
    intro(&mut device_a2, "device a2").await;
    send_hp_write(&mut device_a2, "op-a2-newer", char_a, v1, 20).await;
    let v2 = expect_applied_ack(
        read_frame(&mut device_a2.stream, "ack for op-a2-newer").await,
        "op-a2-newer",
    );

    // A1 returns: hello, then the merged snapshot — B's move and A2's win.
    let mut returning = connect(addr, party_id, &session_a1).await;
    let merged = intro(&mut returning, "returning a1").await;
    let (value_b, version_b) = hp_row(&merged, char_b, "merged snapshot");
    assert_eq!(
        (value_b, version_b),
        (json!(33), v_b),
        "the snapshot carries client B's move"
    );
    assert_eq!(
        hp_row(&merged, char_a, "merged snapshot"),
        (json!(20), v2),
        "the snapshot carries the second device's win"
    );

    // The queued op replays against its stale base: superseded, honestly.
    send_hp_write(&mut returning, "op-queued-2", char_a, v1, 15).await;
    let ServerFrame::Ack(replay) = read_frame(&mut returning.stream, "ack for op-queued-2").await
    else {
        panic!("expected the replay's ack");
    };
    assert_eq!(replay.op_id, "op-queued-2", "the replayed op's ack");
    assert_eq!(
        replay.outcome,
        Outcome::Superseded,
        "stale base loses: {replay:?}"
    );
    assert_eq!(
        replay.winning_version,
        Some(v2),
        "the ack names the winning version"
    );

    // UI-silent on the wire: the witnesses see nothing but the probe's pong.
    let b_diff = read_frame(&mut client_b.stream, "b's own diff").await;
    assert!(
        matches!(&b_diff, ServerFrame::Diff { op_id: Some(op), .. } if op == "op-b-live"),
        "b's applied write diffs: {b_diff:?}"
    );
    let b_copy = read_frame(&mut client_b.stream, "b's copy of a2's diff").await;
    assert!(
        matches!(&b_copy, ServerFrame::Diff { op_id: Some(op), .. } if op == "op-a2-newer"),
        "b sees the applied write on every socket: {b_copy:?}"
    );
    send_raw(&mut client_b.sink, r#"{"t":"ping"}"#).await;
    let probe_b = read_frame(&mut client_b.stream, "b silence probe").await;
    assert!(
        matches!(probe_b, ServerFrame::Pong),
        "no diff for the superseded op reached b: {probe_b:?}"
    );
    let a2_diff = read_frame(&mut device_a2.stream, "a2's own diff").await;
    assert!(
        matches!(&a2_diff, ServerFrame::Diff { op_id: Some(op), .. } if op == "op-a2-newer"),
        "a2's applied write diffs: {a2_diff:?}"
    );
    send_raw(&mut device_a2.sink, r#"{"t":"ping"}"#).await;
    let probe_a2 = read_frame(&mut device_a2.stream, "a2 silence probe").await;
    assert!(
        matches!(probe_a2, ServerFrame::Pong),
        "no diff for the superseded op reached a2: {probe_a2:?}"
    );

    assert_offline_ledger(&pool, char_a, v1, v2, v_b).await;

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "sync_offline_replay").await;
}

/// Every step of the offline story left its durable row, and the
/// superseded replay moved nothing.
async fn assert_offline_ledger(pool: &sqlx::PgPool, char_a: i64, v1: i64, v2: i64, v_b: i64) {
    assert_eq!(
        ledger_row(pool, "op-queued-1").await,
        ("applied".to_owned(), Some(v1)),
        "the live write is ledgered"
    );
    assert_eq!(
        ledger_row(pool, "op-b-live").await,
        ("applied".to_owned(), Some(v_b)),
        "b's move is ledgered"
    );
    assert_eq!(
        ledger_row(pool, "op-a2-newer").await,
        ("applied".to_owned(), Some(v2)),
        "the second device's win is ledgered"
    );
    assert_eq!(
        ledger_row(pool, "op-queued-2").await,
        ("superseded".to_owned(), None),
        "the replay is ledgered without a version: it wrote nothing"
    );
    assert_eq!(
        vitals_version(pool, char_a, "hp_version").await,
        v2,
        "the superseded replay moved nothing"
    );
}

// --- (b) ack-loss replay: exactly-once (spec FR-8, degraded-mode.md) -------

/// The client sends the write and dies before any frame comes back — the
/// ack is lost to it no matter what the server pushed. The commit is proven
/// by the database; the replay of the same `op_id` answers `already_applied`
/// from the ledger with exactly one version bump total.
#[tokio::test]
async fn an_ack_losed_write_replays_exactly_once_as_already_applied() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    intro(&mut writer, "writer").await;
    let base = vitals_version(&pool, character_id, "hp_version").await;

    send_hp_write(&mut writer, "op-lost-1", character_id, base, 7).await;
    drop(writer); // the ack (if sent) is lost; the socket is gone.

    let mut applied = base;
    for _ in 0..100 {
        applied = vitals_version(&pool, character_id, "hp_version").await;
        if applied > base {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(applied > base, "the write must have committed");
    assert_eq!(
        ledger_row(&pool, "op-lost-1").await,
        ("applied".to_owned(), Some(applied)),
        "the lost-ack write is ledgered at its version"
    );

    // The client replays the same op_id from its durable queue.
    let mut replier = connect(addr, party_id, &session).await;
    intro(&mut replier, "replaying writer").await;
    send_hp_write(&mut replier, "op-lost-1", character_id, base, 7).await;
    let ServerFrame::Ack(ack) = read_frame(&mut replier.stream, "replay ack").await else {
        panic!("expected the replay's ack");
    };
    assert_eq!(ack.op_id, "op-lost-1", "the replay's op_id");
    assert_eq!(
        ack.outcome,
        Outcome::AlreadyApplied,
        "the ledger answers, the engine does not re-run: {ack:?}"
    );
    assert_eq!(
        ack.version,
        Some(applied),
        "the original version is returned"
    );

    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        applied,
        "exactly one version bump total"
    );
    let hp: i32 = sqlx::query_scalar("SELECT hp FROM character_vitals WHERE character_id = $1")
        .bind(character_id)
        .fetch_one(&pool)
        .await
        .expect("hp value");
    assert_eq!(i64::from(hp), 7, "the first commit's value stands");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM client_ops WHERE op_id = 'op-lost-1'")
        .fetch_one(&pool)
        .await
        .expect("ledger row count");
    assert_eq!(rows, 1, "the replay stored nothing twice");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "sync_ack_loss_replay").await;
}

// --- (c) the soak: 6 clients, 10 000 ops, server restart (SC-3) ------------

/// The soak's writers: five players, each owning its character's hp — no
/// CAS contention is possible across writers, so every op must apply.
const WRITERS: usize = 5;
const OPS_PER_WRITER: u32 = 2_000;
/// The first three writers hold a steady cadence; the rest fire bursts.
const STEADY_WRITERS: usize = 3;
/// The plan's shape (steady 1/s + 5-op bursts) with the clock compressed
/// ~500x: literal wall-clock pacing would run 2.8 hours. The properties
/// under test — exactly-once ledger, fan-out convergence, durability
/// across restart — are rate-invariant; the dispatch budget itself is
/// Task 14's measurement. Recorded in the PR body.
const STEADY_TICK: Duration = Duration::from_millis(2);
const BURST_SIZE: u32 = 5;
const BURST_PAUSE: Duration = Duration::from_millis(10);
/// Quiet-for-this-long after `done` means every broadcast frame landed:
/// the registry enqueues synchronously at commit time.
const QUIET_WINDOW: Duration = Duration::from_millis(400);

/// What one writer took away from the soak.
struct WriterReport {
    character_id: i64,
    applied: u32,
    last_version: i64,
    /// Max hp version seen per character across acks and diffs.
    seen: HashMap<i64, i64>,
}

fn record_seen(seen: &mut HashMap<i64, i64>, character_id: i64, version: i64) {
    seen.entry(character_id)
        .and_modify(|current| *current = (*current).max(version))
        .or_insert(version);
}

/// The compressed load shape: steady writers tick between ops; burst
/// writers fire `BURST_SIZE` then pause.
async fn pace(burst: bool, n: u32) {
    if burst && n.is_multiple_of(BURST_SIZE) {
        tokio::time::sleep(BURST_PAUSE).await;
    } else if !burst {
        tokio::time::sleep(STEADY_TICK).await;
    }
}

/// Read until `done` is set and the socket has been quiet for a window,
/// tracking hp diff versions and answering liveness pings.
async fn drain_tail(
    stream: &mut WsStream,
    sink: &Arc<tokio::sync::Mutex<WsSink>>,
    seen: &mut HashMap<i64, i64>,
    done: &tokio::sync::watch::Receiver<bool>,
) {
    loop {
        match tokio::time::timeout(QUIET_WINDOW, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ServerFrame>(&text) {
                Ok(ServerFrame::Diff {
                    field:
                        FieldTarget::Vitals {
                            character_id,
                            field: VitalsField::Hp,
                        },
                    version,
                    ..
                }) => record_seen(seen, character_id, version),
                Ok(ServerFrame::Ping) => pump_pong(sink).await,
                Ok(_) => {}
                Err(err) => panic!("drain frame does not decode: {err}"),
            },
            Ok(Some(Ok(
                Message::Binary(_)
                | Message::Ping(_)
                | Message::Pong(_)
                | Message::Close(_)
                | Message::Frame(_),
            ))) => {}
            Ok(Some(Err(err))) => panic!("socket error in the drain tail: {err}"),
            Ok(None) => break,
            Err(_quiet) => {
                if *done.borrow() {
                    break;
                }
            }
        }
    }
}

/// One soak writer: `OPS_PER_WRITER` sequential hp writes to its own
/// character at the compressed cadence, then a drain tail so the writer
/// holds every field's final version before reporting.
async fn run_writer(
    mut client: PumpClient,
    character_id: i64,
    base: i64,
    burst: bool,
    ops_done: Arc<std::sync::atomic::AtomicUsize>,
    done: tokio::sync::watch::Receiver<bool>,
) -> WriterReport {
    let mut last_version = base;
    let mut applied = 0_u32;
    let mut seen: HashMap<i64, i64> = HashMap::new();
    seen.insert(character_id, base);
    for n in 0..OPS_PER_WRITER {
        pace(burst, n).await;
        let op_id = format!("op-{character_id}-{n}");
        let write = json!({
            "t": "write",
            "op_id": op_id,
            "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
            "base_version": last_version,
            "value": i64::from(n) + 1
        });
        pump_send(&client.sink, &write.to_string()).await;
        let what = format!("ack for {op_id}");
        loop {
            match read_frame_within(&mut client.stream, Duration::from_secs(20), &what).await {
                ServerFrame::Ack(ack) if ack.op_id == op_id => {
                    assert_eq!(
                        ack.outcome,
                        Outcome::Applied,
                        "uncontended write must apply: {ack:?}"
                    );
                    let version = ack
                        .version
                        .unwrap_or_else(|| panic!("applied ack without a version: {ack:?}"));
                    last_version = version;
                    record_seen(&mut seen, character_id, version);
                    applied += 1;
                    break;
                }
                ServerFrame::Ack(unexpected) => panic!("unexpected ack: {unexpected:?}"),
                ServerFrame::Diff {
                    field:
                        FieldTarget::Vitals {
                            character_id: cid,
                            field: VitalsField::Hp,
                        },
                    version,
                    ..
                } => record_seen(&mut seen, cid, version),
                ServerFrame::Ping => pump_pong(&client.sink).await,
                ServerFrame::Derived { .. }
                | ServerFrame::Diff { .. }
                | ServerFrame::Hello { .. }
                | ServerFrame::Snapshot { .. }
                | ServerFrame::Pong
                | ServerFrame::Bye { .. } => {}
            }
        }
    }
    ops_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    drain_tail(&mut client.stream, &client.sink, &mut seen, &done).await;
    WriterReport {
        character_id,
        applied,
        last_version,
        seen,
    }
}

/// Every applied op is in the ledger exactly once, every writer applied all
/// its ops, and every witness (writers + the GM observer) converged on the
/// final version of every field.
async fn assert_soak_outcome(
    pool: &sqlx::PgPool,
    reports: &[WriterReport],
    characters: &[i64],
    gm_seen: &HashMap<i64, i64>,
) {
    let expected_total =
        i64::try_from(WRITERS).expect("writers fit i64") * i64::from(OPS_PER_WRITER);
    for report in reports {
        assert_eq!(
            report.applied, OPS_PER_WRITER,
            "writer {} applied every op",
            report.character_id
        );
    }
    let (total_rows, distinct_ops): (i64, i64) =
        sqlx::query_as("SELECT count(*), count(DISTINCT op_id) FROM client_ops")
            .fetch_one(pool)
            .await
            .expect("ledger totals");
    assert_eq!(
        (total_rows, distinct_ops),
        (expected_total, expected_total),
        "every op is in the ledger exactly once"
    );
    let strays: i64 =
        sqlx::query_scalar("SELECT count(*) FROM client_ops WHERE outcome <> 'applied'")
            .fetch_one(pool)
            .await
            .expect("outcome audit");
    assert_eq!(strays, 0, "an uncontended soak applies everything");

    for (&character_id, report) in characters.iter().zip(reports.iter()) {
        let db_version = vitals_version(pool, character_id, "hp_version").await;
        assert_eq!(
            report.last_version, db_version,
            "writer {}'s last ack is the row's version",
            report.character_id
        );
        for witness in reports {
            let seen = witness.seen.get(&character_id).unwrap_or_else(|| {
                panic!(
                    "writer {} never saw char {}",
                    witness.character_id, character_id
                )
            });
            assert_eq!(
                seen, &db_version,
                "writer {} converged on char {}",
                witness.character_id, character_id
            );
        }
        let gm_version = gm_seen
            .get(&character_id)
            .unwrap_or_else(|| panic!("the gm never saw char {character_id}"));
        assert_eq!(
            gm_version, &db_version,
            "the gm converged on char {character_id}"
        );
        let last_op = format!("op-{character_id}-{}", OPS_PER_WRITER - 1);
        let (outcome, resulting) = ledger_row(pool, &last_op).await;
        assert_eq!(outcome, "applied", "the final op of {character_id}");
        assert_eq!(resulting, Some(db_version), "the final op's ledger version");
    }
}

/// The restarted server serves exactly the acknowledged world: a fresh
/// client's snapshot matches the database and every writer's last ack.
async fn assert_restart_recovery(
    pool: &sqlx::PgPool,
    addr: std::net::SocketAddr,
    session: &str,
    party_id: i64,
    reports: &[WriterReport],
    characters: &[i64],
) {
    let mut client = connect(addr, party_id, session).await;
    let snapshot = intro(&mut client, "post-restart").await;
    for (report, &character_id) in reports.iter().zip(characters.iter()) {
        let db_version = vitals_version(pool, character_id, "hp_version").await;
        assert_eq!(
            hp_row(&snapshot, character_id, "post-restart"),
            (json!(i64::from(OPS_PER_WRITER)), db_version),
            "char {character_id} kept its last acknowledged value and version"
        );
        assert_eq!(
            report.last_version, db_version,
            "char {character_id}'s writer agrees with the restarted server"
        );
    }
    let (total_rows, distinct_ops): (i64, i64) =
        sqlx::query_as("SELECT count(*), count(DISTINCT op_id) FROM client_ops")
            .fetch_one(pool)
            .await
            .expect("ledger totals after restart");
    let expected_total =
        i64::try_from(WRITERS).expect("writers fit i64") * i64::from(OPS_PER_WRITER);
    assert_eq!(
        (total_rows, distinct_ops),
        (expected_total, expected_total),
        "the ledger survived the restart, still exactly-once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn six_clients_soak_ten_thousand_ops_through_a_server_restart_with_zero_losses() {
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
            Some(seen) => assert_eq!(party, seen, "one POC party"),
            None => party_seen = Some(party),
        }
        characters.push(character_id);
        sessions.push(testing::seed_session(&pool, sub, chrono::Utc::now()).await);
    }
    let party_id = party_seen.expect("the POC party is seeded");
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;

    // Short liveness so the soak proves sessions survive ping cycles.
    let settings = SyncSettings {
        ping_interval: Duration::from_secs(2),
        pong_timeout: Duration::from_secs(1),
    };
    // The server gets its own pool: five concurrent writers hold a
    // connection per transaction, and the shared test pool's cap of 5
    // would queue them behind each other.
    let server_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(12)
        .connect_with((*pool.connect_options()).clone())
        .await
        .expect("server pool for the soak");
    let (addr, shutdown) = spawn_server_with(server_pool, settings).await;

    let (done_tx, done_rx) = tokio::sync::watch::channel(false);
    let ops_done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for (idx, (&character_id, session)) in characters.iter().zip(sessions.iter()).enumerate() {
        let base = vitals_version(&pool, character_id, "hp_version").await;
        let mut client = connect_pump(addr, party_id, session).await;
        pump_intro(&mut client, "soak writer").await;
        let burst = idx >= STEADY_WRITERS;
        tasks.push(tokio::spawn(run_writer(
            client,
            character_id,
            base,
            burst,
            Arc::clone(&ops_done),
            done_rx.clone(),
        )));
    }
    let mut gm = connect_pump(addr, party_id, &gm_session).await;
    pump_intro(&mut gm, "gm observer").await;
    let gm_task = tokio::spawn(async move {
        let mut gm_seen: HashMap<i64, i64> = HashMap::new();
        drain_tail(&mut gm.stream, &gm.sink, &mut gm_seen, &done_rx).await;
        gm_seen
    });

    // Writers exit their drain tail only when `done` fires, so the flag is
    // gated on the ops-done counter (not on task completion): joining first
    // would deadlock both sides.
    while ops_done.load(std::sync::atomic::Ordering::Relaxed) < WRITERS {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    done_tx.send_replace(true);
    let mut reports = Vec::new();
    for task in tasks {
        reports.push(task.await.expect("soak writer joins"));
    }
    let gm_seen = gm_task.await.expect("gm observer joins");

    assert_soak_outcome(&pool, &reports, &characters, &gm_seen).await;

    // Restart: the drain, then drop + recreate listener and pool in-process.
    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before the restart"
    );
    let options = (*pool.connect_options()).clone();
    drop(pool);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let restarted_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .expect("recreated pool");
    let (restart_addr, restart_shutdown) = spawn_server(restarted_pool.clone()).await;

    let first_session = sessions.first().expect("non-empty sessions");
    assert_restart_recovery(
        &restarted_pool,
        restart_addr,
        first_session,
        party_id,
        &reports,
        &characters,
    )
    .await;

    assert!(
        restart_shutdown.send(()).is_ok(),
        "the restarted server died before shutdown"
    );
    testing::drop_test_db(restarted_pool, "sync_soak_restart").await;
}

// --- (d) stalled-reader eviction over real sockets (Task 5's mechanism) ----

/// The flood writes one huge `prepared` string per slot write: a frame big
/// enough that a stalled reader's TCP buffers provably back up the session
/// sink and overflow the 64-slot outbound registry channel. Numbers chosen
/// against worst-case kernel buffers (~10 MB loopback) plus the 64-frame
/// registry window; tuning them is not a budget question.
const FLOOD_WRITES: usize = 140;
const PREPARED_BYTES: usize = 400_000;

/// Drive `FLOOD_WRITES` slot writes from the writer, reading its own acks
/// (and its own giant diff copies) on the way. Returns the final version.
async fn flood_slot_writes(writer: &mut Client, character_id: i64, base: i64) -> i64 {
    let big = "x".repeat(PREPARED_BYTES);
    let mut version = base;
    for n in 0..FLOOD_WRITES {
        let op_id = format!("op-flood-{n}");
        let value = json!({"used": n % 2 == 0, "prepared": big});
        let write = json!({
            "t": "write",
            "op_id": op_id,
            "target": {"kind": "slot", "character_id": character_id,
                       "caster_key": "Wizard", "rank": 1, "slot_index": 0},
            "base_version": version,
            "value": value
        });
        send_raw(&mut writer.sink, &write.to_string()).await;
        let what = format!("ack for {op_id}");
        loop {
            match read_frame_within(&mut writer.stream, Duration::from_secs(30), &what).await {
                ServerFrame::Ack(ack) if ack.op_id == op_id => {
                    assert_eq!(
                        ack.outcome,
                        Outcome::Applied,
                        "flood write {n} applies: {ack:?}"
                    );
                    version = ack
                        .version
                        .unwrap_or_else(|| panic!("applied flood ack without a version"));
                    break;
                }
                ServerFrame::Ack(unexpected) => panic!("unexpected flood ack: {unexpected:?}"),
                ServerFrame::Ping => {
                    send_raw(&mut writer.sink, r#"{"t":"pong"}"#).await;
                }
                ServerFrame::Derived { .. }
                | ServerFrame::Diff { .. }
                | ServerFrame::Hello { .. }
                | ServerFrame::Snapshot { .. }
                | ServerFrame::Pong
                | ServerFrame::Bye { .. } => {}
            }
        }
    }
    version
}

/// The live reader's whole job during the flood: count the slot diffs as
/// they arrive (proving the fan-out never stalled) and answer pings.
async fn count_slot_diffs(mut healthy: PumpClient) -> (usize, Option<i64>) {
    let mut diffs = 0_usize;
    let mut last_version = None;
    while diffs < FLOOD_WRITES {
        match read_frame_within(
            &mut healthy.stream,
            Duration::from_secs(30),
            "healthy reader's diff",
        )
        .await
        {
            ServerFrame::Diff {
                field: FieldTarget::Slot { .. },
                version,
                ..
            } => {
                diffs += 1;
                last_version = Some(version);
            }
            ServerFrame::Ping => pump_pong(&healthy.sink).await,
            ServerFrame::Derived { .. }
            | ServerFrame::Diff { .. }
            | ServerFrame::Hello { .. }
            | ServerFrame::Snapshot { .. }
            | ServerFrame::Ack(_)
            | ServerFrame::Pong
            | ServerFrame::Bye { .. } => {}
        }
    }
    (diffs, last_version)
}

#[tokio::test]
async fn a_stalled_reader_is_evicted_1013_while_the_party_keeps_flowing() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, char_writer) = seed_member(&pool, "dev-sub-josh").await;
    let (party_bear, _) = seed_member(&pool, "dev-sub-bear").await;
    let (party_dave, _) = seed_member(&pool, "dev-sub-dave").await;
    assert!(
        party_id == party_bear && party_id == party_dave,
        "one POC party for all three members"
    );
    let slot_base = seed_spell_slot(&pool, char_writer).await;
    let session_w = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let session_h = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let session_s = testing::seed_session(&pool, "dev-sub-dave", chrono::Utc::now()).await;
    let settings = SyncSettings {
        ping_interval: Duration::from_secs(10),
        pong_timeout: Duration::from_secs(5),
    };
    let (addr, shutdown) = spawn_server_with(pool.clone(), settings).await;

    // The stall: connected, introduced, then never reads again.
    let mut stalled = connect(addr, party_id, &session_s).await;
    intro(&mut stalled, "stalled reader").await;

    let mut writer = connect(addr, party_id, &session_w).await;
    intro(&mut writer, "flood writer").await;
    let mut healthy = connect_pump(addr, party_id, &session_h).await;
    pump_intro(&mut healthy, "healthy reader").await;

    let healthy_task = tokio::spawn(count_slot_diffs(healthy));

    let final_version = flood_slot_writes(&mut writer, char_writer, slot_base).await;
    let (healthy_diffs, healthy_last) = healthy_task.await.expect("healthy reader joins");
    assert_eq!(
        healthy_diffs, FLOOD_WRITES,
        "the live reader missed nothing during the stall"
    );
    assert_eq!(
        healthy_last,
        Some(final_version),
        "the live reader's last version is the last applied write"
    );

    // Release the stall: the evicted reader drains its backlog and finds
    // the registry's verdict — close 1013.
    let code = read_until_close(
        &mut stalled.stream,
        Duration::from_secs(60),
        "stalled reader",
    )
    .await;
    assert_eq!(code, 1013, "the stalled reader is evicted with 1013");

    assert_eq!(
        slot_version(&pool, char_writer).await,
        final_version,
        "every flood write landed"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM client_ops")
        .fetch_one(&pool)
        .await
        .expect("ledger count");
    assert_eq!(
        rows,
        i64::try_from(FLOOD_WRITES).expect("flood count fits i64"),
        "every flood write is ledgered"
    );
    let strays: i64 =
        sqlx::query_scalar("SELECT count(*) FROM client_ops WHERE outcome <> 'applied'")
            .fetch_one(&pool)
            .await
            .expect("outcome audit");
    assert_eq!(strays, 0, "the flood applied cleanly");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "sync_stalled_reader_eviction").await;
}
