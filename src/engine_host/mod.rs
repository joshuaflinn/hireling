//! Engine host (E8) — the binary crate's glue between the portable
//! `engine/` core and Hireling's storage: base extraction (D3), effect
//! loading, condition mapping, and recompute. No game math lives here —
//! only shape translation and I/O orchestration; the math is the engine's.

pub mod extract;

#[cfg(test)]
#[path = "tests/extract_golden.rs"]
mod extract_golden;
