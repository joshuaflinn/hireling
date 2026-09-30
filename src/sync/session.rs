//! The party WebSocket session — E7 (plan Task 6).
//!
//! One authenticated socket per party (contract §1): handshake authz
//! (owner-in-party or GM, deny-by-default) runs before the upgrade; the
//! session answers `ping`, applies writes through the CAS engine (E3's
//! `authorize()` per frame), acks the writer, and broadcasts diffs through
//! the party registry. Sequence after upgrade: `hello` → `snapshot` (§5).
//! An evicted connection — outbound buffer overflow — is closed 1013; the
//! client recovers via reconnect + snapshot.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Path;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use futures_util::SinkExt;
use futures_util::stream::{SplitSink, StreamExt};
use serde_json::Value as JsonValue;

use crate::auth::AuthState;
use crate::auth::authz::{Actor, Role};
use crate::auth::middleware::SessionAccount;
use crate::sync::protocol::{Ack, ActorInfo, ClientFrame, FieldTarget, Outcome, ServerFrame};
use crate::sync::registry::PartyRegistry;
use crate::sync::write::{ClientOp, apply_write};

/// The session's write half.
type Sink = SplitSink<WebSocket, Message>;

/// `GET /api/ws/party/{party_id}` — the upgrade handler.
///
/// Handshake authorization (contract §1) runs BEFORE the upgrade: the
/// account must own a character in the party or hold the GM seat;
/// anything else is 403 and no socket ever opens.
pub async fn party_ws(
    ws: WebSocketUpgrade,
    State(auth): State<Arc<AuthState>>,
    Extension(sync): Extension<crate::sync::SyncState>,
    account: SessionAccount,
    Path(party_id): Path<i64>,
) -> Response {
    if !handshake_allowed(&auth.pool, &account, party_id).await {
        return (StatusCode::FORBIDDEN, "not a member of this party").into_response();
    }
    let pool = auth.pool.clone();
    ws.on_upgrade(move |socket| async move {
        run_session(pool, sync, party_id, account, socket).await;
    })
}

/// Handshake authz: the GM (with an existing party) or the owner of a
/// character in the party. Deny-by-default: an unknown party has no GM and
/// no members, and a database failure denies rather than admits.
async fn handshake_allowed(pool: &sqlx::PgPool, account: &SessionAccount, party_id: i64) -> bool {
    if account.role == Role::Gm {
        return match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM parties WHERE id = $1)")
            .bind(party_id)
            .fetch_one(pool)
            .await
        {
            Ok(exists) => exists,
            Err(err) => {
                tracing::error!(error = %err, party_id, "gm handshake lookup failed; denying");
                false
            }
        };
    }
    match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM characters WHERE party_id = $1 AND owner_sub = $2)",
    )
    .bind(party_id)
    .bind(&account.sub)
    .fetch_one(pool)
    .await
    {
        Ok(exists) => exists,
        Err(err) => {
            tracing::error!(error = %err, party_id, "handshake lookup failed; denying");
            false
        }
    }
}

/// The post-upgrade session: intro frames, then the select loop until the
/// socket or the registry connection ends. Always unsubscribes on exit.
async fn run_session(
    pool: sqlx::PgPool,
    sync: crate::sync::SyncState,
    party_id: i64,
    actor: SessionAccount,
    socket: WebSocket,
) {
    let registry = &sync.registry;
    let settings = sync.settings;
    let mut drain = sync.drain.clone();
    let (mut sink, mut stream) = socket.split();

    // Subscribe BEFORE the snapshot: a write committing during the snapshot
    // read lands both in this connection's buffer and in the snapshot; the
    // client's strictly-newer merge dedupes. Subscribing after would lose
    // that write forever.
    let handle = registry.subscribe(party_id);
    let mut rx = handle.rx;

    if !send_intro(&mut sink, &pool, party_id, &actor).await {
        registry.unsubscribe(party_id, handle.id);
        return;
    }

    let ctx = SessionCtx {
        pool: &pool,
        registry,
        party_id,
        actor: &actor,
    };

    // Liveness (contract §5): a protocol-independent app-level ping every
    // `ping_interval`; the connection closes 1011 when no `pong` arrives
    // within `pong_timeout` of the ping.
    let (mut pings, mut checks) = liveness_ticks(settings).await;
    let mut awaiting_pong: Option<tokio::time::Instant> = None;

    loop {
        tokio::select! {
            inbound = stream.next() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        handle_inbound(&mut sink, &ctx, &mut awaiting_pong, &text).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    // Protocol-level ping/pong/binary: tungstenite answers
                    // pings; nothing for the session to do.
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::warn!(error = %err, party_id, "socket error; closing session");
                        break;
                    }
                }
            }
            outbound = rx.recv() => {
                if let Some(frame) = outbound {
                    if !send_frame(&mut sink, &frame).await {
                        break;
                    }
                } else {
                    // Evicted for a full outbound buffer (the registry
                    // dropped our sender): close 1013, client reconnects.
                    let close = Message::Close(Some(CloseFrame {
                        code: 1013,
                        reason: "outbound buffer overflow".into(),
                    }));
                    if sink.send(close).await.is_err() {
                        tracing::debug!(party_id, "1013 close lost to a dead socket");
                    }
                    break;
                }
            }
            _ = pings.tick() => {
                if awaiting_pong.is_none() {
                    if send_frame(&mut sink, &ServerFrame::Ping).await {
                        awaiting_pong = Some(tokio::time::Instant::now());
                    } else {
                        break;
                    }
                }
            }
            _ = checks.tick() => {
                if let Some(since) = awaiting_pong
                    && since.elapsed() >= settings.pong_timeout
                {
                    tracing::info!(party_id, "pong timeout; closing 1011");
                    let close = Message::Close(Some(CloseFrame {
                        code: 1011,
                        reason: "pong timeout".into(),
                    }));
                    if sink.send(close).await.is_err() {
                        tracing::debug!(party_id, "1011 close lost to a dead socket");
                    }
                    break;
                }
            }
            _ = drain.changed() => {
                if *drain.borrow() {
                    // E1 drain (contract §2/§6): announce, then close 1001.
                    let bye = ServerFrame::Bye {
                        reason: "shutdown".to_owned(),
                    };
                    if !send_frame(&mut sink, &bye).await {
                        tracing::debug!(party_id, "bye lost to a dead socket");
                    }
                    let close = Message::Close(Some(CloseFrame {
                        code: 1001,
                        reason: "draining".into(),
                    }));
                    if sink.send(close).await.is_err() {
                        tracing::debug!(party_id, "1001 close lost to a dead socket");
                    }
                    break;
                }
            }
        }
    }

    registry.unsubscribe(party_id, handle.id);
}

/// The liveness tickers, first (immediate) tick consumed so the first ping
/// lands one full interval after the intro sequence.
async fn liveness_ticks(
    settings: crate::sync::SyncSettings,
) -> (tokio::time::Interval, tokio::time::Interval) {
    let mut pings = tokio::time::interval(settings.ping_interval);
    pings.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    pings.tick().await;
    let mut checks = tokio::time::interval(settings.pong_timeout);
    checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    checks.tick().await;
    (pings, checks)
}

/// `hello` then `snapshot` — the mandatory intro sequence (contract §5).
/// `false` means the connection could not be introduced and must close.
async fn send_intro(
    sink: &mut Sink,
    pool: &sqlx::PgPool,
    party_id: i64,
    actor: &SessionAccount,
) -> bool {
    let hello = ServerFrame::Hello {
        party_id,
        you: ActorInfo {
            sub: actor.sub.clone(),
            role: actor.role.as_str().to_owned(),
        },
        server_now: Utc::now(),
    };
    if !send_frame(sink, &hello).await {
        return false;
    }

    let fields = match crate::sync::snapshot::party_snapshot(pool, party_id).await {
        Ok(fields) => fields,
        Err(err) => {
            tracing::error!(error = %err, party_id, "snapshot failed; closing session");
            return false;
        }
    };
    let bytes = match crate::sync::snapshot::snapshot_bytes(&fields) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::error!(error = %err, party_id, "snapshot serialization failed");
            return false;
        }
    };
    // Contract §7: one structured log per snapshot, carrying its size.
    tracing::info!(party_id, snapshot_bytes = bytes, "snapshot sent");
    send_frame(
        sink,
        &ServerFrame::Snapshot {
            fields,
            snapshot_bytes: bytes,
        },
    )
    .await
}

/// Everything a frame handler needs about the connection.
struct SessionCtx<'a> {
    pool: &'a sqlx::PgPool,
    registry: &'a PartyRegistry,
    party_id: i64,
    actor: &'a SessionAccount,
}

/// One inbound text frame. Deny-by-default: malformed JSON, unknown frame
/// types, and effect writes (E8's) are dropped with a structured log and
/// the connection stays open (contract §2). A `pong` clears the liveness
/// wait; a `ping` is answered.
async fn handle_inbound(
    sink: &mut Sink,
    ctx: &SessionCtx<'_>,
    awaiting_pong: &mut Option<tokio::time::Instant>,
    text: &str,
) {
    match ClientFrame::decode(text) {
        Ok(ClientFrame::Pong) => {
            *awaiting_pong = None;
        }
        Ok(ClientFrame::Ping) => {
            send_frame(sink, &ServerFrame::Pong).await;
        }
        Ok(ClientFrame::Write {
            op_id,
            target,
            base_version,
            value,
        }) => {
            handle_write(sink, ctx, op_id, target, base_version, value).await;
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                party_id = ctx.party_id,
                actor = %ctx.actor.sub,
                "dropped frame"
            );
        }
    }
}

/// Apply one write and answer it: the ack goes to the writer's socket; an
/// applied write fans the diff out to the whole party — the writer
/// included, via the registry. Per-message authz happens inside
/// `apply_write` through E3's `authorize()` — the same matrix as REST,
/// one call per frame (spec FR-1).
async fn handle_write(
    sink: &mut Sink,
    ctx: &SessionCtx<'_>,
    op_id: String,
    target: FieldTarget,
    base_version: i64,
    value: JsonValue,
) {
    let party_id = ctx.party_id;
    let actor = ctx.actor;
    let writer = Actor {
        sub: actor.sub.clone(),
        role: actor.role,
    };
    let op = ClientOp {
        op_id: op_id.clone(),
        target: target.clone(),
        base_version,
        value: value.clone(),
    };
    let result = match apply_write(ctx.pool, &writer, op).await {
        Ok(result) => result,
        Err(err) => {
            // Database failure: no ack. The client's queue retries per
            // degraded-mode; the ledger makes the replay exactly-once.
            tracing::error!(error = %err, party_id, op_id, "write failed server-side");
            return;
        }
    };

    let ack = Ack {
        op_id: op_id.clone(),
        outcome: result.outcome,
        version: result.version,
        winning_version: result.winning_version,
        reason: result.reason,
    };
    if !send_frame(sink, &ServerFrame::Ack(ack)).await {
        return;
    }
    if result.outcome == Outcome::Applied
        && let Some(version) = result.version
    {
        ctx.registry.broadcast(
            party_id,
            &ServerFrame::Diff {
                field: target,
                value,
                version,
                actor_sub: actor.sub.clone(),
                op_id: Some(op_id),
            },
        );
    }
}

/// Encode and send one server frame. `false` means the connection is done
/// (socket closed or frame unencodable) and the caller must close.
async fn send_frame(sink: &mut Sink, frame: &ServerFrame) -> bool {
    match serde_json::to_string(frame) {
        Ok(text) => sink.send(Message::Text(text.into())).await.is_ok(),
        Err(err) => {
            tracing::error!(error = %err, "frame serialization failed");
            false
        }
    }
}
