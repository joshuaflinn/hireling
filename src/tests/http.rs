//! Tests for [`super`] — the health route and the SPA fallback, exercised
//! in-process through the router. The server loop itself (bind, signals) is
//! verified manually.

use std::io::Write as _;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use super::router;

/// A static dir with one shell page, shared by the fallback tests.
fn static_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hireling-http-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut index = std::fs::File::create(dir.join("index.html")).unwrap();
    index.write_all(b"<h1>hireling</h1>").unwrap();
    dir
}

#[tokio::test]
async fn healthz_returns_200_with_the_version_payload() {
    let response = router(&static_dir())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let response = router(&static_dir())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.headers().contains_key("x-request-id"),
        "a request without a correlation ID should get one generated"
    );
}

#[tokio::test]
async fn a_caller_supplied_request_id_is_propagated() {
    let response = router(&static_dir())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "test-correlation-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "test-correlation-123",
        "a caller-supplied correlation ID should come back on the response"
    );
}

#[tokio::test]
async fn unknown_paths_get_the_shell_page() {
    let response = router(&static_dir())
        .oneshot(
            Request::builder()
                .uri("/some/frontend/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unknown frontend paths should serve the shell page, not a 404"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"<h1>hireling</h1>");
}
