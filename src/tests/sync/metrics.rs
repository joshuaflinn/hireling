//! Tests for E7's dispatch metrics (plan Task 8): the ring histogram's
//! percentile math, the `/metrics/sync` endpoint's auth gate, and — per the
//! PR #30 house rule — the `sync_dispatch_ms` structured log driven through
//! the production path (a real write over a real socket).

use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use tower::ServiceExt as _;

use crate::sync::metrics::SyncMetrics;
use crate::sync::registry::PartyRegistry;
use crate::sync::test_helpers::{connect, read_frame, seed_member, send_raw};
use crate::sync::{SyncSettings, SyncState};
use crate::testing;

/// Percentiles are selected samples, so exact equality holds; spelled as a
/// difference for the float-comparison lint.
fn is_exact(got: f64, want: f64) -> bool {
    (got - want).abs() < f64::EPSILON
}

#[test]
fn the_percentiles_match_the_nearest_rank_definition() {
    let metrics = SyncMetrics::new();
    for ms in 1..=100 {
        metrics.push(f64::from(ms));
    }
    let summary = metrics.snapshot();
    assert_eq!(summary.count, 100);
    assert!(is_exact(summary.p50_ms, 50.0), "p50 of 1..=100 is 50");
    assert!(is_exact(summary.p95_ms, 95.0), "p95 of 1..=100 is 95");
    assert!(is_exact(summary.p99_ms, 99.0), "p99 of 1..=100 is 99");
}

#[test]
fn the_ring_holds_only_the_last_10_000_samples() {
    let metrics = SyncMetrics::new();
    for ms in 0..10_001_u64 {
        metrics.push(f64::from(u32::try_from(ms).expect("fits u32")));
    }
    let summary = metrics.snapshot();
    assert_eq!(summary.count, 10_000, "the ring is capped");
    assert!(
        is_exact(summary.p50_ms, 5_000.0),
        "the oldest sample (0) was evicted; p50 of 1..=10_000 is 5_000"
    );
}

#[test]
fn an_empty_ring_reports_zeroes() {
    let summary = SyncMetrics::new().snapshot();
    assert_eq!(summary.count, 0);
    assert_eq!(
        (summary.p50_ms, summary.p95_ms, summary.p99_ms),
        (0.0, 0.0, 0.0)
    );
}

/// The shared capture buffer behind the process-global tracing subscriber.
/// Only the first installing test wins the install; the buffer is shared so
/// every test asserts against the same stream of events.
fn captured_log() -> Arc<Mutex<Vec<String>>> {
    static BUF: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    Arc::clone(BUF.get_or_init(|| Arc::new(Mutex::new(Vec::new()))))
}

struct CaptureWriter(Arc<Mutex<Vec<String>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let line = String::from_utf8_lossy(buf).to_string();
        self.0.lock().expect("capture lock").push(line);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Install the capturing subscriber once per process. No-op if another test
/// installed a subscriber first — the shared buffer still captures.
fn install_log_capture() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let buffer = captured_log();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || CaptureWriter(Arc::clone(&buffer)))
            .with_max_level(tracing::Level::TRACE)
            .finish();
        if tracing::subscriber::set_global_default(subscriber).is_err() {
            // Another test installed a subscriber first; the shared buffer
            // still captures whatever the global subscriber writes.
        }
    });
}

#[tokio::test]
async fn the_endpoint_gates_and_a_real_write_feeds_the_metrics_and_the_log() {
    install_log_capture();
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let (app, addr, shutdown) = spawn_shared_server(pool.clone()).await;

    // Unauthenticated: the standard 401, no metrics leak.
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/metrics/sync")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);

    // Authenticated: the contract §7 shape.
    let cookie = format!("__Host-hireling_session={session}");

    let before = read_summary(&app, &cookie).await;
    assert_eq!(
        before,
        json!({"count": 0, "p50_ms": 0.0, "p95_ms": 0.0, "p99_ms": 0.0}),
        "a fresh process has dispatched nothing: {before}"
    );

    // One real write over a real socket: count moves, the log gains the
    // structured event (production path — the recorder is never called
    // directly). The socket server and the endpoint reads share ONE app,
    // hence one metrics ring.
    let mut client = connect(addr, party_id, &session).await;
    read_frame(&mut client.stream, "hello").await;
    read_frame(&mut client.stream, "snapshot").await;

    let base: i64 =
        sqlx::query_scalar("SELECT hp_version FROM character_vitals WHERE character_id = $1")
            .bind(character_id)
            .fetch_one(&pool)
            .await
            .expect("hp version");
    let write = json!({
        "t": "write",
        "op_id": "op-metrics",
        "target": {"kind": "vitals", "character_id": character_id, "field": "hp"},
        "base_version": base,
        "value": 9
    });
    send_raw(&mut client.sink, &write.to_string()).await;
    let ack = read_frame(&mut client.stream, "ack").await;
    assert!(
        matches!(&ack, crate::sync::protocol::ServerFrame::Ack(a)
            if a.outcome == crate::sync::protocol::Outcome::Applied),
        "the write applies: {ack:?}"
    );

    let after = read_summary(&app, &cookie).await;
    assert_eq!(
        after.get("count"),
        Some(&json!(1)),
        "exactly the one applied write is in the window: {after}"
    );
    assert!(
        after
            .get("p50_ms")
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|ms| ms >= 0.0),
        "percentiles are reported: {after}"
    );

    let log = captured_log().lock().expect("capture lock").join("\n");
    assert!(
        log.contains("sync_dispatch_ms"),
        "the structured dispatch event is in the production log: {log}"
    );
    assert!(
        log.contains("op-metrics") && log.contains("vitals:hp"),
        "the event carries op_id and field: {log}"
    );

    assert!(
        shutdown.send(()).is_ok(),
        "the test server died before shutdown"
    );
    testing::drop_test_db(pool, "metrics_endpoint").await;
}

/// GET /metrics/sync through the real router with the session cookie; the
/// decoded summary JSON.
async fn read_summary(app: &axum::Router, cookie: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/metrics/sync")
                .header("cookie", cookie)
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&body).expect("summary json")
}

/// Spawn the router once and hand the test its clone: socket traffic and
/// oneshot endpoint reads share the same `SyncState` (same metrics ring).
async fn spawn_shared_server(
    pool: sqlx::PgPool,
) -> (
    axum::Router,
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
) {
    use std::net::SocketAddr;
    let (drain_tx, drain_rx) = tokio::sync::watch::channel(false);
    let sync = SyncState {
        registry: Arc::new(PartyRegistry::new()),
        settings: SyncSettings::default(),
        drain: drain_rx,
        metrics: Arc::new(SyncMetrics::new()),
    };
    let app = testing::router_for_with_sync(pool, &testing::auth_settings(), sync);
    let served = app.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind");
    let addr = listener.local_addr().expect("bound addr");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let running = axum::serve(
            listener,
            served.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            shutdown_rx.await.ok();
            drain_tx.send_replace(true);
        });
        if let Err(err) = running.await {
            eprintln!("test server error: {err}");
        }
    });
    (app, addr, shutdown_tx)
}
