//! Authentication and authorization: the ownership matrix, sessions, the
//! OIDC legs, the audit trail, and the middleware that binds them to axum.
//!
//! Shape (per the shell/pure split): `authz` and `oidc` are pure; `session`,
//! `account`, `audit`, and `provider` do the I/O; `middleware` and `handlers`
//! are the framework shell. The design lives in
//! `specs/003-authentik-oidc/design.md`.

pub mod account;
pub mod audit;
pub mod authz;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod oidc;
pub mod provider;
pub mod session;

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context as _;
use cookie::Key;
use jsonwebtoken::DecodingKey;
use sqlx::PgPool;

use crate::config::AuthSettings;
use crate::config::OidcSettings;

/// Shared state for every auth layer, extractor, and handler. Cheap to clone
/// via `Arc`.
pub struct AuthState {
    pub pool: PgPool,
    /// HMAC key for the signed OIDC transaction cookie.
    pub cookie_key: Key,
    /// OIDC provider settings; `None` only in debug builds without config.
    pub oidc: Option<Arc<OidcSettings>>,
    /// The `sub`s allowed to hold a session, checked per request so a config
    /// withdrawal denies the account on its next request.
    pub allowlist: HashSet<String>,
    /// Seat-name fallbacks from the allowlist config (`sub=name` entries).
    pub seat_names: HashMap<String, String>,
    /// The `sub` holding the GM seat.
    pub gm_sub: String,
    pub session_idle_secs: i64,
    pub session_absolute_secs: i64,
    /// Dev-session opt-in (see `AuthSettings::dev_sessions`); the legs are
    /// also compile-time debug-only and peer-gated to loopback.
    pub dev_sessions: bool,
    /// HTTP client for the token endpoint and JWKS fetches.
    pub http: reqwest::Client,
    /// JWKS keys by `kid`, cached across requests; refetched when an unknown
    /// `kid` appears (house key rotation).
    pub jwks: tokio::sync::Mutex<HashMap<String, Arc<DecodingKey>>>,
}

impl AuthState {
    /// The seat-name fallback for a `sub`, if the allowlist config paired one.
    pub fn seat_name(&self, sub: &str) -> Option<&str> {
        self.seat_names.get(sub).map(String::as_str)
    }

    /// Build the auth state from settings and a pool. The HTTP client uses
    /// rustls; redirects are never followed on auth calls.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(pool: PgPool, auth: &AuthSettings) -> anyhow::Result<Arc<Self>> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build HTTP client for the OIDC provider")?;
        Ok(Arc::new(Self {
            pool,
            cookie_key: Key::derive_from(&auth.cookie_key),
            oidc: auth.oidc.clone().map(Arc::new),
            allowlist: auth.allowlist.clone(),
            seat_names: auth.seat_names.clone(),
            gm_sub: auth.gm_sub.clone(),
            session_idle_secs: auth.session_idle_secs,
            session_absolute_secs: auth.session_absolute_secs,
            dev_sessions: auth.dev_sessions,
            http,
            jwks: tokio::sync::Mutex::new(HashMap::new()),
        }))
    }
}
