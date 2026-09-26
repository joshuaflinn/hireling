//! The second pure heart: re-import anchoring (FR-10–FR-13, design §4).
//!
//! Live table state survives a re-import by exact anchors: a live slot row
//! is *matched* iff the new export still grants that exact `(caster_key,
//! rank, slot_index)` position — matched rows keep `used` and `prepared`
//! untouched. Positions the new export grants but no live row occupies are
//! seeded from the export (FR-12). Live rows and inventory deltas with no
//! match in the new export are kept and named in the diff — never dropped
//! (FR-11). Live prep wins over export prep on divergence; the divergence is
//! reported. Importing the identical export twice yields zero actions and an
//! empty diff (FR-13).
//!
//! Pure module: snapshots in, a plan of writes out. The store executes the
//! plan inside one transaction.

use std::collections::HashMap;
use std::collections::HashSet;

use serde::Serialize;

use crate::pbimport::transform::BaseSheet;

/// The export's prepared spell at each slot position, keyed by
/// `(caster_key, rank)` and position-indexed (FR-12 seeding source).
#[derive(Debug, Clone, Default)]
pub struct PreparedMap {
    inner: HashMap<(String, i64), Vec<String>>,
}

impl PreparedMap {
    /// Build from a transformed sheet.
    #[must_use]
    pub fn build(sheet: &BaseSheet) -> Self {
        let mut inner = HashMap::new();
        for caster in &sheet.spellcasters {
            for list in &caster.prepared {
                inner.insert((caster.caster_key.clone(), list.rank), list.spells.clone());
            }
        }
        Self { inner }
    }

    /// The prepared spell the export names at one position, if any.
    #[must_use]
    pub fn at(&self, caster_key: &str, rank: i64, slot_index: i64) -> Option<&str> {
        let spells = self.inner.get(&(caster_key.to_owned(), rank))?;
        let index = usize::try_from(slot_index).ok()?;
        spells.get(index).map(String::as_str)
    }
}

/// One live slot-row snapshot (the store's read of `character_spell_slots`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRow {
    pub caster_key: String,
    pub rank: i64,
    pub slot_index: i64,
    pub used: bool,
    pub prepared_spell: Option<String>,
}

/// One live inventory delta (the store's read of `character_inventory_live`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDelta {
    pub name: String,
    pub qty_delta: i64,
}

/// A slot position to INSERT on first import or layout growth (FR-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotSeed {
    pub caster_key: String,
    pub rank: i64,
    pub slot_index: i64,
    pub prepared: Option<String>,
}

/// The post-import diff: a review surface, never a mutation (data-model §5).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Diff {
    pub first_import: bool,
    pub kept_unmatched: Vec<KeptEntry>,
    pub seeded: Vec<SeededSlot>,
    pub prep_divergence: Vec<PrepDivergence>,
    pub notices: Vec<Notice>,
}

impl Diff {
    /// An empty diff; `first_import` responses return all-empty arrays.
    #[must_use]
    pub fn empty(first_import: bool) -> Self {
        Self {
            first_import,
            ..Self::default()
        }
    }
}

/// A live entity the new export no longer matches — kept, never dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeptEntry {
    /// `"slot"` or `"item"`.
    pub kind: &'static str,
    pub caster_key: Option<String>,
    pub rank: Option<i64>,
    pub slot_index: Option<i64>,
    pub used: Option<bool>,
    pub prepared: Option<String>,
    pub name: Option<String>,
    pub qty_delta: Option<i64>,
}

impl KeptEntry {
    /// A vanished slot position (kept row).
    #[must_use]
    pub fn slot(row: &SlotRow) -> Self {
        Self {
            kind: "slot",
            caster_key: Some(row.caster_key.clone()),
            rank: Some(row.rank),
            slot_index: Some(row.slot_index),
            used: Some(row.used),
            prepared: row.prepared_spell.clone(),
            name: None,
            qty_delta: None,
        }
    }

    /// A vanished inventory item (kept delta).
    #[must_use]
    pub fn item(delta: &ItemDelta) -> Self {
        Self {
            kind: "item",
            caster_key: None,
            rank: None,
            slot_index: None,
            used: None,
            prepared: None,
            name: Some(delta.name.clone()),
            qty_delta: Some(delta.qty_delta),
        }
    }
}

/// A new layout position with no live row, seeded from the export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeededSlot {
    pub caster_key: String,
    pub rank: i64,
    pub slot_index: i64,
    pub prepared: Option<String>,
}

/// Live prep disagrees with export prep on a matched slot — live wins (FR-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepDivergence {
    pub caster_key: String,
    pub rank: i64,
    pub slot_index: i64,
    pub live: Option<String>,
    pub export: Option<String>,
}

/// A non-diffing observation riding the import response (data-model §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    /// A delta drives the displayed quantity below zero — surfaced, not
    /// corrected (FR-10).
    NegativeQuantity {
        name: String,
        base_qty: i64,
        delta: i64,
    },
    /// The new export implies a different max HP; clamping is E6/E8 logic.
    MaxHpChanged { old: i64, new: i64 },
    /// A known section whose type drifted; it imported empty (contract §5).
    SectionSkipped { section: String },
}

/// The outcome of anchoring one re-import: slot rows to insert, the diff to
/// report, and how many live rows were left untouched.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnchorPlan {
    pub seeds: Vec<SlotSeed>,
    pub diff: Diff,
    pub unchanged_rows: usize,
}

/// Compute the anchor plan for one re-import (design §4, FR-10–FR-13).
///
/// `layout` is the new export's slot positions (`BaseSheet::slot_layout()`),
/// `export_prepared` its prepared map, `live_slots`/`live_items` the
/// character's current live rows, `base_items` the new export's equipment
/// quantities (name → base qty), and the max-HP pair the old and new
/// derived maxima.
#[must_use]
pub fn anchor(
    layout: &[(String, i64, i64)],
    export_prepared: &PreparedMap,
    live_slots: &[SlotRow],
    live_items: &[ItemDelta],
    base_items: &[(String, i64)],
    old_max_hp: i64,
    new_max_hp: i64,
) -> AnchorPlan {
    let layout_set: HashSet<&(String, i64, i64)> = layout.iter().collect();
    let mut plan = AnchorPlan {
        diff: Diff::empty(false),
        ..AnchorPlan::default()
    };

    // Matched rows keep everything; unmatched rows are kept and diffed.
    for row in live_slots {
        let position = (row.caster_key.clone(), row.rank, row.slot_index);
        if layout_set.contains(&position) {
            plan.unchanged_rows += 1;
            let export_at = export_prepared.at(&row.caster_key, row.rank, row.slot_index);
            if export_at.is_some() && export_at != row.prepared_spell.as_deref() {
                plan.diff.prep_divergence.push(PrepDivergence {
                    caster_key: row.caster_key.clone(),
                    rank: row.rank,
                    slot_index: row.slot_index,
                    live: row.prepared_spell.clone(),
                    export: export_at.map(str::to_owned),
                });
            }
        } else {
            plan.diff.kept_unmatched.push(KeptEntry::slot(row));
        }
    }

    // New positions seed from the export (FR-12).
    let live_positions: HashSet<(String, i64, i64)> = live_slots
        .iter()
        .map(|row| (row.caster_key.clone(), row.rank, row.slot_index))
        .collect();
    for (caster_key, rank, slot_index) in layout {
        let position = (caster_key.clone(), *rank, *slot_index);
        if live_positions.contains(&position) {
            continue;
        }
        let seed = SlotSeed {
            caster_key: caster_key.clone(),
            rank: *rank,
            slot_index: *slot_index,
            prepared: export_prepared
                .at(caster_key, *rank, *slot_index)
                .map(str::to_owned),
        };
        plan.seeds.push(seed.clone());
        plan.diff.seeded.push(SeededSlot {
            caster_key: seed.caster_key,
            rank: seed.rank,
            slot_index: seed.slot_index,
            prepared: seed.prepared,
        });
    }

    // Inventory: vanished items kept and diffed; matched deltas driving the
    // displayed quantity negative are surfaced, not corrected (FR-10/11).
    let base_names: HashSet<&str> = base_items.iter().map(|(name, _)| name.as_str()).collect();
    for delta in live_items {
        if !base_names.contains(delta.name.as_str()) {
            plan.diff.kept_unmatched.push(KeptEntry::item(delta));
        }
    }
    for (name, base_qty) in base_items {
        let delta: i64 = live_items
            .iter()
            .filter(|item| &item.name == name)
            .map(|item| item.qty_delta)
            .sum();
        if base_qty + delta < 0 {
            plan.diff.notices.push(Notice::NegativeQuantity {
                name: name.clone(),
                base_qty: *base_qty,
                delta,
            });
        }
    }

    if old_max_hp != new_max_hp {
        plan.diff.notices.push(Notice::MaxHpChanged {
            old: old_max_hp,
            new: new_max_hp,
        });
    }

    plan
}

#[cfg(test)]
#[path = "tests/anchor.rs"]
mod tests;
