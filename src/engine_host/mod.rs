//! Engine host (E8) — the binary crate's glue between the portable
//! `engine/` core and Hireling's storage: base extraction (D3), effect
//! loading, condition mapping, and recompute. No game math lives here —
//! only shape translation and I/O orchestration; the math is the engine's.

pub mod apply;
pub mod extract;
pub mod load;
pub mod recompute;
pub mod rest;

use hireling_engine::model::Modifier;
use hireling_engine::vocab::{ModifierType, Stat};

/// Parse one stored or submitted `(type, stat, value)` row into an engine
/// [`Modifier`]. One validator for every text that claims to name a stat or
/// modifier type — the load path and the write path share it (one source,
/// D2). Loud on anything outside the closed vocabulary: the caller decides
/// whether that is a rejected write or a corrupt-store error, but no path
/// drops the row silently.
///
/// # Errors
///
/// A human-readable message naming the offending text — the engine's own
/// reason for stats, a listed alternative for types.
pub fn parse_modifier_row(
    context: impl std::fmt::Display,
    modifier_type_text: &str,
    stat_text: &str,
    value: i32,
) -> anyhow::Result<Modifier> {
    let modifier_type = ModifierType::parse(modifier_type_text).ok_or_else(|| {
        anyhow::anyhow!(
            "{context}: modifier type `{modifier_type_text}` is outside the closed vocabulary \
             (circumstance, status, item, untyped)"
        )
    })?;
    let stat = Stat::parse(stat_text)
        .map(|parsed| parsed.stat_name())
        .map_err(|reason| anyhow::anyhow!("{context}: {reason}"))?;
    Ok(Modifier {
        modifier_type,
        stat,
        value,
    })
}

#[cfg(test)]
#[path = "tests/extract_golden.rs"]
mod extract_golden;

#[cfg(test)]
#[path = "tests/apply.rs"]
mod apply_tests;

#[cfg(test)]
#[path = "tests/load.rs"]
mod load_tests;
