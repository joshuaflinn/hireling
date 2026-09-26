//! The two standard auth failure payloads (spec FR-5, FR-14).
//!
//! Every endpoint answers authentication failure with the same 401 shape and
//! authorization failure with the same 403 shape — byte-identical everywhere,
//! carrying no ownership internals. Tests assert the exact JSON, so drift
//! fails loudly.

use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

/// Authentication required: the session is absent, expired, invalidated, or
/// the account is no longer allowlisted. Clients route the user to re-login.
#[derive(Debug, Clone, Copy)]
pub struct Unauthenticated;

/// Authorization denied: an authenticated actor attempted something the
/// ownership matrix refuses. Deliberately indistinguishable across endpoints
/// and refusal reasons — a non-owner learns nothing about the target.
#[derive(Debug, Clone, Copy)]
pub struct Forbidden;

impl Unauthenticated {
    /// The payload as a raw JSON string, for handlers that write it directly.
    #[must_use]
    pub fn body() -> &'static str {
        r#"{"error":{"code":"unauthenticated","message":"authentication required"}}"#
    }
}

impl Forbidden {
    /// The payload as a raw JSON string, for handlers that write it directly.
    #[must_use]
    pub fn body() -> &'static str {
        r#"{"error":{"code":"forbidden","message":"you do not have permission to do that"}}"#
    }
}

impl IntoResponse for Unauthenticated {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            [(CONTENT_TYPE, "application/json")],
            Self::body(),
        )
            .into_response()
    }
}

impl IntoResponse for Forbidden {
    fn into_response(self) -> Response {
        (
            StatusCode::FORBIDDEN,
            [(CONTENT_TYPE, "application/json")],
            Self::body(),
        )
            .into_response()
    }
}
