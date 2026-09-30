//! Party sync & realtime (E7): protocol, write engine, snapshots, fan-out,
//! sessions, metrics. See `specs/007-party-sync/` for the binding contracts.

pub mod protocol;
pub mod write;

#[cfg(test)]
#[path = "../tests/sync/write.rs"]
mod write_tests;
