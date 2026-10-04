//! Integration tests for the E7 WebSocket session (plan Tasks 6/7): real
//! router, real listener, real sockets — the PR #30 house rule. Handshake
//! authz, the hello→snapshot sequence, write→ack/diff fan-out, and
//! deny-by-default frame handling; liveness timers and the E1 drain.

use std::time::Duration;

use futures_util::SinkExt as _;
use futures_util::StreamExt as _;
use serde_json::json;
use socket2::SockRef;
use tokio::net::TcpStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::auth::oidc;
use crate::sync::SyncSettings;
use crate::sync::protocol::{Outcome, ServerFrame, VitalsField};
use crate::sync::test_helpers::{
    connect, read_close, read_frame, seed_member, send_raw, spawn_server, spawn_server_with,
    try_connect, vitals_version,
};
use crate::testing;

#[tokio::test]
async fn a_member_connects_and_receives_hello_then_snapshot() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    // Probe: the same cookie must authenticate a plain GET over this real
    // listener — isolates WS-client problems from auth-chain problems.
    let probe = reqwest::Client::new()
        .get(format!("http://{addr}/api/me"))
        .header("cookie", format!("{}={}", oidc::SESSION_COOKIE, session))
        .send()
        .await
        .expect("probe request");
    assert_eq!(
        probe.status(),
        axum::http::StatusCode::OK,
        "cookie must authenticate a plain GET over the real socket"
    );

    let mut client = connect(addr, party_id, &session).await;

    let hello = read_frame(&mut client.stream, "hello").await;
    let ServerFrame::Hello {
        party_id: hello_party,
        you,
        server_now: _,
    } = hello
    else {
        panic!("first frame must be hello, got {hello:?}")
    };
    assert_eq!(hello_party, party_id);
    assert_eq!(you.sub, "dev-sub-josh");
    assert_eq!(you.role, "player");

    let snapshot = read_frame(&mut client.stream, "snapshot").await;
    let ServerFrame::Snapshot {
        fields,
        snapshot_bytes,
        derived,
    } = snapshot
    else {
        panic!("second frame must be snapshot, got {snapshot:?}")
    };
    assert_eq!(fields.len(), 7, "one character, seven vitals fields");
    assert_eq!(
        derived.len(),
        1,
        "one derived array entry per roster character"
    );
    assert!(snapshot_bytes > 0, "the metric is carried");
    let hp = fields
        .iter()
        .find(|row| {
            matches!(
                row.field,
                crate::sync::protocol::FieldTarget::Vitals {
                    field: VitalsField::Hp,
                    ..
                }
            )
        })
        .expect("hp row in snapshot");
    assert_eq!(
        hp.version,
        vitals_version(&pool, character_id, "hp_version").await,
        "the snapshot version is the database's"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_hello_snapshot").await;
}

#[tokio::test]
async fn an_outsider_is_refused_before_any_socket_opens() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-dave").await;
    // An account with a session but no character in the party.
    testing::seed_account(&pool, "dev-sub-becky", "player").await;
    let session = testing::seed_session(&pool, "dev-sub-becky", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let Err(error) = try_connect(addr, party_id, &session).await else {
        panic!("an outsider must not upgrade");
    };
    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("expected an HTTP rejection, got {error:?}")
    };
    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_outsider_403").await;
}

#[tokio::test]
async fn the_gm_connects_read_only() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-dave").await;
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut client = connect(addr, party_id, &session).await;
    let hello = read_frame(&mut client.stream, "gm hello").await;
    let ServerFrame::Hello { you, .. } = hello else {
        panic!("first frame must be hello, got {hello:?}")
    };
    assert_eq!(you.role, "gm", "the GM seat greets as gm");
    let snapshot = read_frame(&mut client.stream, "gm snapshot").await;
    assert!(matches!(snapshot, ServerFrame::Snapshot { .. }));

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_gm_connect").await;
}

/// The applied-write `diff` shape both sockets must see.
fn diff_matches(frame: &ServerFrame, character_id: i64, version: i64) -> bool {
    matches!(
        frame,
        ServerFrame::Diff {
            field: crate::sync::protocol::FieldTarget::Vitals {
                character_id: cid,
                field: VitalsField::Hp,
            },
            value,
            version: seen,
            actor_sub,
            op_id: Some(op),
        } if *cid == character_id
            && value == &json!(14)
            && *seen == version
            && actor_sub == "dev-sub-josh"
            && op == "op-ws-1"
    )
}

#[tokio::test]
async fn an_applied_write_acks_the_writer_and_diffs_every_socket() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let second = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_id, &session).await;
    let mut watcher = connect(addr, party_id, &second).await;
    // Drain both handshakes so the assertions below see only write traffic.
    read_frame(&mut writer.stream, "writer hello").await;
    read_frame(&mut writer.stream, "writer snapshot").await;
    read_frame(&mut watcher.stream, "watcher hello").await;
    read_frame(&mut watcher.stream, "watcher snapshot").await;

    let base = vitals_version(&pool, character_id, "hp_version").await;
    let write = json!({
        "t": "write",
        "op_id": "op-ws-1",
        "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
        "base_version": base,
        "value": 14
    });
    send_raw(&mut writer.sink, &write.to_string()).await;

    let ack = read_frame(&mut writer.stream, "ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("writer expects an ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Applied);
    let version = ack.version.expect("applied acks carry the version");
    assert!(version > base);

    let own_diff = read_frame(&mut writer.stream, "writer's own diff").await;
    assert!(
        diff_matches(&own_diff, character_id, version),
        "the writer sees its own diff: {own_diff:?}"
    );
    let watcher_diff = read_frame(&mut watcher.stream, "watcher diff").await;
    assert!(
        diff_matches(&watcher_diff, character_id, version),
        "the second socket sees the diff: {watcher_diff:?}"
    );
    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        version,
        "the row carries the diffed version"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_write_ack_diff").await;
}

#[tokio::test]
async fn a_gm_write_is_forbidden_and_diffs_nothing() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut owner = connect(addr, party_id, &session).await;
    let mut gm = connect(addr, party_id, &gm_session).await;
    read_frame(&mut owner.stream, "owner hello").await;
    read_frame(&mut owner.stream, "owner snapshot").await;
    read_frame(&mut gm.stream, "gm hello").await;
    read_frame(&mut gm.stream, "gm snapshot").await;

    let base = vitals_version(&pool, character_id, "hp_version").await;
    let write = json!({
        "t": "write",
        "op_id": "op-gm-ws",
        "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
        "base_version": base,
        "value": 5
    });
    send_raw(&mut gm.sink, &write.to_string()).await;

    let ack = read_frame(&mut gm.stream, "gm ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("gm expects an ack, got {ack:?}")
    };
    assert_eq!(ack.outcome, Outcome::Forbidden);
    assert!(
        ack.reason
            .as_deref()
            .is_some_and(|r| r.contains("read-only")),
        "the reason names the gm seat: {:?}",
        ack.reason
    );

    // No diff may precede the owner's pong: the forbidden write fanned out
    // nothing (contract §4 — no diff for forbidden).
    send_raw(&mut owner.sink, r#"{"t":"ping"}"#).await;
    let owner_next = read_frame(&mut owner.stream, "owner silence probe").await;
    assert!(
        matches!(owner_next, ServerFrame::Pong),
        "no diff reached the owner after a forbidden write, got {owner_next:?}"
    );
    assert_eq!(
        vitals_version(&pool, character_id, "hp_version").await,
        base,
        "the gm write touched nothing"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_gm_forbidden").await;
}

#[tokio::test]
async fn garbage_is_dropped_but_effect_writes_reach_the_engine_and_reject() {
    // E7's deny-by-default for garbage frames stands; E8 realized the
    // effect extension (wire-protocol.md §8), so an effect write now
    // REACHES the write engine — this one is bounds-rejected with a
    // durable ledger row, and the ack names why. The connection lives.
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut client = connect(addr, party_id, &session).await;
    read_frame(&mut client.stream, "hello").await;
    read_frame(&mut client.stream, "snapshot").await;

    send_raw(&mut client.sink, "this is not json").await;
    let malformed_effect_write = json!({
        "t": "write",
        "op_id": "op-effect-ws",
        "target": {"kind": "effect", "effect_id": 7},
        "base_version": 1,
        "value": {"active": false}
    });
    send_raw(&mut client.sink, &malformed_effect_write.to_string()).await;

    let ack = read_frame(&mut client.stream, "ack for the effect write").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("the malformed effect write must be acked, got {ack:?}")
    };
    assert_eq!(ack.op_id, "op-effect-ws");
    assert_eq!(ack.outcome, Outcome::Rejected);
    assert!(
        ack.reason
            .clone()
            .is_some_and(|reason| reason.contains("op")),
        "the rejection names the problem: {ack:?}"
    );

    // The connection stayed open: the next ping is answered.
    send_raw(&mut client.sink, r#"{"t":"ping"}"#).await;
    let pong = read_frame(&mut client.stream, "pong after garbage").await;
    assert!(matches!(pong, ServerFrame::Pong));

    // The ledger holds exactly the effect rejection; the garbage frame was
    // dropped before any handler (no row, no ack).
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM client_ops")
        .fetch_one(&pool)
        .await
        .expect("ledger count");
    assert_eq!(
        rows, 1,
        "garbage dropped; the effect write recorded its rejection"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    let _ = character_id;
    testing::drop_test_db(pool, "ws_garbage_ignored").await;
}

// --- liveness + drain (plan Task 7) ---------------------------------------

#[tokio::test]
async fn the_server_pings_and_a_silent_client_is_closed_1011() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let settings = SyncSettings {
        ping_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_millis(50),
    };
    let (addr, shutdown) = spawn_server_with(pool.clone(), settings).await;

    let mut client = connect(addr, party_id, &session).await;
    read_frame(&mut client.stream, "hello").await;
    read_frame(&mut client.stream, "snapshot").await;

    let ping = read_frame(&mut client.stream, "liveness ping").await;
    assert!(
        matches!(ping, ServerFrame::Ping),
        "the server pings at the interval, got {ping:?}"
    );

    // No pong follows: the server closes 1011 within its timeout window.
    let code = read_close(&mut client.stream, "pong-timeout close").await;
    assert_eq!(code, 1011, "a silent client is closed with 1011");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_liveness_timeout").await;
}

#[tokio::test]
async fn a_ponging_client_stays_connected_across_cycles() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let settings = SyncSettings {
        ping_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_millis(50),
    };
    let (addr, shutdown) = spawn_server_with(pool.clone(), settings).await;

    let mut client = connect(addr, party_id, &session).await;
    read_frame(&mut client.stream, "hello").await;
    read_frame(&mut client.stream, "snapshot").await;

    // Three full ping/pong cycles — the connection must survive them all.
    for cycle in 0..3 {
        let ping = read_frame(&mut client.stream, "liveness ping").await;
        assert!(matches!(ping, ServerFrame::Ping), "cycle {cycle}");
        send_raw(&mut client.sink, r#"{"t":"pong"}"#).await;
    }

    // Still alive: an application ping is answered, and a write still works.
    send_raw(&mut client.sink, r#"{"t":"ping"}"#).await;
    let pong = read_frame(&mut client.stream, "app ping answer").await;
    assert!(matches!(pong, ServerFrame::Pong));

    let base = vitals_version(&pool, character_id, "hp_version").await;
    let write = json!({
        "t": "write",
        "op_id": "op-after-cycles",
        "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
        "base_version": base,
        "value": 7
    });
    send_raw(&mut client.sink, &write.to_string()).await;
    let ack = read_frame(&mut client.stream, "ack after cycles").await;
    assert!(
        matches!(&ack, ServerFrame::Ack(a) if a.outcome == Outcome::Applied),
        "the connection still writes after surviving the cycles: {ack:?}"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_liveness_healthy").await;
}

#[tokio::test]
async fn the_drain_signal_sends_bye_then_close_1001() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut client = connect(addr, party_id, &session).await;
    read_frame(&mut client.stream, "hello").await;
    read_frame(&mut client.stream, "snapshot").await;

    // Fire the E1 signal the way production does: the oneshot flips the
    // drain watch; sessions must say bye, then close 1001 (contract §2/§6).
    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );

    let bye = read_frame(&mut client.stream, "bye").await;
    assert!(
        matches!(&bye, ServerFrame::Bye { reason } if reason == "shutdown"),
        "the drain announces itself with bye, got {bye:?}"
    );
    let code = read_close(&mut client.stream, "drain close").await;
    assert_eq!(code, 1001, "drain closes with 1001");

    testing::drop_test_db(pool, "ws_drain_bye_1001").await;
}

/// The writer's socket is closed with `SO_LINGER` 0, so the kernel sends RST
/// the moment the client vanishes — the server's ack write then fails the
/// way a peer that died mid-request fails in production. The write is
/// already committed: the party must still hear about it (spec FR-3) — a
/// fan-out suppressed by the ack failure strands every other client until
/// reconnect (review finding, PR #34).
#[tokio::test]
async fn a_committed_write_fans_out_even_when_the_writer_vanishes_before_the_ack() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, writer_char) = seed_member(&pool, "dev-sub-josh").await;
    seed_member(&pool, "dev-sub-becky").await;
    let writer_session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let witness_session = testing::seed_session(&pool, "dev-sub-becky", chrono::Utc::now()).await;
    let base = vitals_version(&pool, writer_char, "hp_version").await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    // The witness stays connected; it is the one that must not be starved.
    let mut witness = connect(addr, party_id, &witness_session).await;
    read_frame(&mut witness.stream, "witness hello").await;
    read_frame(&mut witness.stream, "witness snapshot").await;

    // The writer: a raw socket held at SO_LINGER 0, upgraded, never read.
    let tcp = TcpStream::connect(addr).await.expect("writer tcp connect");
    SockRef::from(&tcp)
        .set_linger(Some(std::time::Duration::ZERO))
        .expect("SO_LINGER 0");
    let mut request = format!("ws://{addr}/api/ws/party/{party_id}")
        .into_client_request()
        .expect("writer ws request builds");
    request.headers_mut().insert(
        "cookie",
        format!("{}={}", oidc::SESSION_COOKIE, writer_session)
            .parse()
            .expect("cookie header"),
    );
    let (ws, _response) = client_async(request, tcp).await.expect("writer upgrade");
    let (mut writer_sink, writer_stream) = ws.split();

    // The server's intro (hello + snapshot) is written before it reads —
    // the write frame must leave only after those writes have landed, or
    // the RST kills the session before the frame is ever processed. A beat
    // of sleep gives the server the time; the RST on drop does the rest.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The write frame leaves; the ack is never read.
    writer_sink
        .send(Message::Text(
            json!({
                "t": "write",
                "op_id": "op-vanish",
                "target": {"kind": "vitals", "character_id": writer_char, "field": "hp"},
                "base_version": base,
                "value": 77
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("writer sends the write frame");
    drop(writer_sink);
    drop(writer_stream);

    // The commit happened server-side; the witness must receive the diff
    // even though the ack went to a dead socket.
    let diff = read_frame(
        &mut witness.stream,
        "fan-out diff after the writer vanished",
    )
    .await;
    let ServerFrame::Diff {
        field,
        value,
        version,
        actor_sub,
        op_id,
    } = diff
    else {
        panic!("the witness must receive the committed write as a diff, got {diff:?}")
    };
    assert_eq!(
        op_id.as_deref(),
        Some("op-vanish"),
        "the vanished writer's op"
    );
    assert_eq!(value, json!(77), "the committed value");
    assert_eq!(actor_sub, "dev-sub-josh", "the vanished writer owns it");
    assert!(version > base, "strictly newer than base");
    assert!(matches!(
        field,
        crate::sync::protocol::FieldTarget::Vitals {
            field: VitalsField::Hp,
            ..
        }
    ));

    // The write is durable: the commit preceded the RST.
    let hp: i32 = sqlx::query_scalar("SELECT hp FROM character_vitals WHERE character_id = $1")
        .bind(writer_char)
        .fetch_one(&pool)
        .await
        .expect("vitals row");
    assert_eq!(hp, 77, "the write committed before the writer vanished");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_vanished_writer_fanout").await;
}

/// Party A's socket, party B's character: the one-party-per-socket
/// invariant (contract §1) denies the write before ownership is even
/// asked, no diff reaches either party, and the target row is untouched
/// (review finding, PR #34).
#[tokio::test]
async fn a_write_targeting_another_party_s_character_is_denied_and_fans_out_nothing() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // The writer and the witness live in the POC party (A).
    let (party_a, _) = seed_member(&pool, "dev-sub-josh").await;
    seed_member(&pool, "dev-sub-becky").await;
    // A second party (B) with its own member and character.
    testing::seed_account(&pool, "dev-sub-dave", "player").await;
    let party_b: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('forge-second') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");
    let char_b: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, 'dev-sub-dave', '{}', '{}') RETURNING id",
    )
    .bind(party_b)
    .fetch_one(&pool)
    .await
    .expect("party-B character");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(char_b)
        .execute(&pool)
        .await
        .expect("party-B vitals");
    let (base_b, hp_b): (i64, i32) =
        sqlx::query_as("SELECT hp_version, hp FROM character_vitals WHERE character_id = $1")
            .bind(char_b)
            .fetch_one(&pool)
            .await
            .expect("party-B vitals row");

    let writer_session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let same_party_witness_session =
        testing::seed_session(&pool, "dev-sub-becky", chrono::Utc::now()).await;
    let other_party_witness_session =
        testing::seed_session(&pool, "dev-sub-dave", chrono::Utc::now()).await;
    let (addr, shutdown) = spawn_server(pool.clone()).await;

    let mut writer = connect(addr, party_a, &writer_session).await;
    read_frame(&mut writer.stream, "writer hello").await;
    read_frame(&mut writer.stream, "writer snapshot").await;
    let mut same_party_witness = connect(addr, party_a, &same_party_witness_session).await;
    read_frame(&mut same_party_witness.stream, "A hello").await;
    read_frame(&mut same_party_witness.stream, "A snapshot").await;
    let mut other_party_witness = connect(addr, party_b, &other_party_witness_session).await;
    read_frame(&mut other_party_witness.stream, "B hello").await;
    read_frame(&mut other_party_witness.stream, "B snapshot").await;

    send_raw(
        &mut writer.sink,
        &json!({
            "t": "write",
            "op_id": "op-cross-party",
            "target": {"kind": "vitals", "character_id": char_b, "field": "hp"},
            "base_version": base_b,
            "value": 33
        })
        .to_string(),
    )
    .await;

    let ack = read_frame(&mut writer.stream, "cross-party ack").await;
    let ServerFrame::Ack(ack) = ack else {
        panic!("the cross-party write must be acked, got {ack:?}")
    };
    assert_eq!(ack.op_id, "op-cross-party");
    assert_eq!(
        ack.outcome,
        Outcome::Forbidden,
        "cross-party writes are denied"
    );
    assert!(
        ack.reason
            .as_deref()
            .is_some_and(|reason| reason.contains("party")),
        "the denial names the party scope, got {:?}",
        ack.reason
    );

    // Neither party sees a diff — no wrong fan-out in either direction.
    for (who, stream) in [
        ("the writer's party", &mut same_party_witness.stream),
        ("the target's party", &mut other_party_witness.stream),
    ] {
        let quiet = tokio::time::timeout(Duration::from_millis(300), stream.next()).await;
        assert!(
            quiet.is_err(),
            "{who} must receive no diff for a cross-party write, got {quiet:?}"
        );
    }

    // Party B's character is untouched.
    let (hp, version): (i32, i64) =
        sqlx::query_as("SELECT hp, hp_version FROM character_vitals WHERE character_id = $1")
            .bind(char_b)
            .fetch_one(&pool)
            .await
            .expect("party-B vitals row");
    assert_eq!(hp, hp_b, "the cross-party write touched nothing");
    assert_eq!(version, base_b, "no version bump on the target");

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "ws_cross_party_denied").await;
}
