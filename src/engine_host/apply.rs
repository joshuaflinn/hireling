//! Condition mapping resolution (D7 / FR-8): a stored corpus row → the
//! resolved, signed modifiers an effect freezes at apply time. Pure shape
//! translation — the tier and the value expectations come from the corpus
//! data, never code (zero condition names here, SC-5). No I/O: the write
//! path (Task 7) owns the transaction around this.

use hireling_engine::model::Modifier;
use hireling_engine::vocab::{ModifierType, Stat};
use serde_json::Value;

use crate::import::transform::resolve_mapping_value;

/// What applying a corpus condition resolves to: the signed modifiers to
/// store on the effect (array order = ord), and whether the row is a
/// display-only badge (`tracked_manually`, zero math — US-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCondition {
    pub modifiers: Vec<Modifier>,
    pub tracked_manually: bool,
}

/// Resolution refused: the corpus row (or the call) cannot be read as
/// stored. Surfaced to the write path as a human-readable `rejected`
/// reason — a corrupt row is loud at apply, never a silent one (FR-8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError {
    /// What was violated and where, human-readable.
    pub problem: String,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "condition apply: {}", self.problem)
    }
}

impl std::error::Error for ApplyError {}

fn err(problem: impl Into<String>) -> ApplyError {
    ApplyError {
        problem: problem.into(),
    }
}

/// Resolve a condition's stored corpus shape into modifiers.
///
/// - Mappings absent or empty ⇒ display-only (FR-8's fail-safe direction):
///   `tracked_manually=true`, zero modifiers — whatever the tier label
///   says, the data rules.
/// - Mappings present ⇒ engine math: each mapping row resolves as stored —
///   `constant` carries its own signed value; `condition_value` scales the
///   supplied value by its polarity (the sign is stored data, never
///   derived). The tier label must agree (`engine_math`, or absent on a
///   legacy row): a disagreement is corruption and stays loud.
///
/// # Errors
///
/// An [`ApplyError`] naming the offending row for: an unknown tier label,
/// an unknown modifier type, a stat outside the closed vocabulary, an
/// unresolvable value shape (unknown kind, missing polarity/constant
/// value), a valued mapping applied without a value, or a value that does
/// not fit `i32`.
pub fn apply_condition(
    tier: Option<&str>,
    modifiers: Option<&Value>,
    condition_value: Option<i64>,
) -> Result<ResolvedCondition, ApplyError> {
    // A jsonb NULL decodes as Some(Value::Null) on some sqlx paths — it
    // reads exactly like an absent column: display-only either way.
    let Some(rows) = modifiers.filter(|value| !value.is_null()) else {
        return Ok(display_only());
    };
    let Some(rows) = rows.as_array() else {
        return Err(err(
            "stored modifiers is not a JSON array — corrupt corpus row",
        ));
    };
    if rows.is_empty() {
        return Ok(display_only());
    }
    match tier {
        Some("engine_math") | None => {}
        Some(other) => {
            return Err(err(format!(
                "corpus row carries mappings but its tier label is `{other}` — the importer's \
                 all-or-nothing rule makes this unstorable; the row is corrupt"
            )));
        }
    }
    let mut resolved = Vec::with_capacity(rows.len());
    for (ord, row) in rows.iter().enumerate() {
        resolved.push(resolve_row(ord, row, condition_value)?);
    }
    Ok(ResolvedCondition {
        modifiers: resolved,
        tracked_manually: false,
    })
}

/// The display-only verdict: badge chip, zero math (US-3).
fn display_only() -> ResolvedCondition {
    ResolvedCondition {
        modifiers: Vec::new(),
        tracked_manually: true,
    }
}

/// Resolve one stored mapping row. The value reading goes through E4's
/// `resolve_mapping_value` — the canonical reader beside the writer, so
/// the two shapes cannot drift.
fn resolve_row(
    ord: usize,
    row: &Value,
    condition_value: Option<i64>,
) -> Result<Modifier, ApplyError> {
    let modifier_type_text = row.get("type").and_then(Value::as_str).ok_or_else(|| {
        err(format!(
            "mapping row {ord} has no modifier type — corrupt corpus row"
        ))
    })?;
    let modifier_type = ModifierType::parse(modifier_type_text).ok_or_else(|| {
        err(format!(
            "mapping row {ord} has unknown modifier type `{modifier_type_text}` \
             (expected circumstance, status, item, or untyped)"
        ))
    })?;
    let stat_text = row.get("stat").and_then(Value::as_str).ok_or_else(|| {
        err(format!(
            "mapping row {ord} has no stat — corrupt corpus row"
        ))
    })?;
    let stat = Stat::parse(stat_text)
        .map(|parsed| parsed.stat_name())
        .map_err(|reason| err(format!("mapping row {ord}: {reason}")))?;
    // A `condition_value` mapping applied without a value has nothing to
    // scale — apply never defaults the condition's value (no `× 0`).
    let is_parameterized = matches!(
        row.get("value_kind").and_then(Value::as_str),
        Some("condition_value")
    );
    if is_parameterized && condition_value.is_none() {
        return Err(err(format!(
            "mapping row {ord} to `{stat}` is condition-valued but no value was supplied"
        )));
    }
    let value =
        resolve_mapping_value(row, condition_value.unwrap_or_default()).ok_or_else(|| {
            err(format!(
                "mapping row {ord} to `{stat}` cannot be resolved as stored — a parameterized row \
             without its polarity or a constant without a value is corrupt"
            ))
        })?;
    let value = i32::try_from(value).map_err(|source| {
        err(format!(
            "mapping row {ord} resolves to {value}, which does not fit the engine's i32 range ({source})"
        ))
    })?;
    Ok(Modifier {
        modifier_type,
        stat,
        value,
    })
}
