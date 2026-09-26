//! Server-side input caps (FR-3): body size and JSON nesting depth.
//!
//! Both caps sit far above any legitimate export (the frozen fixture is
//! 5,493 bytes at depth 8) and below every hostile one: `DoS` hygiene, not
//! correctness. They are enforced here, server-side, before any parsing
//! work that matters, regardless of client-side checks.
//!
//! Pure module: `serde_json::Value` in, verdict out, no I/O.

use serde_json::Value;

use crate::pbimport::error::ImportError;

/// The request-body size cap: 1 MiB (1,048,576 bytes).
pub const MAX_BODY_BYTES: usize = 1_048_576;

/// The JSON nesting-depth cap: 64 levels.
pub const MAX_DEPTH: usize = 64;

/// Reject a body over [`MAX_BODY_BYTES`] (checked before parsing, FR-3).
///
/// # Errors
///
/// [`ImportError::PayloadTooLarge`] when `len` exceeds the cap.
pub fn check_size(len: usize) -> Result<(), ImportError> {
    if len > MAX_BODY_BYTES {
        Err(ImportError::PayloadTooLarge)
    } else {
        Ok(())
    }
}

/// Measure the nesting depth of a JSON document.
///
/// Depth counts every object/array level on the longest path; a scalar leaf
/// is depth 1, an empty container is depth 1, and each container level adds
/// one. The frozen fixture measures 8 (`contracts/pb-export.md` §6).
///
/// The recursion is bounded: `serde_json` itself refuses to parse documents
/// deeper than its own 128-level limit, so the walk can never see anything
/// deeper than that — no stack-exhaustion path exists through parsed input.
#[must_use]
pub fn measure_depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(measure_depth).max().unwrap_or(0),
        Value::Object(entries) => 1 + entries.values().map(measure_depth).max().unwrap_or(0),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 1,
    }
}

/// Reject a parsed document nested deeper than [`MAX_DEPTH`] (FR-3).
///
/// # Errors
///
/// [`ImportError::PayloadTooDeep`] when the document's nesting exceeds the cap.
pub fn check_depth(value: &Value) -> Result<(), ImportError> {
    if measure_depth(value) > MAX_DEPTH {
        Err(ImportError::PayloadTooDeep)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/caps.rs"]
mod tests;
