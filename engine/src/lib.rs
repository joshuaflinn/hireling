//! The buff/effect modifier engine — Hireling epic E8.
//!
//! A pure, portable core: base stats and active effects in; derived totals,
//! full provenance (applied AND suppressed), and effect chips out. The
//! boundary is the business goal (spec FR-1): zero imports of UI, transport,
//! HTTP, WebSocket, or database machinery — a port to another host is a
//! compile of this crate plus new glue, never a rewrite. The binary crate's
//! `engine_host` module is that glue for Hireling; `cargo tree
//! -p hireling-engine --edges normal` showing serde-family only is the
//! review criterion, asserted in CI.
//!
//! Module map:
//! - [`vocab`]: the closed stat vocabulary — single stats, blanket targets,
//!   `skill:<name>` — and the expansion data the expander consumes.
//! - [`model`]: the wire-facing serde types (the engine-output contract).
//! - [`stack`]: expand → group → select → total, with provenance.
//! - [`compute`]: `EngineOutput` assembly per character.
//!
//! Everything here is total and deterministic: same inputs, byte-identical
//! output, no I/O, no clock, no randomness.

pub mod compute;
pub mod model;
pub mod stack;
pub mod vocab;
