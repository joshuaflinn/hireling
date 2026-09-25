//! The `/api/auth/*` legs, `GET /api/me`, and the debug-only dev-session
//! route. Thin shell: each handler orchestrates the pure modules and the I/O
//! modules, and renders one of three faces — a redirect, a small HTML page,
//! or the standard JSON payloads.

use axum::Json;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::header::LOCATION;
use axum::response::IntoResponse;
use axum::response::Response;
use chrono::Utc;
use cookie::Cookie;
use cookie::SameSite;
use cookie::time::Duration as CookieDuration;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::account;
use crate::auth::audit;
use crate::auth::audit::AuditEvent;
use crate::auth::audit::AuditOutcome;
use crate::auth::middleware::SessionAccount;
use crate::auth::oidc;
use crate::auth::provider;
use crate::auth::session;

/// `GET /api/me` — the session's account. The SPA's auth probe and the
/// ownership tests' identity check.
pub async fn me(
    SessionAccount {
        sub,
        username,
        display_name,
        role,
    }: SessionAccount,
) -> Json<Me> {
    Json(Me {
        sub,
        username,
        display_name,
        role: role.as_str().to_owned(),
    })
}

#[derive(Serialize)]
pub struct Me {
    pub sub: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

/// `GET /api/auth/login` — mint the transaction, drop the signed transaction
/// cookie, and send the browser to the provider. No password field ever
/// renders here (spec FR-1).
pub async fn login(State(state): State<Arc<AuthState>>) -> Response {
    let Some(oidc_cfg) = state.oidc.clone() else {
        return html_page(
            StatusCode::SERVICE_UNAVAILABLE,
            "Login unavailable",
            "Authentication is not configured on this instance. In development, \
             use the dev session route.",
        );
    };

    let transaction = match oidc::Transaction::generate() {
        Ok(transaction) => transaction,
        Err(err) => return login_server_error("could not start the login", &err),
    };
    let cookie_value = match transaction.encode(&state.cookie_key, Utc::now()) {
        Ok(value) => value,
        Err(err) => return login_server_error("could not start the login", &err),
    };
    let authorize = oidc::authorize_url(
        &oidc_cfg.urls,
        &oidc_cfg.client_id,
        &oidc_cfg.redirect_uri(),
        &transaction,
    );

    let mut transaction_cookie = Cookie::new(oidc::TRANSACTION_COOKIE, cookie_value);
    harden_cookie(&mut transaction_cookie);
    let ttl = i64::try_from(oidc::TRANSACTION_TTL.as_secs()).unwrap_or(i64::MAX);
    transaction_cookie.set_max_age(CookieDuration::seconds(ttl));

    let mut response = (StatusCode::FOUND, [(LOCATION, authorize)]).into_response();
    append_cookie(&mut response, &transaction_cookie);
    response
}

/// `GET /api/auth/callback` — the provider came back. Verify the transaction,
/// exchange the code, validate the ID token, enforce the allowlist, upsert
/// the account, and establish the session.
pub async fn callback(
    State(state): State<Arc<AuthState>>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> Response {
    let Some(transaction) = take_transaction(&state, &headers) else {
        return html_page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            "This login attempt could not be verified (missing or stale login \
             state). Start again from the app.",
        );
    };

    if params.state.as_deref() != Some(transaction.state.as_str()) {
        return html_page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            "The login response did not match this browser's login attempt.",
        );
    }

    let Some(code) = params.code.as_deref() else {
        return html_page(
            StatusCode::UNAUTHORIZED,
            "Login not completed",
            "The identity provider did not approve the login. If you cancelled \
             or your account is not provisioned, contact the administrator.",
        );
    };

    match complete_login(&state, &transaction, code).await {
        Ok(session_id) => {
            let mut session_cookie = Cookie::new(oidc::SESSION_COOKIE, session_id);
            harden_cookie(&mut session_cookie);
            let mut cleared = Cookie::new(oidc::TRANSACTION_COOKIE, "");
            harden_cookie(&mut cleared);
            cleared.set_max_age(CookieDuration::ZERO);

            let mut response = (StatusCode::FOUND, [(LOCATION, "/".to_owned())]).into_response();
            append_cookie(&mut response, &session_cookie);
            append_cookie(&mut response, &cleared);
            response
        }
        Err(LoginFailure::Denied(identity)) => {
            audit::record(
                &state.pool,
                AuditEvent::LoginAllowlistDenied,
                Some(&identity.sub),
                "login",
                AuditOutcome::Denied,
                None,
            )
            .await;
            html_page(
                StatusCode::FORBIDDEN,
                "Not on the list",
                "This account is not provisioned for Hireling. Ask the \
                 administrator to add you to the party.",
            )
        }
        Err(LoginFailure::Provider(err)) => {
            tracing::warn!(error = %err, "login round trip failed");
            html_page(
                StatusCode::BAD_GATEWAY,
                "Login failed",
                "The sign-in service could not complete the login. Try again; \
                 if it keeps failing, the administrator should check the \
                 identity provider.",
            )
        }
    }
}

/// Exchange, validate, allowlist, upsert, and insert the session. Returns the
/// new session id.
async fn complete_login(
    state: &Arc<AuthState>,
    transaction: &oidc::Transaction,
    code: &str,
) -> Result<String, LoginFailure> {
    let oidc_cfg = state
        .oidc
        .clone()
        .ok_or_else(|| LoginFailure::Provider(anyhow::anyhow!("OIDC is not configured")))?;

    let id_token = provider::exchange_code(
        state,
        code,
        &transaction.code_verifier,
        &oidc_cfg.redirect_uri(),
    )
    .await
    .map_err(LoginFailure::Provider)?;
    let claims = provider::validate_id_token(state, &id_token, &transaction.nonce)
        .await
        .map_err(LoginFailure::Provider)?;

    let identity = oidc::map_claims(&claims, state.seat_name(&claims.sub));
    if !state.allowlist.contains(&identity.sub) {
        return Err(LoginFailure::Denied(identity));
    }
    account::upsert(&state.pool, &identity, &state.gm_sub)
        .await
        .map_err(LoginFailure::Provider)?;

    let session_id = oidc::random_token().map_err(LoginFailure::Provider)?;
    session::insert(
        &state.pool,
        &session_id,
        &identity.sub,
        Utc::now(),
        state.session_idle_secs,
        state.session_absolute_secs,
    )
    .await
    .map_err(LoginFailure::Provider)?;
    audit::record(
        &state.pool,
        AuditEvent::LoginSuccess,
        Some(&identity.sub),
        "login",
        AuditOutcome::Allowed,
        None,
    )
    .await;
    Ok(session_id)
}

enum LoginFailure {
    /// Authenticated but not on the allowlist — carries the presented
    /// identity for the audit record.
    Denied(oidc::AccountIdentity),
    /// Anything on the provider round trip.
    Provider(anyhow::Error),
}

/// The query parameters the callback leg consumes. Anything else the provider
/// appends (e.g. `error`) is deliberately ignored; missing legs read as `None`
/// and take the failure paths. Public only because the handler is.
#[derive(Deserialize)]
pub struct CallbackParams {
    state: Option<String>,
    code: Option<String>,
}

/// `POST /api/auth/logout` — delete the server-side session (if any) and
/// clear the cookie. Idempotent: logging out without a session succeeds.
/// The house `IdP` session is deliberately left alone; it is shared with other
/// applications.
pub async fn logout(State(state): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    if let Some(cookie_header) = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
    {
        let jar = oidc::jar_from_header(cookie_header);
        if let Some(cookie) = jar.get(oidc::SESSION_COOKIE) {
            destroy_session(&state, cookie.value()).await;
        }
    }

    let mut cleared = Cookie::new(oidc::SESSION_COOKIE, "");
    harden_cookie(&mut cleared);
    cleared.set_max_age(CookieDuration::ZERO);
    let mut response = StatusCode::NO_CONTENT.into_response();
    append_cookie(&mut response, &cleared);
    response
}

/// Delete one session row and audit the logout. Failures are logged, never
/// surfaced: the client's cookie is cleared regardless, so the outcome is
/// the same for the browser.
async fn destroy_session(state: &Arc<AuthState>, session_id: &str) {
    let record = match session::find(&state.pool, session_id).await {
        Ok(Some(record)) => record,
        Ok(None) => return,
        Err(err) => {
            tracing::error!(error = %err, "logout could not load the session");
            return;
        }
    };
    match session::delete(&state.pool, session_id).await {
        Ok(true) => {
            audit::record(
                &state.pool,
                AuditEvent::Logout,
                Some(&record.account_sub),
                "session",
                AuditOutcome::Allowed,
                None,
            )
            .await;
        }
        Ok(false) => {}
        Err(err) => {
            tracing::error!(error = %err, "logout could not delete the session");
        }
    }
}

/// Read and verify the transaction cookie.
fn take_transaction(state: &Arc<AuthState>, headers: &HeaderMap) -> Option<oidc::Transaction> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    let jar = oidc::jar_from_header(cookie_header);
    let value = jar.get(oidc::TRANSACTION_COOKIE)?.value();
    oidc::Transaction::decode(value, &state.cookie_key, Utc::now())
}

/// The browser-facing cookie hardening every auth cookie gets.
fn harden_cookie(cookie: &mut Cookie<'static>) {
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
}

fn append_cookie(response: &mut Response, cookie: &Cookie<'static>) {
    if let Ok(value) = header::HeaderValue::from_str(&cookie.to_string()) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn login_server_error(message: &'static str, err: &anyhow::Error) -> Response {
    tracing::error!(error = %err, "login could not start");
    html_page(StatusCode::INTERNAL_SERVER_ERROR, "Login failed", message)
}

/// A minimal human-readable page for the login legs' failure faces.
fn html_page(status: StatusCode, title: &str, message: &str) -> Response {
    let body = format!(
        "<!doctype html><html><head><title>{title} — Hireling</title></head>\
         <body><h1>{title}</h1><p>{message}</p></body></html>"
    );
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Dev-only session route — compiled out of release builds entirely. Its
// absence in production is part of its definition (spec Assumption, settled
// at design review as a compile-time gate).
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
pub mod dev {
    use super::{
        Arc, AuditEvent, AuditOutcome, AuthState, Cookie, Deserialize, IntoResponse, Response,
        State, StatusCode, account, append_cookie, audit, harden_cookie, html_page, oidc, session,
    };
    use chrono::Utc;

    /// The seats a developer may take. `bruce` and `gm` both map to the GM
    /// seat's test sub; set `HIRELING_GM_SUB=dev-sub-gm` in the dev env.
    pub const SEATS: &[&str] = &["josh", "bear", "dave", "becky", "jake", "bruce", "gm"];

    /// `POST /api/dev/session` — establish a real session as one of the six
    /// seats without a live provider. Everything downstream (session store,
    /// middleware, audit) runs the production path.
    ///
    /// Accepts JSON `{"seat":"bear"}` (curl, tests) and an HTML form
    /// `seat=bear` (the picker page). Seat names need no percent-decoding.
    pub async fn create(State(state): State<Arc<AuthState>>, body: String) -> Response {
        let Some(seat) = parse_seat(&body) else {
            return (
                StatusCode::BAD_REQUEST,
                format!("a seat is required; seats are: {}", SEATS.join(", ")),
            )
                .into_response();
        };
        let Some(sub) = seat_sub(&seat) else {
            return (
                StatusCode::BAD_REQUEST,
                format!("unknown seat {seat:?}; seats are: {}", SEATS.join(", ")),
            )
                .into_response();
        };
        let username = if sub == "dev-sub-gm" {
            "bruce".to_owned()
        } else {
            seat.clone()
        };
        let identity = oidc::AccountIdentity {
            sub: sub.clone(),
            username: username.clone(),
            display_name: username.clone(),
        };
        if let Err(err) = account::upsert(&state.pool, &identity, &state.gm_sub).await {
            tracing::error!(error = %err, "dev session could not upsert account");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        let Ok(session_id) = oidc::random_token() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        if let Err(err) = session::insert(
            &state.pool,
            &session_id,
            &identity.sub,
            Utc::now(),
            state.session_idle_secs,
            state.session_absolute_secs,
        )
        .await
        {
            tracing::error!(error = %err, "dev session could not insert session");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        audit::record(
            &state.pool,
            AuditEvent::LoginSuccess,
            Some(&identity.sub),
            "dev-session",
            AuditOutcome::Allowed,
            None,
        )
        .await;

        let mut cookie = Cookie::new(oidc::SESSION_COOKIE, session_id);
        harden_cookie(&mut cookie);
        let mut response = StatusCode::NO_CONTENT.into_response();
        append_cookie(&mut response, &cookie);
        response
    }

    /// `GET /api/dev/session` — a seat picker so manual dev needs no curl.
    pub async fn picker() -> Response {
        let mut buttons = String::new();
        for seat in SEATS {
            buttons.push_str("<form method=\"post\" action=\"/api/dev/session\" ");
            buttons.push_str("style=\"display:inline\">");
            buttons.push_str("<input type=\"hidden\" name=\"seat\" value=\"");
            buttons.push_str(seat);
            buttons.push_str("\"><button type=\"submit\">");
            buttons.push_str(seat);
            buttons.push_str("</button></form> ");
        }
        html_page(StatusCode::OK, "Dev seats", &buttons)
    }

    /// Seat from a JSON body or a one-field form body.
    fn parse_seat(body: &str) -> Option<String> {
        if let Ok(parsed) = serde_json::from_str::<DevSessionBody>(body) {
            return Some(parsed.seat);
        }
        body.split('&')
            .find_map(|pair| pair.strip_prefix("seat=").map(str::to_owned))
            .filter(|seat| !seat.is_empty())
    }

    fn seat_sub(seat: &str) -> Option<String> {
        match seat {
            "bruce" | "gm" => Some("dev-sub-gm".to_owned()),
            name if SEATS.contains(&name) => Some(format!("dev-sub-{name}")),
            _ => None,
        }
    }

    #[derive(Deserialize)]
    struct DevSessionBody {
        seat: String,
    }
}

#[cfg(debug_assertions)]
pub use dev::SEATS as DEV_SEATS;
