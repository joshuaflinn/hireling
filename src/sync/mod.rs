//! Party sync & realtime (E7): protocol, write engine, snapshots, fan-out,
//! sessions, metrics. See `specs/007-party-sync/` for the binding contracts.

use std::sync::Arc;

use registry::PartyRegistry;

pub mod protocol;
pub mod registry;
pub mod session;
pub mod snapshot;
pub mod write;

/// Runtime state for the sync routes. The router's `State` is E3's
/// `Arc<AuthState>`; sync's own runtime dependency — the fan-out registry
/// — rides the request as an `Extension`, keeping the auth state type
/// untouched (E3 continuity: the WS handshake reuses the same session
/// surface as every REST route).
#[derive(Clone, Default)]
pub struct SyncState {
    pub registry: Arc<PartyRegistry>,
}

impl SyncState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
#[path = "../tests/sync/write.rs"]
mod write_tests;

#[cfg(test)]
#[path = "../tests/sync/snapshot.rs"]
mod snapshot_tests;

#[cfg(test)]
#[path = "../tests/sync/registry.rs"]
mod registry_tests;

#[cfg(test)]
#[path = "../tests/sync/session.rs"]
mod session_tests;
