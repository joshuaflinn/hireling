//! Integration tests for the import HTTP surface (FR-15/FR-17): the full
//! route through the router — auth layers, ownership matrix membership, and
//! the exact contract §4 payloads over the wire.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use serde_json::Value;
use tower::ServiceExt as _;

use crate::auth::error::{Forbidden, Unauthenticated};
use crate::auth::oidc;
use crate::pbimport::error::ImportError;
use crate::pbimport::fixtures::reference_export;
use crate::testing::{self, GM_SUB, PLAYER_SUB};

/// A router over a real test database, plus the pool for seeding and
/// assertions.
async fn test_app() -> Option<(axum::Router, sqlx::PgPool)> {
    let pool = testing::test_pool().await?;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    Some((app, pool))
}

fn cookie_header(session_id: &str) -> String {
    format!("{}={}", oidc::SESSION_COOKIE, session_id)
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

fn post(path: &str, cookie: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("cookie", cookie_header(cookie))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn get_with_cookie(path: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("cookie", cookie_header(cookie))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn a_player_imports_the_reference_export_over_http() {
    let Some((app, pool)) = test_app().await else {
        return;
    };
    testing::seed_account(&pool, PLAYER_SUB, "player").await;
    let cookie = testing::seed_session(&pool, PLAYER_SUB, chrono::Utc::now()).await;

    let response = app
        .clone()
        .oneshot(post(
            "/api/characters/import",
            &cookie,
            reference_export().to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the import succeeds");
    let payload = response_json(response).await;
    assert_eq!(
        payload.pointer("/character/name"),
        Some(&serde_json::json!("Lorum Ipsum")),
        "summary"
    );
    assert_eq!(
        payload.pointer("/character/first_import"),
        Some(&serde_json::json!(true)),
        "first import"
    );
    assert_eq!(
        payload.pointer("/diff/first_import"),
        Some(&serde_json::json!(true)),
        "diff marks first import"
    );
    assert_eq!(
        payload.pointer("/advisory/skipped_fields"),
        Some(&serde_json::json!(0)),
        "no unknowns"
    );

    // The self-scoped read sees it.
    let me = app
        .clone()
        .oneshot(get_with_cookie("/api/characters/me", &cookie))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let me_payload = response_json(me).await;
    assert_eq!(
        me_payload.get("character").and_then(|c| c.get("name")),
        Some(&serde_json::json!("Lorum Ipsum"))
    );
    assert_eq!(
        me_payload.get("vitals").and_then(|v| v.get("hp")),
        Some(&serde_json::json!(14)),
        "seeded HP reads back"
    );

    testing::drop_test_db(pool, "http_import").await;
}

#[tokio::test]
async fn me_is_204_without_a_character() {
    let Some((app, pool)) = test_app().await else {
        return;
    };
    testing::seed_account(&pool, PLAYER_SUB, "player").await;
    let cookie = testing::seed_session(&pool, PLAYER_SUB, chrono::Utc::now()).await;

    let response = app
        .oneshot(get_with_cookie("/api/characters/me", &cookie))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "no character yet"
    );
    testing::drop_test_db(pool, "http_me_empty").await;
}

#[tokio::test]
async fn every_failure_class_answers_with_its_exact_envelope() {
    let Some((app, pool)) = test_app().await else {
        return;
    };
    testing::seed_account(&pool, PLAYER_SUB, "player").await;
    let cookie = testing::seed_session(&pool, PLAYER_SUB, chrono::Utc::now()).await;

    // (a) not JSON; (b) JSON, wrong shape; depth past the cap (built under
    // serde's own parse limit so it reaches our check).
    let deep = {
        let mut body = serde_json::Value::Bool(true);
        for _ in 0..70 {
            body = serde_json::json!({ "nested": body });
        }
        body.to_string()
    };
    let cases: Vec<(String, ImportError)> = vec![
        ("{not json at all".to_owned(), ImportError::InvalidJson),
        (
            r#"{"hello":"world"}"#.to_owned(),
            ImportError::NotPathbuilder,
        ),
        (deep, ImportError::PayloadTooDeep),
    ];
    for (body, class) in cases {
        let response = app
            .clone()
            .oneshot(post("/api/characters/import", &cookie, body))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{class:?} status"
        );
        let payload = response_json(response).await;
        assert_eq!(
            payload,
            serde_json::json!({
                "error": { "code": class.code(), "message": class.message() }
            }),
            "{class:?} envelope, verbatim"
        );
    }

    // size: over the cap, answered 413 before any parsing.
    let oversized = "x".repeat(crate::pbimport::caps::MAX_BODY_BYTES + 1);
    let response = app
        .oneshot(post("/api/characters/import", &cookie, oversized))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let payload = response_json(response).await;
    assert_eq!(
        payload.pointer("/error/code"),
        Some(&serde_json::json!("payload-too-large"))
    );
    assert_eq!(
        payload.pointer("/error/message"),
        Some(&serde_json::json!(ImportError::PayloadTooLarge.message())),
        "the exact message over the wire"
    );
    testing::drop_test_db(pool, "http_failure_classes").await;
}

#[tokio::test]
async fn the_gm_cannot_import_and_the_rejection_is_audited() {
    let Some((app, pool)) = test_app().await else {
        return;
    };
    testing::seed_account(&pool, GM_SUB, "gm").await;
    let cookie = testing::seed_session(&pool, GM_SUB, chrono::Utc::now()).await;

    let response = app
        .oneshot(post(
            "/api/characters/import",
            &cookie,
            reference_export().to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GM writes nothing"
    );
    let expected: Value =
        serde_json::from_str(Forbidden::body()).expect("the standard forbidden payload");
    assert_eq!(
        response_json(response).await,
        expected,
        "the standard forbidden payload"
    );
    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter().any(
            |(event, actor, _target, outcome)| event == "forbidden_gm_write"
                && actor.as_deref() == Some(GM_SUB)
                && outcome == "denied"
        ),
        "the middleware audited the GM write attempt — got {rows:?}"
    );
    testing::drop_test_db(pool, "http_gm_denied").await;
}

#[tokio::test]
async fn anonymous_import_is_rejected_before_any_validation() {
    let Some((app, pool)) = test_app().await else {
        return;
    };
    let response = app
        .oneshot(post("/api/characters/import", "no-cookie", "{}".to_owned()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        String::from_utf8(bytes.to_vec()).unwrap(),
        Unauthenticated::body(),
        "the standard unauthenticated payload, before validation"
    );
    testing::drop_test_db(pool, "http_anonymous").await;
}
