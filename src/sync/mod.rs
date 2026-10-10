//! Party sync & realtime (E7): protocol, write engine, snapshots, fan-out,
//! sessions, metrics. See `specs/007-party-sync/` for the binding contracts.

use std::sync::Arc;

use registry::PartyRegistry;

pub mod metrics;
pub mod protocol;
pub mod registry;
pub mod session;
pub mod snapshot;
pub mod write;

/// Runtime state for the sync routes. The router's `State` is E3's
/// `Arc<AuthState>`; sync's own runtime dependencies — the fan-out
/// registry, the liveness timers, and the drain watch — ride the request
/// as an `Extension`, keeping the auth state type untouched (E3
/// continuity: the WS handshake reuses the same session surface as every
/// REST route).
#[derive(Clone)]
pub struct SyncState {
    pub registry: Arc<PartyRegistry>,
    /// Liveness timers; binding values in `Default`, shortened in tests.
    pub settings: SyncSettings,
    /// The E1 drain signal: `true` means the server is draining. Sessions
    /// answer with `bye` + Close 1001. A dropped sender is "no signal".
    pub drain: tokio::sync::watch::Receiver<bool>,
    /// The dispatch-time ring behind `/metrics/sync` (contract §7).
    pub metrics: Arc<metrics::SyncMetrics>,
}

impl SyncState {
    /// Production/default state: binding timers, registry fresh per state.
    #[must_use]
    pub fn new(drain: tokio::sync::watch::Receiver<bool>) -> Self {
        Self {
            registry: Arc::new(PartyRegistry::new()),
            settings: SyncSettings::default(),
            drain,
            metrics: Arc::new(metrics::SyncMetrics::new()),
        }
    }
}

/// Liveness timers (contract §5 — binding: ping 20 s, pong timeout 10 s).
/// Tests shorten both; the numbers themselves are never tuned.
#[derive(Debug, Clone, Copy)]
pub struct SyncSettings {
    pub ping_interval: std::time::Duration,
    pub pong_timeout: std::time::Duration,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            ping_interval: std::time::Duration::from_secs(20),
            pong_timeout: std::time::Duration::from_secs(10),
        }
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
#[path = "../tests/sync/helpers.rs"]
pub(crate) mod test_helpers;

#[cfg(test)]
#[path = "../tests/sync/metrics.rs"]
mod metrics_tests;

#[cfg(test)]
#[path = "../tests/sync/session.rs"]
mod session_tests;

#[cfg(test)]
#[path = "../tests/sync/integration.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "../tests/sync/effects.rs"]
mod effects_tests;

#[cfg(test)]
#[path = "../tests/sync/derived.rs"]
mod derived_tests;

#[cfg(test)]
#[path = "../tests/sync/shaper.rs"]
mod shaper;

#[cfg(test)]
#[path = "../tests/sync/latency.rs"]
mod latency_tests;

#[cfg(test)]
#[path = "../tests/sync/contract.rs"]
mod contract_tests;
