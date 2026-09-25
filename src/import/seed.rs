//! The human-authored condition tier seed and the engine's closed stat
//! vocabulary.
//!
//! The seed (`data/seed/condition-tiers.json`) is checked in, human-reviewed
//! data (FR-11) — the importer NEVER derives modifier math from the pack
//! documents (Constitution Article IV). A condition with no seed entry
//! defaults to display-only: the fail-safe direction (FR-10).
//!
//! Every stat and modifier type in the seed is validated against the closed
//! vocabulary at load; a bad seed fails the run at startup, never
//! mid-transaction.

use std::collections::HashMap;

use serde::Deserialize;

/// The single stats and rule-exact blanket targets the buff engine (E8)
/// computes over. Blanket targets are names, not wildcards: `all_checks` is
/// every d20 roll (not damage/speed), `all_dcs` is `ac + spell_dc +
/// class_dc`, and `all_checks_and_dcs` is the union.
pub const SINGLE_STATS: [&str; 11] = [
    "ac",
    "fort",
    "ref",
    "will",
    "perception",
    "speed",
    "attack",
    "damage",
    "spell_attack",
    "spell_dc",
    "class_dc",
];

/// The blanket targets with their defined expansion sets.
pub const BLANKET_STATS: [&str; 3] = ["all_checks", "all_dcs", "all_checks_and_dcs"];

/// The four modifier types the engine stacks over.
pub const MODIFIER_TYPES: [&str; 4] = ["circumstance", "status", "item", "untyped"];

/// Which tier a condition landed in — stored corpus data, never derived by
/// consumers (FR-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Fully expressible in the closed vocabulary; carries mapping rows.
    EngineMath,
    /// Not (fully) expressible; tracked manually, zero modifier rows.
    DisplayOnly,
}

impl Tier {
    /// The stored value in the row's `data.import.tier`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::EngineMath => "engine_math",
            Tier::DisplayOnly => "display_only",
        }
    }
}

/// Where a modifier's value comes from. The payload makes the invariant
/// a type: a condition-value mapping cannot exist without its sign, so
/// the writer can never emit the corrupt signless row the review found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// A fixed integer (negative = penalty).
    Constant,
    /// The condition's own value (frightened 1..4 share one mapping; FR-9)
    /// scaled by the stored sign.
    ConditionValue(Polarity),
}

impl ValueKind {
    /// The stored value in a mapping row's `value_kind`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ValueKind::Constant => "constant",
            ValueKind::ConditionValue(_) => "condition_value",
        }
    }
}

/// The sign a parameterized mapping applies to the condition's value.
/// Stored data, never derived: `status` names a stacking type, not a
/// direction, so "frightened 2 is −2" is only knowable because the seed
/// says so (review finding — a positive frightened value must not be able
/// to read as a bonus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    /// The condition's value applies as a bonus (`+value`).
    Positive,
    /// The condition's value applies as a penalty (`−value`).
    Negative,
}

impl Polarity {
    /// The stored value in a mapping row's `polarity`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Polarity::Positive => "positive",
            Polarity::Negative => "negative",
        }
    }
}

/// One condition→modifier mapping from the seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierSeed {
    /// One of [`MODIFIER_TYPES`].
    pub modifier_type: String,
    /// One of the closed vocabulary stats, or `skill:<name>`.
    pub stat: String,
    /// Where the value comes from — for [`ValueKind::ConditionValue`], the
    /// sign rides in the variant.
    pub value_kind: ValueKind,
    /// Set iff `value_kind` is [`ValueKind::Constant`] — the signed value.
    pub value: Option<i64>,
}

/// A seed entry for one condition.
#[derive(Debug, Clone)]
pub struct ConditionSeed {
    /// Upstream `_id` the entry keys on.
    pub upstream_id: String,
    /// Carried for human readability of the seed file.
    pub name: String,
    /// The tier verdict.
    pub tier: Tier,
    /// The seed author's expectation that the condition is valued; a
    /// mismatch with the upstream document becomes a run-report warning.
    pub valued: Option<bool>,
    /// Mappings — empty iff tier is display-only (all-or-nothing; FR-10).
    pub modifiers: Vec<ModifierSeed>,
}

/// The loaded, validated tier seed.
#[derive(Debug, Clone)]
pub struct TierSeed {
    by_upstream_id: HashMap<String, ConditionSeed>,
}

impl TierSeed {
    /// The seed entry for an upstream condition id, if authored.
    #[must_use]
    pub fn get(&self, upstream_id: &str) -> Option<&ConditionSeed> {
        self.by_upstream_id.get(upstream_id)
    }

    /// Number of authored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_upstream_id.len()
    }

    /// Whether any entry is authored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_upstream_id.is_empty()
    }
}

/// The seed failed validation — it is repo data, so this fails at load.
#[derive(Debug)]
pub struct SeedError {
    /// What was violated and where.
    pub problem: String,
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "condition tier seed: {}", self.problem)
    }
}

impl std::error::Error for SeedError {}

#[derive(Debug, Deserialize)]
struct SeedFile {
    conditions: Vec<SeedEntry>,
}

#[derive(Debug, Deserialize)]
struct SeedEntry {
    upstream_id: String,
    name: String,
    tier: String,
    #[serde(default)]
    valued: Option<bool>,
    #[serde(default)]
    modifiers: Vec<RawModifier>,
}

#[derive(Debug, Deserialize)]
struct RawModifier {
    modifier_type: String,
    stat: String,
    value_kind: String,
    #[serde(default)]
    value: Option<i64>,
    #[serde(default)]
    polarity: Option<String>,
}

/// Load and validate the tier seed from its JSON text.
///
/// # Errors
///
/// Returns a [`SeedError`] for invalid JSON, unknown tiers/types/kinds,
/// stats outside the closed vocabulary, engine-math entries without
/// mappings, display-only entries with mappings, constant mappings without
/// a value, parameterized mappings with one, or duplicate upstream ids.
pub fn load_seed(json: &str) -> Result<TierSeed, SeedError> {
    let file: SeedFile = serde_json::from_str(json).map_err(|err| SeedError {
        problem: format!("invalid JSON: {err}"),
    })?;
    let mut by_upstream_id = HashMap::new();
    for entry in file.conditions {
        let seed = validate_entry(entry)?;
        if by_upstream_id
            .insert(seed.upstream_id.clone(), seed)
            .is_some()
        {
            return Err(SeedError {
                problem: "duplicate upstream_id in seed — each condition maps once".to_owned(),
            });
        }
    }
    Ok(TierSeed { by_upstream_id })
}

/// Validate one stat against the closed vocabulary.
///
/// # Errors
///
/// Returns an error naming the offending stat for anything outside the
/// vocabulary (single stats, blanket targets, `skill:<name>`).
pub fn validate_stat(stat: &str) -> Result<(), SeedError> {
    if SINGLE_STATS.contains(&stat) || BLANKET_STATS.contains(&stat) {
        return Ok(());
    }
    if stat
        .strip_prefix("skill:")
        .is_some_and(|skill| !skill.is_empty())
    {
        return Ok(());
    }
    Err(SeedError {
        problem: format!(
            "stat `{stat}` is outside the closed vocabulary (single stats, blanket targets, or `skill:<name>`)"
        ),
    })
}

fn validate_entry(entry: SeedEntry) -> Result<ConditionSeed, SeedError> {
    if entry.upstream_id.is_empty() {
        return Err(SeedError {
            problem: format!("entry `{}` has an empty upstream_id", entry.name),
        });
    }
    let tier = match entry.tier.as_str() {
        "engine_math" => Tier::EngineMath,
        "display_only" => Tier::DisplayOnly,
        other => {
            return Err(SeedError {
                problem: format!("entry `{}` has unknown tier `{other}`", entry.name),
            });
        }
    };
    let mut modifiers = Vec::new();
    for raw in entry.modifiers {
        modifiers.push(validate_modifier(&entry.name, raw)?);
    }
    match (tier, modifiers.is_empty()) {
        (Tier::EngineMath, true) => {
            return Err(SeedError {
                problem: format!(
                    "entry `{}` is engine_math but carries no modifier mappings",
                    entry.name
                ),
            });
        }
        (Tier::DisplayOnly, false) => {
            return Err(SeedError {
                problem: format!(
                    "entry `{}` is display_only but carries modifier mappings — expressibility is all-or-nothing",
                    entry.name
                ),
            });
        }
        (Tier::EngineMath, false) | (Tier::DisplayOnly, true) => {}
    }
    Ok(ConditionSeed {
        upstream_id: entry.upstream_id,
        name: entry.name,
        tier,
        valued: entry.valued,
        modifiers,
    })
}

/// Validate one seed mapping against the vocabularies and the shape
/// rules: constants carry a signed value and no polarity; condition-value
/// mappings carry a polarity and no value (the sign is the polarity).
fn validate_modifier(entry_name: &str, raw: RawModifier) -> Result<ModifierSeed, SeedError> {
    if !MODIFIER_TYPES.contains(&raw.modifier_type.as_str()) {
        return Err(SeedError {
            problem: format!(
                "entry `{entry_name}` has unknown modifier_type `{}`",
                raw.modifier_type
            ),
        });
    }
    validate_stat(&raw.stat)?;
    let value_kind = match raw.value_kind.as_str() {
        "constant" => {
            if raw.polarity.is_some() {
                return Err(SeedError {
                    problem: format!(
                        "entry `{entry_name}` mapping to `{}` is constant and carries its own \
                         signed value — remove the redundant polarity",
                        raw.stat
                    ),
                });
            }
            if raw.value.is_none() {
                return Err(SeedError {
                    problem: format!(
                        "entry `{entry_name}` mapping to `{}` is constant but has no value",
                        raw.stat
                    ),
                });
            }
            ValueKind::Constant
        }
        "condition_value" => {
            if raw.value.is_some() {
                return Err(SeedError {
                    problem: format!(
                        "entry `{entry_name}` mapping to `{}` is condition-valued but carries a value",
                        raw.stat
                    ),
                });
            }
            let raw_polarity = raw.polarity.ok_or_else(|| SeedError {
                problem: format!(
                    "entry `{entry_name}` mapping to `{}` is condition-valued but has no polarity — \
                     the sign is stored data, never derived (use positive or negative)",
                    raw.stat
                ),
            })?;
            let polarity = match raw_polarity.as_str() {
                "positive" => Polarity::Positive,
                "negative" => Polarity::Negative,
                other => {
                    return Err(SeedError {
                        problem: format!(
                            "entry `{entry_name}` mapping to `{}` has unknown polarity `{other}` \
                             (expected positive or negative)",
                            raw.stat
                        ),
                    });
                }
            };
            ValueKind::ConditionValue(polarity)
        }
        other => {
            return Err(SeedError {
                problem: format!("entry `{entry_name}` has unknown value_kind `{other}`"),
            });
        }
    };
    Ok(ModifierSeed {
        modifier_type: raw.modifier_type,
        stat: raw.stat,
        value_kind,
        value: raw.value,
    })
}

#[cfg(test)]
#[path = "tests/seed.rs"]
mod tests;
