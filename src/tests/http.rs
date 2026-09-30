//! Tests for [`super`] — health, the SPA fallback, the auth layers' response
//! contract, and (when a Postgres is available) the full session lifecycle
//! and a stub-provider OIDC round trip.

use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::http::header::SET_COOKIE;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use super::API_ROUTES;
use super::router;
use crate::auth::error::Forbidden;
use crate::auth::error::Unauthenticated;
use crate::auth::oidc;
use crate::testing;

/// A static dir with one shell page, unique per call: concurrent tests
/// share one process (one pid), so a pid-keyed dir made every caller
/// truncate and rewrite the SAME index.html — a request served inside
/// that window read an empty body (the flaky
/// `unknown_api_paths_follow_the_spa_fallback_contract` failure).
fn static_dir() -> std::path::PathBuf {
    static CALL: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "hireling-http-test-{}-{}",
        std::process::id(),
        CALL.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut index = std::fs::File::create(dir.join("index.html")).unwrap();
    index.write_all(b"<h1>hireling</h1>").unwrap();
    dir
}

/// A router over a lazily-connected pool: fine for paths that never touch
/// the database (health, static, auth denials before the lookup).
fn test_router() -> axum::Router {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://hireling:hireling@127.0.0.1:5432/hireling")
        .expect("lazy test pool");
    router(
        testing::auth_state(pool, &testing::auth_settings()),
        &static_dir(),
    )
}

async fn response_body(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The value of the first `Set-Cookie` header whose cookie is named `name`.
fn set_cookie_value(response: &axum::response::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .find_map(|value| {
            let value = value.to_str().ok()?;
            let rest = value.strip_prefix(name)?;
            let rest = rest.strip_prefix('=')?;
            Some(rest.split(';').next().unwrap_or("").to_owned())
        })
}

fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

// --- E1 surfaces (unchanged contracts) ------------------------------------

#[tokio::test]
async fn healthz_returns_200_with_the_version_payload() {
    let response = test_router().oneshot(get("/healthz")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })
    );
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let response = test_router().oneshot(get("/healthz")).await.unwrap();

    assert!(
        response.headers().contains_key("x-request-id"),
        "a request without a correlation ID should get one generated"
    );
}

#[tokio::test]
async fn a_caller_supplied_request_id_is_propagated() {
    let response = test_router()
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
    let response = test_router()
        .oneshot(get("/some/frontend/route"))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unknown frontend paths should serve the shell page, not a 404"
    );
    assert_eq!(response_body(response).await, "<h1>hireling</h1>");
}

// --- the auth boundary (SC-5): every listed route, no session --------------

/// The ownership matrix, structural form (SC-5, FR-7): every route in the
/// declarative list, probed anonymously with its declared method — 401 with
/// the standard payload, proving each is (a) mounted (404/405 would mean
/// mount drift) and (b) inside the auth layer (an answer would mean unguarded).
/// Every route declared read-only must additionally 405 every mutating method:
/// no undeclared write handler may hide on its path.
#[tokio::test]
async fn every_listed_api_route_rejects_anonymous_requests_with_the_standard_401() {
    for route in API_ROUTES {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(route.method)
                    .uri(format!("{}?probe=1", route.path))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{} declares {} but the router does not mount it",
            route.path,
            route.method
        );
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{} must sit behind require_auth",
            route.path
        );
        assert_eq!(
            response_body(response).await,
            Unauthenticated::body(),
            "{} must speak the standard unauthenticated payload",
            route.path
        );

        if !route.writes {
            for method in ["POST", "PUT", "PATCH", "DELETE"] {
                let write_probe = test_router()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(route.path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                // Anonymous, the guard answers before method routing does:
                // a 401 here proves the mutating method never bypasses
                // require_auth (no public write path on a protected route).
                // The player-level 405 (no write handler at all) is asserted
                // with a live session in the database-gated matrix below.
                assert_eq!(
                    write_probe.status(),
                    StatusCode::UNAUTHORIZED,
                    "{} is declared read-only but {method} escapes the guard",
                    route.path
                );
            }
        }
    }
}

#[tokio::test]
async fn the_auth_legs_are_public_inside_the_api_nest() {
    // A GET to the legs without cookies is handled by the legs themselves —
    // never the standard 401 (which would mean the guard wraps them).
    let login = test_router().oneshot(get("/api/auth/login")).await.unwrap();
    assert_ne!(login.status(), StatusCode::UNAUTHORIZED, "login is public");

    let logout = test_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        logout.status(),
        StatusCode::NO_CONTENT,
        "logout without a session is idempotent success, not an auth failure"
    );
}

#[tokio::test]
async fn unknown_api_paths_follow_the_spa_fallback_contract() {
    // E1's contract: unmatched paths serve the shell page (the frontend owns
    // unknown-route UX). No API path leaks data, and nothing here changes that.
    let response = test_router().oneshot(get("/api/nothing")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, "<h1>hireling</h1>");
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn login_without_oidc_config_says_so_instead_of_erroring() {
    // The test fixture has no OIDC settings (debug builds allow that), so the
    // login leg must render its human-readable "not configured" face.
    let response = test_router().oneshot(get("/api/auth/login")).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response_body(response).await;
    assert!(body.contains("not configured"), "body: {body}");
}

// --- session lifecycle over the real router (needs Postgres) ---------------

/// The loopback peer the dev-session gate accepts (real servers supply this
/// via `ConnectInfo`; `oneshot` tests stamp it by hand).
fn loopback_peer() -> std::net::SocketAddr {
    std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), 51_515)
}

#[tokio::test]
async fn dev_session_authenticates_as_the_seat_end_to_end() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(
        pool.clone(),
        &testing::auth_settings_with_dev_sessions(testing::auth_settings()),
    );

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/dev/session")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"seat":"bear"}"#))
        .unwrap();
    testing::with_peer(&mut request, Some(loopback_peer()));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let session_cookie = set_cookie_value(&response, oidc::SESSION_COOKIE).expect("session cookie");

    let me_response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(
                    "cookie",
                    format!("{}={}", oidc::SESSION_COOKIE, session_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);
    assert_eq!(
        response_body(me_response).await,
        r#"{"sub":"dev-sub-bear","username":"bear","display_name":"bear","role":"player"}"#,
    );

    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter()
            .any(|(event, actor, _target, outcome)| event == "login_success"
                && actor.as_deref() == Some("dev-sub-bear")
                && outcome == "allowed"),
        "login must be audit-recorded, got {rows:?}"
    );
    testing::drop_test_db(pool, "dev_session_round_trip").await;
}

#[tokio::test]
async fn dev_session_rejects_unknown_seats() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(
        pool.clone(),
        &testing::auth_settings_with_dev_sessions(testing::auth_settings()),
    );

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/dev/session")
        .body(Body::from(r#"{"seat":"mallory"}"#))
        .unwrap();
    testing::with_peer(&mut request, Some(loopback_peer()));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response_body(response).await.contains("unknown seat"),
        "the error names the seat list"
    );
    testing::drop_test_db(pool, "dev_session_unknown_seat").await;
}

/// Without the explicit opt-in, the dev-session legs read as absent — an
/// ordinary debug run cannot mint a session. This is the failure that would
/// let a network neighbour take a seat in a default `just dev`.
#[tokio::test]
async fn dev_sessions_are_absent_without_the_opt_in() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    // Even a loopback peer gets nothing: the flag gates first.
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/dev/session")
        .body(Body::from(r#"{"seat":"bear"}"#))
        .unwrap();
    testing::with_peer(&mut request, Some(loopback_peer()));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "no opt-in, no dev route"
    );
    let (session_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    assert_eq!(session_rows, 0, "no session was minted");
    testing::drop_test_db(pool, "dev_sessions_off").await;
}

/// With the opt-in, remote peers are still rejected — only loopback may
/// mint a dev session.
#[tokio::test]
async fn dev_sessions_reject_remote_peers_even_when_enabled() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(
        pool.clone(),
        &testing::auth_settings_with_dev_sessions(testing::auth_settings()),
    );

    let remote = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 9, 9, 9)),
        44_444,
    );
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/dev/session")
        .body(Body::from(r#"{"seat":"bear"}"#))
        .unwrap();
    testing::with_peer(&mut request, Some(remote));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a non-loopback peer cannot take a dev seat"
    );
    let (session_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    assert_eq!(session_rows, 0, "no session was minted");
    testing::drop_test_db(pool, "dev_sessions_remote").await;
}

#[tokio::test]
async fn a_tampered_session_cookie_is_unauthenticated() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    let real = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;

    // Flip the first character to a *different* one. A naive "set it to 'A'"
    // is a no-op whenever the random token already starts with 'A' (p≈1/64),
    // which authenticated the tampered cookie and flaked the test.
    let tampered: String = real
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                if c == 'A' { 'B' } else { 'A' }
            } else {
                c
            }
        })
        .collect();
    assert_ne!(tampered, real, "the tamper must change the cookie value");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", format!("{}={}", oidc::SESSION_COOKIE, tampered))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response_body(response).await, Unauthenticated::body());
    testing::drop_test_db(pool, "tampered_cookie").await;
}

#[tokio::test]
async fn the_gm_reads_everything_and_writes_nothing() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(
        pool.clone(),
        &testing::auth_settings_with_dev_sessions(testing::auth_settings()),
    );

    // GM seat logs in.
    let mut login_request = Request::builder()
        .method("POST")
        .uri("/api/dev/session")
        .body(Body::from(r#"{"seat":"gm"}"#))
        .unwrap();
    testing::with_peer(&mut login_request, Some(loopback_peer()));
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::NO_CONTENT);
    let session_cookie =
        set_cookie_value(&login_response, oidc::SESSION_COOKIE).expect("session cookie");
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session_cookie);

    // Reads succeed like any party member's.
    let read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        read_response.status(),
        StatusCode::OK,
        "GM reads are unrestricted"
    );
    assert!(
        response_body(read_response)
            .await
            .contains(r#""role":"gm""#),
        "the GM seat resolves with the gm role"
    );

    // Any mutating method is rejected at the transport layer — even where no
    // write handler exists — with the standard payload and an audit record.
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let write_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/api/me")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            write_response.status(),
            StatusCode::FORBIDDEN,
            "GM {method} must be rejected by the gm_read_only layer"
        );
        assert_eq!(
            response_body(write_response).await,
            Forbidden::body(),
            "GM {method} must speak the standard forbidden payload"
        );
    }

    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter().any(
            |(event, actor, _target, outcome)| event == "forbidden_gm_write"
                && actor.as_deref() == Some("dev-sub-gm")
                && outcome == "denied"
        ),
        "the GM write attempt must be audit-recorded, got {rows:?}"
    );
    testing::drop_test_db(pool, "gm_read_only").await;
}

#[tokio::test]
async fn a_player_write_to_a_read_route_is_not_the_gm_rejection() {
    // A non-GM write to a GET-only route passes the transport layers and hits
    // axum's method routing (405) — the GM 403 is role-specific, not a
    // blanket method ban. Every route declared read-only in the matrix must
    // behave this way: a mounted write handler would answer 2xx/4xx here
    // instead of 405.
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    for route in API_ROUTES.iter().filter(|route| !route.writes) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(route.path)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{} is declared read-only but a player's write did not hit method routing",
            route.path
        );
    }

    let rows = testing::audit_rows(&pool).await;
    assert!(
        !rows
            .iter()
            .any(|(event, _, _, _)| event == "forbidden_gm_write"),
        "a player's write attempt is not a GM violation"
    );
    testing::drop_test_db(pool, "player_post_405").await;
}

// --- boundary: logout must not claim invalidation it did not perform ------

#[tokio::test]
async fn a_failed_logout_does_not_claim_invalidation() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    // Break the session store without losing the row: the lookup (and any
    // delete) fails, while the session itself stays alive.
    sqlx::query("ALTER TABLE sessions RENAME TO sessions_moved")
        .execute(&pool)
        .await
        .expect("move sessions table away");
    let failed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        failed.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "logout must report failure, not a 204 that lies"
    );

    // The same cookie still authenticates: nothing was invalidated.
    sqlx::query("ALTER TABLE sessions_moved RENAME TO sessions")
        .execute(&pool)
        .await
        .expect("restore sessions table");
    let replay = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        replay.status(),
        StatusCode::OK,
        "a failed logout leaves the session alive — and says so"
    );

    // Retry with the store back: the logout now succeeds and holds.
    let retry = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::NO_CONTENT);
    let replay_after = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay_after.status(), StatusCode::UNAUTHORIZED);
    testing::drop_test_db(pool, "failed_logout").await;
}

// --- boundary: a GM rejection that cannot be audited fails closed ----------

#[tokio::test]
async fn a_gm_write_rejection_that_cannot_be_audited_fails_closed() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    // GM seat, live session (seed the GM account explicitly; seed_session
    // would upsert a player role).
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    // Sanity first: with the audit sink intact the GM write is a 403.
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "a healthy audit sink records the rejection and denies"
    );

    // Break the audit sink: the rejection cannot be recorded, so it is not
    // delivered as a 403 — the request fails closed instead.
    sqlx::query("DROP TABLE audit_events")
        .execute(&pool)
        .await
        .expect("drop audit_events");
    let failed = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        failed.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "an unrecordable GM rejection must not be delivered as a 403"
    );
    testing::drop_test_db(pool, "gm_audit_failclosed").await;
}

#[tokio::test]
async fn logout_destroys_the_session_server_side() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    let logout_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    // Replaying the old cookie is rejected as unauthenticated (US6.3).
    let replay_response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay_response.status(), StatusCode::UNAUTHORIZED);

    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter()
            .any(|(event, actor, _target, _outcome)| event == "logout"
                && actor.as_deref() == Some("dev-sub-josh")),
        "logout is audit-recorded, got {rows:?}"
    );
    testing::drop_test_db(pool, "logout").await;
}

#[tokio::test]
async fn an_idle_expired_session_is_rejected_as_unauthenticated() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    // last_seen 25 hours ago, idle window 24h: both deadlines derive from the
    // seeded `now`, so the session expired an hour ago.
    let old = chrono::Utc::now() - chrono::Duration::hours(25);
    let session = testing::seed_session(&pool, "dev-sub-josh", old).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response_body(response).await, Unauthenticated::body());
    testing::drop_test_db(pool, "idle_expiry").await;
}

#[tokio::test]
async fn the_absolute_cap_holds_even_inside_the_idle_window() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    // Seeded "now" is 8 days ago with a 30-day idle window and a 7-day
    // absolute cap: the idle deadline is in the future, the cap is past.
    let old = chrono::Utc::now() - chrono::Duration::days(8);
    testing::seed_account(&pool, "dev-sub-josh", "player").await;
    crate::auth::session::insert(
        &pool,
        "abs-cap-session",
        "dev-sub-josh",
        old,
        30 * 86_400,
        7 * 86_400,
    )
    .await
    .unwrap();
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, "abs-cap-session");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "activity never rescues the absolute cap"
    );
    testing::drop_test_db(pool, "absolute_cap").await;
}

#[tokio::test]
async fn the_idle_window_slides_on_activity() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    // last_seen 10 minutes ago: past the 5-minute amortization window, so the
    // next authenticated request must renew.
    let old = chrono::Utc::now() - chrono::Duration::minutes(10);
    let session = testing::seed_session(&pool, "dev-sub-josh", old).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (last_seen_at,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT last_seen_at FROM sessions WHERE id = $1")
            .bind(&session)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        last_seen_at > chrono::Utc::now() - chrono::Duration::minutes(1),
        "the renewal must have written last_seen_at forward, got {last_seen_at}"
    );
    testing::drop_test_db(pool, "sliding_window").await;
}

#[tokio::test]
async fn allowlist_withdrawal_denies_the_next_request() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // Session created while the account was allowlisted...
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);

    // ...then the config withdraws the sub.
    let mut auth = testing::auth_settings();
    auth.allowlist.remove("dev-sub-josh");
    let app = testing::router_for(pool.clone(), &auth);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "withdrawal denies the account on its next request, no session restart"
    );
    testing::drop_test_db(pool, "allowlist_withdrawal").await;
}

// --- the full OIDC round trip against a stub provider (needs Postgres) -----

mod stub_provider {
    //! A tiny local provider: a JWKS endpoint and a token endpoint that mints
    //! a real RS256 ID token. Everything else (authorize page, login UI) is
    //! Authentik's job and is not faked.
    //!
    //! The signing keys are static fixtures (`fixtures/*.pem`), generated once
    //! and checked in so the `rsa` crate never enters the dependency tree:
    //! RUSTSEC-2023-0071 flags every rsa release (no patch exists), and the
    //! gate is fail-closed. They sign tokens for a stub client against a stub
    //! issuer only — a fixture, not a secret.

    use axum::extract::State;
    use axum::routing::get;
    use axum::routing::post;
    use axum::{Json, Router};
    use jsonwebtoken::Algorithm;
    use jsonwebtoken::EncodingKey;
    use jsonwebtoken::Header;
    use serde::Deserialize;
    use serde::Serialize;
    use tokio::sync::Mutex;

    pub const KID: &str = "test-signing-key";

    /// Modulus of `fixtures/stub_signing_key.pem`, base64url (the JWKS `n`).
    const SIGNING_KEY_N: &str = "r-n_qZIFd9FR1Yx5CwpBRX1x2Azlj6x-4chzDbPGiIzlq1MuWUejdhdyps-nFqovtIu_UdMQnnr8Wjjc8am9QuZmFrrC099FRkMGzVrLnzojN7mIlKYpRQ4A_XNxn9klAGrpSS3iuQbW4ZnNAOKPle25s0V0F2uMyK2uZseD3EbYXL_4-mBCFfw9cPzIRqeoz0jOdf0ZzvI5zis0bRqWJO8CUX_MmLJ-Nfu4sn_YxBbOEkTqlL9_nZGA1VnMsrHZTZ7VifIW5KBXNAta1_nnVFD0gr5ev8r6uDuQ3lZW1cOaPii3gqxLG3RWR1jkJqMQQdkywQiQQZGv-lUW0ejkzw";

    #[derive(Clone)]
    pub struct Stub {
        pub config: std::sync::Arc<Mutex<StubConfig>>,
        pub recorded: std::sync::Arc<Mutex<Vec<RecordedTokenRequest>>>,
        pub jwks: std::sync::Arc<serde_json::Value>,
        pub enc_key: std::sync::Arc<EncodingKey>,
        pub decoy_key: std::sync::Arc<EncodingKey>,
    }

    #[derive(Debug, Clone)]
    pub struct StubConfig {
        pub sub: String,
        pub nonce: String,
        pub aud: String,
        pub issuer: String,
        /// `false` makes the token endpoint sign with a key absent from the
        /// JWKS — simulating an attacker-minted token.
        pub signed_by_jwks_key: bool,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct RecordedTokenRequest {
        pub authorization: String,
        pub form: serde_json::Value,
    }

    #[derive(Serialize, Deserialize)]
    struct StubClaims {
        iss: String,
        aud: String,
        sub: String,
        exp: u64,
        nonce: String,
    }

    pub async fn spawn() -> (String, Stub) {
        let enc_key =
            EncodingKey::from_rsa_pem(include_str!("fixtures/stub_signing_key.pem").as_bytes())
                .expect("encoding key from fixture");
        let decoy_key =
            EncodingKey::from_rsa_pem(include_str!("fixtures/stub_decoy_key.pem").as_bytes())
                .expect("decoy encoding key from fixture");

        let jwks = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "alg": "RS256",
                "use": "sig",
                "kid": KID,
                "n": SIGNING_KEY_N,
                "e": "AQAB",
            }]
        });

        let stub = Stub {
            config: std::sync::Arc::new(Mutex::new(StubConfig {
                sub: "stub-sub".to_owned(),
                nonce: String::new(),
                aud: "stub-client".to_owned(),
                issuer: "https://auth.test/hireling/".to_owned(),
                signed_by_jwks_key: true,
            })),
            recorded: std::sync::Arc::new(Mutex::new(Vec::new())),
            jwks: std::sync::Arc::new(jwks),
            enc_key: std::sync::Arc::new(enc_key),
            decoy_key: std::sync::Arc::new(decoy_key),
        };

        let app = Router::new()
            .route("/jwks", get(serve_jwks))
            .route("/token", post(serve_token))
            .with_state(stub.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), stub)
    }

    async fn serve_jwks(State(stub): State<Stub>) -> Json<serde_json::Value> {
        Json((*stub.jwks).clone())
    }

    /// Percent-decode an `application/x-www-form-urlencoded` value — what the
    /// token endpoint receives, `reqwest`'s `.form()` sends properly encoded.
    fn form_decode(value: &str) -> String {
        let spaced = value.replace('+', " ");
        let bytes = spaced.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while let Some(byte) = bytes.get(index) {
            if *byte == b'%'
                && let Some([high, low]) = bytes.get(index + 1..index + 3)
                && let Some(decoded) = hex_byte([*high, *low])
            {
                out.push(decoded);
                index += 3;
            } else {
                out.push(*byte);
                index += 1;
            }
        }
        String::from_utf8(out).unwrap_or(spaced)
    }

    /// One `%XX` byte, or `None` when the pair is not two hex digits.
    fn hex_byte([high, low]: [u8; 2]) -> Option<u8> {
        let high = u8::try_from((high as char).to_digit(16)?).ok()?;
        let low = u8::try_from((low as char).to_digit(16)?).ok()?;
        Some(high * 16 + low)
    }

    async fn serve_token(
        State(stub): State<Stub>,
        headers: axum::http::HeaderMap,
        body: String,
    ) -> Json<serde_json::Value> {
        let authorization = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let form: Vec<(String, String)> = body
            .split('&')
            .filter_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                Some((key.to_owned(), form_decode(value)))
            })
            .collect();
        let form = serde_json::to_value(form).unwrap_or(serde_json::Value::Null);
        stub.recorded.lock().await.push(RecordedTokenRequest {
            authorization,
            form,
        });

        let config = stub.config.lock().await.clone();
        let signing_key = if config.signed_by_jwks_key {
            &*stub.enc_key
        } else {
            &*stub.decoy_key
        };
        let claims = StubClaims {
            iss: config.issuer,
            aud: config.aud,
            sub: config.sub,
            exp: chrono::Utc::now().timestamp().unsigned_abs() + 600,
            nonce: config.nonce,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(KID.to_owned());
        let token = jsonwebtoken::encode(&header, &claims, signing_key).expect("sign stub token");
        Json(serde_json::json!({
            "access_token": "not-used",
            "id_token": token,
            "token_type": "Bearer",
            "expires_in": 300,
        }))
    }
}

/// Settings pointed at the stub provider.
fn stub_oidc_settings(base: &str) -> crate::config::OidcSettings {
    crate::config::OidcSettings {
        issuer: "https://auth.test/hireling/".to_owned(),
        client_id: "stub-client".to_owned(),
        client_secret: "sekrit".to_owned(),
        base_url: "https://hireling.test".to_owned(),
        urls: crate::auth::oidc::ProviderUrls {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            jwks: format!("{base}/jwks"),
        },
    }
}

/// Drive the app's login leg and return (redirect target, transaction cookie).
async fn start_login(app: &axum::Router) -> (String, String) {
    let response = app.clone().oneshot(get("/api/auth/login")).await.unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let transaction_cookie =
        set_cookie_value(&response, oidc::TRANSACTION_COOKIE).expect("transaction cookie");
    (location, transaction_cookie)
}

fn query_param(url: &str, name: &str) -> String {
    url.split('?')
        .nth(1)
        .and_then(|query| {
            query.split('&').find_map(|pair| {
                pair.split_once('=')
                    .filter(|(key, _)| *key == name)
                    .map(|(_, value)| value.to_owned())
            })
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn the_full_oidc_round_trip_establishes_a_session() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth = testing::auth_settings_with_oidc(
        {
            let mut auth = testing::auth_settings();
            auth.allowlist.insert("stub-sub".to_owned());
            auth
        },
        stub_oidc_settings(&base),
    );
    let app = testing::router_for(pool.clone(), &auth);

    // 1. login: the app mints the transaction and aims the browser at the
    //    provider with the full code-flow request.
    let (location, transaction_cookie) = start_login(&app).await;
    assert!(location.starts_with("http://127.0.0.1:"), "{location}");
    let state = query_param(&location, "state");
    let nonce = query_param(&location, "nonce");
    let challenge = query_param(&location, "code_challenge");
    assert!(!state.is_empty() && !nonce.is_empty() && !challenge.is_empty());
    assert_eq!(query_param(&location, "code_challenge_method"), "S256");

    // 2. the provider signs the ID token with the state the app sent.
    {
        let mut config = stub.config.lock().await;
        config.sub = "stub-sub".to_owned();
        config.nonce = nonce;
    }

    // 3. callback: exchange, validate, allowlist, upsert, session.
    let callback_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/auth/callback?code=the-code&state={state}"))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        callback_response.status(),
        StatusCode::FOUND,
        "callback must redirect into the app"
    );
    assert_eq!(
        callback_response
            .headers()
            .get(axum::http::header::LOCATION)
            .unwrap(),
        "/",
        "a logged-in player lands on the app"
    );
    let session_cookie = set_cookie_value(&callback_response, oidc::SESSION_COOKIE)
        .expect("session cookie on callback");

    // 4. the session identifies the account everywhere.
    let me_response = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(
                    "cookie",
                    format!("{}={}", oidc::SESSION_COOKIE, session_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);
    assert_eq!(
        response_body(me_response).await,
        r#"{"sub":"stub-sub","username":"stub-sub","display_name":"stub-sub","role":"player"}"#,
    );

    // 5. the token exchange was what the contract says: basic client auth,
    //    the code, the redirect URI, and the PKCE verifier matching the
    //    challenge from step 1.
    assert_token_exchange(&stub, &challenge).await;

    // 6. the login is audit-recorded.
    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter()
            .any(|(event, actor, _target, outcome)| event == "login_success"
                && actor.as_deref() == Some("stub-sub")
                && outcome == "allowed"),
        "login_success must be recorded, got {rows:?}"
    );
    testing::drop_test_db(pool, "oidc_round_trip").await;
}

/// SC-8: one record per login. With the audit sink broken, the callback must
/// fail the login and persist nothing — no session, no account row.
#[tokio::test]
async fn a_login_without_its_audit_record_leaves_no_session() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth = testing::auth_settings_with_oidc(
        {
            let mut auth = testing::auth_settings();
            auth.allowlist.insert("stub-sub".to_owned());
            auth
        },
        stub_oidc_settings(&base),
    );
    let app = testing::router_for(pool.clone(), &auth);

    // Break the audit sink: with the record unwritable, the login must not
    // produce a session.
    sqlx::query("DROP TABLE audit_events")
        .execute(&pool)
        .await
        .expect("drop audit_events");

    let (location, transaction_cookie) = start_login(&app).await;
    {
        let mut config = stub.config.lock().await;
        config.sub = "stub-sub".to_owned();
        config.nonce = query_param(&location, "nonce");
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/auth/callback?code=the-code&state={}",
                    query_param(&location, "state")
                ))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a login whose audit record cannot be written must fail, not silently pass"
    );

    let (session_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    let (account_rows,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM accounts WHERE sub = 'stub-sub'")
            .fetch_one(&pool)
            .await
            .expect("count accounts");
    assert_eq!(session_rows, 0, "no session without its audit record");
    assert_eq!(
        account_rows, 0,
        "the account upsert rolled back with the session"
    );

    testing::drop_test_db(pool, "audit_failure_login").await;
}

/// SC-8 counts allowlist denials in "100% of logins": a denial whose record
/// cannot be written is not delivered — the browser gets a server error
/// instead of the denial page.
#[tokio::test]
async fn an_allowlist_denial_without_its_audit_record_is_a_server_error() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth =
        testing::auth_settings_with_oidc(testing::auth_settings(), stub_oidc_settings(&base));
    let app = testing::router_for(pool.clone(), &auth);

    sqlx::query("DROP TABLE audit_events")
        .execute(&pool)
        .await
        .expect("drop audit_events");

    let (location, transaction_cookie) = start_login(&app).await;
    {
        let mut config = stub.config.lock().await;
        config.sub = "stranger-sub".to_owned();
        config.nonce = query_param(&location, "nonce");
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/auth/callback?code=the-code&state={}",
                    query_param(&location, "state")
                ))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a denial that cannot be recorded must not be delivered as a 403"
    );

    let (session_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    assert_eq!(session_rows, 0, "denied logins never mint sessions");

    testing::drop_test_db(pool, "audit_failure_denial").await;
}

/// The token request the app made must match the captured house contract:
/// `client_secret_basic` auth, the authorization code, the exact redirect URI,
/// and a PKCE verifier whose S256 challenge is the one from the authorize
/// redirect.
async fn assert_token_exchange(stub: &stub_provider::Stub, challenge: &str) {
    let recorded = stub.recorded.lock().await;
    assert_eq!(recorded.len(), 1, "exactly one token request");
    let token_request = recorded.first().expect("one token request");
    assert!(
        token_request.authorization.starts_with("Basic "),
        "client auth must be client_secret_basic, got {:?}",
        token_request.authorization
    );
    let form = token_request.form.as_array().expect("form as pairs");
    let form_value = |name: &str| {
        form.iter()
            .find_map(|pair| match pair.as_array().map(Vec::as_slice) {
                Some([key, value]) if key.as_str() == Some(name) => {
                    value.as_str().map(String::from)
                }
                _ => None,
            })
            .unwrap_or_default()
    };
    assert_eq!(form_value("grant_type"), "authorization_code");
    assert_eq!(form_value("code"), "the-code");
    assert_eq!(
        form_value("redirect_uri"),
        "https://hireling.test/api/auth/callback"
    );
    let verifier = form_value("code_verifier");
    assert_eq!(
        oidc_pkce(&verifier),
        challenge,
        "the sent verifier must hash to the challenge from the authorize URL"
    );
}

fn oidc_pkce(verifier: &str) -> String {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
    use sha2::Digest;
    use sha2::Sha256;
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

#[tokio::test]
async fn a_login_by_an_account_not_on_the_allowlist_is_denied_and_recorded() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth =
        testing::auth_settings_with_oidc(testing::auth_settings(), stub_oidc_settings(&base));
    let app = testing::router_for(pool.clone(), &auth);

    let (location, transaction_cookie) = start_login(&app).await;
    let state = query_param(&location, "state");
    {
        let mut config = stub.config.lock().await;
        config.sub = "stranger-sub".to_owned();
        config.nonce = query_param(&location, "nonce");
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/auth/callback?code=c&state={state}"))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        set_cookie_value(&response, oidc::SESSION_COOKIE).is_none(),
        "a denied login must not establish a session"
    );
    let body = response_body(response).await;
    assert!(
        body.contains("not provisioned"),
        "the denial is human-readable: {body}"
    );

    let rows = testing::audit_rows(&pool).await;
    assert!(
        rows.iter().any(
            |(event, actor, _target, outcome)| event == "login_allowlist_denied"
                && actor.as_deref() == Some("stranger-sub")
                && outcome == "denied"
        ),
        "the denial must be audit-recorded with the presented sub, got {rows:?}"
    );
    testing::drop_test_db(pool, "allowlist_denied").await;
}

#[tokio::test]
async fn a_callback_state_mismatch_is_rejected_without_a_session() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, _stub) = stub_provider::spawn().await;
    let auth =
        testing::auth_settings_with_oidc(testing::auth_settings(), stub_oidc_settings(&base));
    let app = testing::router_for(pool.clone(), &auth);

    let (_location, transaction_cookie) = start_login(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/callback?code=c&state=not-my-state")
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        set_cookie_value(&response, oidc::SESSION_COOKIE).is_none(),
        "no session for a state mismatch"
    );
    testing::drop_test_db(pool, "state_mismatch").await;
}

#[tokio::test]
async fn a_tampered_transaction_cookie_never_reaches_the_token_exchange() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, _stub) = stub_provider::spawn().await;
    let auth =
        testing::auth_settings_with_oidc(testing::auth_settings(), stub_oidc_settings(&base));
    let app = testing::router_for(pool.clone(), &auth);

    let (_location, transaction_cookie) = start_login(&app).await;
    let forged = format!("{transaction_cookie}x");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/callback?code=c&state=anything")
                .header("cookie", format!("{}={}", oidc::TRANSACTION_COOKIE, forged))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "a forged transaction fails signature verification and stops before the token exchange"
    );
    testing::drop_test_db(pool, "tampered_transaction").await;
}

#[tokio::test]
async fn an_id_token_with_the_wrong_nonce_is_refused() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth = testing::auth_settings_with_oidc(
        {
            let mut auth = testing::auth_settings();
            auth.allowlist.insert("stub-sub".to_owned());
            auth
        },
        stub_oidc_settings(&base),
    );
    let app = testing::router_for(pool.clone(), &auth);

    let (location, transaction_cookie) = start_login(&app).await;
    let state = query_param(&location, "state");
    {
        let mut config = stub.config.lock().await;
        config.sub = "stub-sub".to_owned();
        config.nonce = "a-nonce-from-a-different-login".to_owned();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/auth/callback?code=c&state={state}"))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "nonce replay/mismatch must fail the login"
    );
    assert!(
        set_cookie_value(&response, oidc::SESSION_COOKIE).is_none(),
        "no session survives a failed validation"
    );
    testing::drop_test_db(pool, "wrong_nonce").await;
}

#[tokio::test]
async fn an_id_token_signed_outside_the_jwks_is_refused() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (base, stub) = stub_provider::spawn().await;
    let auth = testing::auth_settings_with_oidc(
        {
            let mut auth = testing::auth_settings();
            auth.allowlist.insert("stub-sub".to_owned());
            auth
        },
        stub_oidc_settings(&base),
    );
    let app = testing::router_for(pool.clone(), &auth);

    let (location, transaction_cookie) = start_login(&app).await;
    let state = query_param(&location, "state");
    {
        let mut config = stub.config.lock().await;
        config.sub = "stub-sub".to_owned();
        config.nonce = query_param(&location, "nonce");
        config.signed_by_jwks_key = false;
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/auth/callback?code=c&state={state}"))
                .header(
                    "cookie",
                    format!("{}={}", oidc::TRANSACTION_COOKIE, transaction_cookie),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "a token from an unknown signer must fail signature validation"
    );
    testing::drop_test_db(pool, "bad_signer").await;
}

// --- profile-gated dev route checks ----------------------------------------

#[cfg(not(debug_assertions))]
#[tokio::test]
async fn release_builds_have_no_dev_session_route() {
    // Run under `cargo test --release` (release-gate time): the route and its
    // handler are compiled out, so no environment variable can conjure it.
    let response = test_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/dev/session")
                .body(Body::from(r#"{"seat":"bear"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
