//! The pure transform: validated documents + existing corpus rows + tier
//! seed in; a write plan out. No I/O — exhaustively unit-tested
//! (Constitution Article IV discipline applies to the engine; this module
//! feeds it and holds the same standard).
//!
//! Idempotence model (FR-3): identity is the upstream `source_id`; the
//! change detector is the content hash. A row whose stored hash AND
//! importer version match the incoming document is a skip candidate, but
//! the candidate is only a true no-op when the stored tier AND the stored
//! mapping rows agree with what the seed would write today — so a seed
//! correction (tier or mapping) converges the corpus on the next run even
//! without an importer-version bump, and an incompletely written row is
//! repaired rather than revered. The unmapped-condition gap list is
//! release-vs-seed state: it is collected for every document, so an
//! idempotent re-run's report names the same gaps as the first run
//! (FR-11).

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::import::model::{
    IMPORTER_VERSION, Kind, PackDoc, Publication, is_valued_of, publication_of, slug_of,
};
use crate::import::seed::{Tier, TierSeed, ValueKind};

/// The existing imported rows for one kind, keyed by upstream id.
#[derive(Debug, Clone, Default)]
pub struct ExistingRows {
    by_source_id: HashMap<String, ExistingRow>,
}

impl ExistingRows {
    /// Empty set (clean corpus).
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_source_id: HashMap::new(),
        }
    }

    /// Record one existing row.
    pub fn insert(&mut self, source_id: String, row: ExistingRow) {
        self.by_source_id.insert(source_id, row);
    }

    /// Look up one row.
    #[must_use]
    pub fn get(&self, source_id: &str) -> Option<&ExistingRow> {
        self.by_source_id.get(source_id)
    }

    /// Iterate `(source_id, row)`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ExistingRow)> {
        self.by_source_id.iter()
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_source_id.len()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_source_id.is_empty()
    }
}

/// A row already in the corpus for this kind, as far as the importer cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingRow {
    /// Display name as stored.
    pub name: String,
    /// Stored content hash (empty when the row predates hash stamping —
    /// such rows simply never skip).
    pub content_hash: String,
    /// Stored importer version.
    pub importer_version: i64,
    /// Stored tier (`data.import.tier`); `None` when missing or malformed —
    /// such rows never skip, so the run re-stamps them honestly.
    pub tier: Option<String>,
    /// Stored mapping rows (`modifiers` column); `None` when NULL.
    pub modifiers: Option<Value>,
}

/// One row the plan writes (insert or update — the two carry the same
/// payload).
#[derive(Debug, Clone, PartialEq)]
pub struct RowWrite {
    /// Upstream `_id`.
    pub source_id: String,
    /// Display name.
    pub name: String,
    /// The full `data` jsonb payload (upstream doc + import metadata).
    pub data: Value,
    /// Mapping rows for engine-math conditions; NULL otherwise.
    pub modifiers: Option<Value>,
}

/// What to do about one corpus category this run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CategoryPlan {
    /// Rows to INSERT (not present in the corpus).
    pub inserts: Vec<RowWrite>,
    /// Rows to UPDATE in place (present, changed or re-stamped).
    pub updates: Vec<RowWrite>,
    /// Rows whose hash and importer version match — untouched.
    pub skipped: u64,
    /// Corpus rows for this kind absent from this release — kept, never
    /// deleted, reported for human reconciliation (FR-15).
    pub stale: Vec<StaleRow>,
    /// Imported conditions with no seed entry — they land display-only and
    /// are listed so the seed gets extended by PR (FR-11).
    pub unmapped: Vec<String>,
    /// Non-fatal observations for the run report.
    pub warnings: Vec<String>,
}

/// A kept-but-stale corpus row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRow {
    /// Upstream `_id` of the kept row.
    pub source_id: String,
    /// Its stored name.
    pub name: String,
}

/// Build the write plan for one category.
///
/// `existing` maps `source_id` → stored row for this kind (lane
/// `imported`); `seed` is the tier seed (conditions only).
#[must_use]
pub fn plan_category(
    kind: Kind,
    docs: &[PackDoc],
    existing: &ExistingRows,
    seed: Option<&TierSeed>,
) -> CategoryPlan {
    let mut plan = CategoryPlan::default();
    let mut present_ids: HashSet<&str> = HashSet::new();
    for doc in docs {
        present_ids.insert(doc.source_id.as_str());
        let existing_row = existing.get(doc.source_id.as_str());
        // Compute the payload for every document, skip candidates included:
        // the skip decision itself compares the planned tier and mapping
        // rows against the stored ones (a seed correction must reach rows
        // whose hash, version and tier match), and the unmapped gap list
        // must survive idempotent re-runs — it is release-vs-seed state,
        // not a write side effect.
        let (data, modifiers) = row_payload(kind, doc, seed, &mut plan);
        let write = RowWrite {
            source_id: doc.source_id.clone(),
            name: doc.name.clone(),
            data,
            modifiers,
        };
        let is_noop = existing_row.is_some_and(|row| {
            row.content_hash == doc.content_hash
                && row.importer_version == IMPORTER_VERSION
                && !tier_diverges(kind, doc, seed, row.tier.as_deref())
                && row.modifiers == write.modifiers
        });
        if is_noop {
            plan.skipped += 1;
            continue;
        }
        if existing_row.is_some() {
            plan.updates.push(write);
        } else {
            plan.inserts.push(write);
        }
    }
    for (source_id, row) in existing.iter() {
        if !present_ids.contains(source_id.as_str()) {
            plan.stale.push(StaleRow {
                source_id: source_id.clone(),
                name: row.name.clone(),
            });
        }
    }
    plan.stale.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    plan
}

/// Whether the stored tier disagrees with the seed's current verdict for
/// this condition. Items carry no tier and never diverge.
fn tier_diverges(kind: Kind, doc: &PackDoc, seed: Option<&TierSeed>, stored: Option<&str>) -> bool {
    if kind != Kind::Condition {
        return false;
    }
    let expected = seed
        .and_then(|seed| seed.get(doc.source_id.as_str()))
        .map_or(Tier::DisplayOnly, |entry| entry.tier);
    stored != Some(expected.as_str())
}

/// Build the stored `data` payload and `modifiers` value for one document.
fn row_payload(
    kind: Kind,
    doc: &PackDoc,
    seed: Option<&TierSeed>,
    plan: &mut CategoryPlan,
) -> (Value, Option<Value>) {
    let publication = publication_of(&doc.doc);
    let slug = slug_of(&doc.doc);
    if kind == Kind::Condition {
        let (tier, modifiers) = classify_condition(doc, seed, plan);
        let payload = json!({
            "content_hash": doc.content_hash,
            "importer_version": IMPORTER_VERSION,
            "publication": publication_json(&publication),
            "slug": slug,
            "tier": tier.as_str(),
            "is_valued": is_valued_of(&doc.doc),
        });
        return (json!({ "upstream": doc.doc, "import": payload }), modifiers);
    }
    let payload = json!({
        "content_hash": doc.content_hash,
        "importer_version": IMPORTER_VERSION,
        "publication": publication_json(&publication),
        "slug": slug,
    });
    (json!({ "upstream": doc.doc, "import": payload }), None)
}

/// Tier classification + mapping rows for one condition document.
fn classify_condition(
    doc: &PackDoc,
    seed: Option<&TierSeed>,
    plan: &mut CategoryPlan,
) -> (Tier, Option<Value>) {
    let Some(entry) = seed.and_then(|seed| seed.get(doc.source_id.as_str())) else {
        plan.unmapped.push(doc.name.clone());
        return (Tier::DisplayOnly, None);
    };
    let seed_valued = entry.valued;
    if seed_valued.is_some_and(|expected| expected != is_valued_of(&doc.doc)) {
        plan.warnings.push(format!(
            "seed valued-expectation for `{}` disagrees with the upstream document",
            doc.name
        ));
    }
    if entry.tier == Tier::DisplayOnly {
        return (Tier::DisplayOnly, None);
    }
    let mappings: Vec<Value> = entry
        .modifiers
        .iter()
        .map(|modifier| match modifier.value_kind {
            ValueKind::ConditionValue(polarity) => json!({
                "type": modifier.modifier_type,
                "stat": modifier.stat,
                "value": Value::Null,
                "value_kind": modifier.value_kind.as_str(),
                "polarity": polarity.as_str(),
            }),
            ValueKind::Constant => json!({
                "type": modifier.modifier_type,
                "stat": modifier.stat,
                "value": modifier.value,
            }),
        })
        .collect();
    (Tier::EngineMath, Some(Value::Array(mappings)))
}

/// Resolve one stored mapping row against a condition's value — the
/// canonical reading of the stored `modifiers` shape. `None` means the
/// row cannot be resolved as stored (a parameterized row without
/// polarity, or an unknown value kind): the engine must treat that as
/// corrupt data, never guess a sign.
///
/// This lives beside the writer so the two cannot drift: the transform
/// tests resolve what `plan_category` wrote, proving the stored row alone
/// is sufficient to reproduce frightened 1 → −1 .. 4 → −4.
#[must_use]
pub fn resolve_mapping_value(mapping: &Value, condition_value: i64) -> Option<i64> {
    match mapping.get("value_kind").and_then(Value::as_str)? {
        "constant" => mapping.get("value").and_then(Value::as_i64),
        "condition_value" => {
            let polarity = mapping.get("polarity").and_then(Value::as_str)?;
            match polarity {
                "negative" => Some(-condition_value),
                "positive" => Some(condition_value),
                _ => None,
            }
        }
        _ => None,
    }
}

fn publication_json(publication: &Publication) -> Value {
    json!({
        "license": publication.license,
        "title": publication.title,
        "remaster": publication.remaster,
    })
}

#[cfg(test)]
#[path = "tests/transform.rs"]
mod tests;
