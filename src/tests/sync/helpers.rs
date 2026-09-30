//! Shared harness for the sync session tests (plan Task 6/13): real-router
//! spawn, real-socket clients, frame readers, and seed helpers. The PR #30
//! rule lives here — every session-level assertion rides a real socket.

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::auth::oidc;
use crate::sync::protocol::ServerFrame;
use crate::sync::{SyncSettings, SyncState};
use crate::testing;

/// One connected client's socket pair.
pub type WsSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// The client half of a session: split so tests can send and read
/// independently.
pub struct Client {
    pub sink: futures_util::stream::SplitSink<WsSocket, Message>,
    pub stream: futures_util::stream::SplitStream<WsSocket>,
}

/// Open the party socket with the given session cookie.
pub async fn connect(addr: SocketAddr, party_id: i64, session: &str) -> Client {
    match try_connect(addr, party_id, session).await {
        Ok(client) => client,
        Err(err) => panic!("socket upgrade failed: {err:?}"),
    }
}

/// Open the party socket, returning the raw connection error (the 403 path
/// asserts on it).
pub async fn try_connect(
    addr: SocketAddr,
    party_id: i64,
    session: &str,
) -> Result<Client, tokio_tungstenite::tungstenite::Error> {
    let mut request = format!("ws://{addr}/api/ws/party/{party_id}")
        .into_client_request()
        .expect("ws request builds");
    request.headers_mut().insert(
        "cookie",
        format!("{}={}", oidc::SESSION_COOKIE, session)
            .parse()
            .expect("cookie header"),
    );
    let (socket, _response) = tokio_tungstenite::connect_async(request).await?;
    let (sink, stream) = socket.split();
    Ok(Client { sink, stream })
}

/// Read the next text frame and decode it as a server frame.
pub async fn read_frame(
    stream: &mut futures_util::stream::SplitStream<WsSocket>,
    what: &str,
) -> ServerFrame {
    let message = read_message(stream, what).await;
    match message {
        Message::Text(text) => serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("server frame for {what} does not decode: {err}")),
        Message::Binary(bytes) => {
            panic!(
                "expected a text frame for {what}, got binary({} bytes)",
                bytes.len()
            )
        }
        Message::Ping(_) => panic!("expected a text frame for {what}, got a protocol ping"),
        Message::Pong(_) => panic!("expected a text frame for {what}, got a protocol pong"),
        Message::Close(frame) => panic!("expected a text frame for {what}, got close {frame:?}"),
        Message::Frame(raw) => panic!("expected a text frame for {what}, got raw frame {raw:?}"),
    }
}

/// Read the next frame and return its close code (panics on anything else).
pub async fn read_close(
    stream: &mut futures_util::stream::SplitStream<WsSocket>,
    what: &str,
) -> u16 {
    let message = read_message(stream, what).await;
    match message {
        Message::Close(Some(frame)) => frame.code.into(),
        Message::Close(None) => panic!("expected a coded close frame for {what}, got close(None)"),
        Message::Text(text) => panic!("expected a close frame for {what}, got text {text:?}"),
        Message::Binary(bytes) => {
            panic!(
                "expected a close frame for {what}, got binary({} bytes)",
                bytes.len()
            )
        }
        Message::Ping(_) => panic!("expected a close frame for {what}, got a protocol ping"),
        Message::Pong(_) => panic!("expected a close frame for {what}, got a protocol pong"),
        Message::Frame(raw) => panic!("expected a close frame for {what}, got raw frame {raw:?}"),
    }
}

/// The next raw message, bounded so a silent server cannot hang the suite.
async fn read_message(
    stream: &mut futures_util::stream::SplitStream<WsSocket>,
    what: &str,
) -> Message {
    tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
        .unwrap_or_else(|| panic!("socket closed waiting for {what}"))
        .unwrap_or_else(|err| panic!("socket error waiting for {what}: {err}"))
}

/// Send one raw text frame.
pub async fn send_raw(sink: &mut futures_util::stream::SplitSink<WsSocket, Message>, text: &str) {
    sink.send(Message::Text(text.into()))
        .await
        .expect("client send");
}

/// Spawn the real router on an ephemeral loopback port, with the caller's
/// sync settings and the production drain wiring: the oneshot stands in for
/// the E1 shutdown signal; on signal the drain watch flips and sessions
/// bye+close. Returns the address and the shutdown trigger.
pub async fn spawn_server_with(
    pool: sqlx::PgPool,
    settings: SyncSettings,
) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let (drain_tx, drain_rx) = tokio::sync::watch::channel(false);
    let sync = SyncState {
        registry: std::sync::Arc::new(crate::sync::registry::PartyRegistry::new()),
        settings,
        drain: drain_rx,
        metrics: std::sync::Arc::new(crate::sync::metrics::SyncMetrics::new()),
    };
    let app = testing::router_for_with_sync(pool, &testing::auth_settings(), sync);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind");
    let addr = listener.local_addr().expect("bound addr");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            // Fire or drop both stop the server — a dropped sender is teardown.
            shutdown_rx.await.ok();
            // The E1 drain path: the signal propagates to sessions via the
            // same watch channel `http::serve` uses.
            drain_tx.send_replace(true);
        });
        if let Err(err) = server.await {
            eprintln!("test server error: {err}");
        }
    });
    (addr, shutdown_tx)
}

/// Spawn with the binding default timers.
pub async fn spawn_server(pool: sqlx::PgPool) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
    spawn_server_with(pool, SyncSettings::default()).await
}

/// Seed account + character + vitals in the POC party; return
/// (`party_id`, `character_id`).
pub async fn seed_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
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

/// One vitals version column (test-literal column names — the
/// `AssertSqlSafe` audit is honest, same policy as src/tests/db.rs).
pub async fn vitals_version(pool: &sqlx::PgPool, character_id: i64, column: &str) -> i64 {
    let sql = format!("SELECT {column} FROM character_vitals WHERE character_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(character_id)
        .fetch_one(pool)
        .await
        .expect("vitals version")
}
