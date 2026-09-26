//! The three transport layers that make deny-by-default structural:
//!
//! `resolve_session` (outermost) → `require_auth` → `gm_read_only` → handler.
//!
//! None of them names an endpoint. `require_auth` guards everything mounted
//! in the protected router; `gm_read_only` rejects every mutating method for
//! the GM role before any handler runs — the Article-I backstop that covers
//! every write path that ever lands inside the nest.

use axum::extract::Request;
use axum::extract::State;
use axum::http::Method;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::response::IntoResponse as _;
use axum::response::Response;
use std::future::Future;
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::audit;
use crate::auth::audit::AuditEvent;
use crate::auth::audit::AuditOutcome;
use crate::auth::authz::Role;
use crate::auth::error::Unauthenticated;
use crate::auth::oidc;
use crate::auth::session;
use crate::http::REQUEST_ID_HEADER;

/// The acting identity, resolved from the server-held session. Attached to
/// every `/api` request as `Option<SessionAccount>`; never taken from request
/// data (spec FR-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAccount {
    pub sub: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
}

impl axum::extract::FromRequestParts<Arc<AuthState>> for SessionAccount {
    type Rejection = Unauthenticated;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &Arc<AuthState>,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        // The lookup is synchronous — no await, so no async wrapper.
        std::future::ready(
            parts
                .extensions
                .get::<Option<SessionAccount>>()
                .cloned()
                .flatten()
                .ok_or(Unauthenticated),
        )
    }
}

/// Layer middleware: cookie → session row → account, attached as
/// `Option<SessionAccount>`. Never rejects. Slides the idle window when the
/// last touch is older than the amortization window.
///
/// The cookie header is fully consumed into an owned id before any await:
/// `Request` is not `Sync`, so holding a borrow of it across the store
/// lookup would make this middleware's future non-`Send`.
pub async fn resolve_session(
    State(state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_id = session_id_from(&request);
    let account = match session_id {
        Some(session_id) => load_session_account(&state, &session_id).await,
        None => None,
    };
    request.extensions_mut().insert(account);
    next.run(request).await
}

/// The opaque session id from the request's cookies, if any.
fn session_id_from(request: &Request) -> Option<String> {
    let cookie_header = request.headers().get(header::COOKIE)?.to_str().ok()?;
    let jar = oidc::jar_from_header(cookie_header);
    jar.get(oidc::SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
}

/// Load the session and its account, or `None` for any absent/expired
/// case. Absent and expired are deliberately the same `None` to the
/// caller — both are just "unauthenticated".
async fn load_session_account(state: &Arc<AuthState>, session_id: &str) -> Option<SessionAccount> {
    let record = match session::find(&state.pool, session_id).await {
        Ok(Some(record)) => record,
        Ok(None) => return None,
        Err(err) => {
            tracing::error!(error = %err, "failed to load session; treating as absent");
            return None;
        }
    };
    if record.is_expired(chrono::Utc::now()) {
        return None;
    }
    if record.needs_renewal(chrono::Utc::now())
        && let Err(err) = session::touch(
            &state.pool,
            &record.id,
            chrono::Utc::now(),
            state.session_idle_secs,
        )
        .await
    {
        tracing::warn!(error = %err, "failed to renew session; lifetime unchanged");
    }
    Some(SessionAccount {
        sub: record.account_sub,
        username: record.username,
        display_name: record.display_name,
        role: record.role,
    })
}

/// Layer middleware: reject any request without a valid, allowlisted session
/// with the standard 401 (spec FR-5/FR-6). Allowlist withdrawal lands here:
/// the check runs against live config on every request, so removing a `sub`
/// denies that account's next request with no session restart.
pub async fn require_auth(
    State(state): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    let account = request.extensions().get::<Option<SessionAccount>>();
    let Some(account) = account.cloned().flatten() else {
        return Unauthenticated.into_response();
    };
    if !state.allowlist.contains(&account.sub) {
        tracing::info!(sub = %account.sub, "session account no longer allowlisted");
        return Unauthenticated.into_response();
    }
    next.run(request).await
}

/// Layer middleware: the GM account writes nothing, anywhere (spec FR-12).
/// Applies to every mutating method on the protected router, independent of
/// any handler, and audit-records each attempt.
pub async fn gm_read_only(
    State(state): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    let is_mutating = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if is_mutating {
        let account = request
            .extensions()
            .get::<Option<SessionAccount>>()
            .cloned()
            .flatten()
            .filter(|account| account.role == Role::Gm);
        if let Some(account) = account {
            let target = request.uri().path().to_owned();
            let request_id = request_id(&request);
            // SC-8 counts every ownership-relevant rejection: if the record
            // cannot be written, the rejection is not delivered — fail closed
            // with a server error rather than a silent un-audited denial.
            if let Err(err) = audit::try_record(
                &state.pool,
                AuditEvent::ForbiddenGmWrite,
                Some(&account.sub),
                &target,
                AuditOutcome::Denied,
                request_id.as_deref(),
            )
            .await
            {
                tracing::error!(
                    error = %err,
                    sub = %account.sub,
                    target = %target,
                    "GM write rejection could not be audit-recorded; failing closed"
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            return crate::auth::error::Forbidden.into_response();
        }
    }
    next.run(request).await
}

/// Gate for the debug-only dev-session legs: requires the explicit
/// `HIRELING_DEV_SESSIONS` opt-in (otherwise 404 — the route reads as absent)
/// and rejects any peer that is not loopback (403). Release builds never
/// mount the route at all, so production is doubly inert.
pub async fn dev_gate(
    State(state): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    if !state.dev_sessions {
        return StatusCode::NOT_FOUND.into_response();
    }
    let peer = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>();
    match peer {
        Some(peer) if peer.0.ip().is_loopback() => next.run(request).await,
        _ => {
            tracing::warn!(
                peer = ?peer.map(|p| p.0),
                "dev-session leg reached by a non-loopback peer; rejected"
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}

/// The correlation id this request already carries (set by the outermost
/// request-id layer), for audit/trace correlation.
pub fn request_id(request: &Request) -> Option<String> {
    request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}
