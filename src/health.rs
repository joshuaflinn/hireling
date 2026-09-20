//! The health endpoint's payload.
//!
//! Kept pure — no handler plumbing — so the response contract is testable
//! without standing up a server. The endpoint must answer even when every
//! dependency is down, so nothing here may touch the database, the
//! filesystem, or the network.

use serde::Serialize;

/// The crate version, baked in at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Body of `GET /healthz`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HealthPayload {
    /// Always `"ok"` — a 200 with this body means the process is serving.
    pub status: &'static str,
    /// The running build's version.
    pub version: &'static str,
}

/// Build the health payload.
#[must_use]
pub fn health_payload() -> HealthPayload {
    HealthPayload {
        status: "ok",
        version: VERSION,
    }
}

#[cfg(test)]
#[path = "tests/health.rs"]
mod tests;
