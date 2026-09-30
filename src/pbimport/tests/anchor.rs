//! Unit tests for the anchoring matrix (FR-10–FR-13): every
//! preserve/keep/seed/diverge case the plan pins, plus idempotency.

use super::{ItemDelta, PreparedMap, SlotRow, anchor};
use crate::pbimport::fixtures::reference_export;
use crate::pbimport::model::parse_and_validate;
use crate::pbimport::transform::transform;

/// Slot-row constructor shorthand.
fn row(
    caster_key: &str,
    rank: i64,
    slot_index: i64,
    used: bool,
    prepared: Option<&str>,
) -> SlotRow {
    SlotRow {
        caster_key: caster_key.to_owned(),
        rank,
        slot_index,
        used,
        prepared_spell: prepared.map(str::to_owned),
    }
}

/// The fixture's sheet, layout, and prepared map.
fn fixture_anchor_inputs() -> (
    Vec<(String, i64, i64)>,
    PreparedMap,
    crate::pbimport::transform::BaseSheet,
) {
    let export = parse_and_validate(reference_export()).expect("fixture validates");
    let (sheet, skips) = transform(&export);
    assert!(skips.sections.is_empty(), "fixture drifts nothing");
    let layout = sheet.slot_layout();
    let prepared = PreparedMap::build(&sheet);
    (layout, prepared, sheet)
}

/// The plan's kept entry at `index`.
fn kept(plan: &super::AnchorPlan, index: usize) -> &super::KeptEntry {
    plan.diff
        .kept_unmatched
        .get(index)
        .unwrap_or_else(|| panic!("kept entry {index} present"))
}

/// The plan's seed at `index`.
fn seed(plan: &super::AnchorPlan, index: usize) -> &super::SlotSeed {
    plan.seeds
        .get(index)
        .unwrap_or_else(|| panic!("seed {index} present"))
}

#[test]
fn all_matched_rows_yield_an_empty_diff_and_zero_seeds() {
    let (layout, prepared, sheet) = fixture_anchor_inputs();
    // Live rows exactly as the first import seeded them.
    let live: Vec<SlotRow> = layout
        .iter()
        .map(|(key, rank, index)| {
            let exported = sheet.prepared_at(key, *rank, *index).map(str::to_owned);
            row(key, *rank, *index, false, exported.as_deref())
        })
        .collect();
    let plan = anchor(&layout, &prepared, &live, &[], &[], 14, 14);
    assert!(plan.seeds.is_empty(), "every position has a live row");
    assert!(plan.diff.kept_unmatched.is_empty(), "nothing vanished");
    assert!(plan.diff.prep_divergence.is_empty(), "no prep disagreement");
    assert!(plan.diff.notices.is_empty(), "no notices");
    assert_eq!(plan.unchanged_rows, layout.len(), "every row matched");
}

#[test]
fn idempotent_reimport_of_the_identical_export_changes_nothing() {
    // FR-13: the same export imported twice — the live rows from the first
    // import (used/prepared as seeded) match the identical layout exactly.
    let (layout, prepared, sheet) = fixture_anchor_inputs();
    let live: Vec<SlotRow> = layout
        .iter()
        .map(|(key, rank, index)| {
            let prepared_at = sheet.prepared_at(key, *rank, *index).map(str::to_owned);
            row(key, *rank, *index, false, prepared_at.as_deref())
        })
        .collect();
    let base_items: Vec<(String, i64)> = sheet
        .equipment
        .iter()
        .map(|item| (item.name.clone(), item.qty))
        .collect();
    let plan = anchor(
        &layout,
        &prepared,
        &live,
        &[],
        &base_items,
        sheet.hp.max_hp,
        sheet.hp.max_hp,
    );
    assert!(plan.seeds.is_empty(), "zero seeds");
    assert!(plan.diff.kept_unmatched.is_empty(), "zero kept entries");
    assert!(plan.diff.seeded.is_empty(), "zero diff seeds");
    assert!(plan.diff.prep_divergence.is_empty(), "zero divergences");
    assert!(plan.diff.notices.is_empty(), "zero notices");
}

#[test]
fn a_vanished_caster_block_keeps_its_rows_and_names_them() {
    let (layout, prepared, _) = fixture_anchor_inputs();
    // The new export (layout) no longer grants any Wellspring Gnome position.
    let wizard_only: Vec<(String, i64, i64)> = layout
        .iter()
        .filter(|(key, _, _)| key == "Wizard")
        .cloned()
        .collect();
    let live = vec![
        row("Wizard", 0, 0, true, Some("Shield")),
        row("Wellspring Gnome", 0, 0, false, None),
    ];
    let plan = anchor(&wizard_only, &prepared, &live, &[], &[], 14, 14);
    assert_eq!(plan.unchanged_rows, 1, "only the Wizard row matches");
    assert_eq!(
        plan.diff.kept_unmatched.len(),
        1,
        "the vanished block's row is named, kept"
    );
    assert_eq!(kept(&plan, 0).kind, "slot");
    assert_eq!(
        kept(&plan, 0).caster_key.as_deref(),
        Some("Wellspring Gnome")
    );
    assert_eq!(kept(&plan, 0).used, Some(false), "kept verbatim");
}

#[test]
fn a_shrunk_rank_keeps_the_beyond_layout_row() {
    let layout = vec![
        ("Wizard".to_owned(), 0, 0),
        ("Wizard".to_owned(), 2, 0),
        ("Wizard".to_owned(), 2, 1),
    ];
    let prepared = PreparedMap::default();
    let live = vec![
        row("Wizard", 0, 0, false, None),
        row("Wizard", 2, 0, true, Some("Blazing Bolt")),
        row("Wizard", 2, 1, false, None),
        row("Wizard", 2, 2, true, Some("Illusory Creature")),
    ];
    let plan = anchor(&layout, &prepared, &live, &[], &[], 14, 14);
    assert_eq!(plan.unchanged_rows, 3, "three matched");
    assert_eq!(
        plan.diff.kept_unmatched.len(),
        1,
        "index 2 beyond new layout"
    );
    assert_eq!(kept(&plan, 0).slot_index, Some(2));
    assert_eq!(kept(&plan, 0).used, Some(true), "kept verbatim");
    assert!(plan.seeds.is_empty(), "a shrunk rank seeds nothing");
}

#[test]
fn a_grown_rank_seeds_the_new_positions() {
    let (_, prepared, _) = fixture_anchor_inputs();
    // The live character has one cantrip slot; the new export grants two.
    let layout = vec![("Wizard".to_owned(), 0, 0), ("Wizard".to_owned(), 0, 1)];
    let live = vec![row("Wizard", 0, 0, true, None)];
    let plan = anchor(&layout, &prepared, &live, &[], &[], 14, 14);
    assert_eq!(plan.unchanged_rows, 1, "index 0 matched");
    assert_eq!(plan.seeds.len(), 1, "index 1 is new");
    assert_eq!(seed(&plan, 0).slot_index, 1);
    assert_eq!(
        seed(&plan, 0).prepared.as_deref(),
        Some("Daze"),
        "the export prepares Daze at cantrip index 1 (FR-12)"
    );
    assert_eq!(plan.diff.seeded.len(), 1, "the seed is diffed");
}

#[test]
fn duplicate_name_tiebreak_is_stable_across_reimports() {
    // The same duplicated-caster export transformed twice must produce the
    // identical layout, so the second import finds every row matched.
    let mutate = |body: String| {
        let mut doc: serde_json::Value = serde_json::from_str(&body).expect("fixture parses");
        doc.as_object_mut()
            .expect("object")
            .get_mut("build")
            .expect("build")
            .as_object_mut()
            .expect("object")
            .insert(
                "spellCasters".to_owned(),
                serde_json::json!([
                    { "name": "Wizard", "perDay": [1,0,0,0,0,0,0,0,0,0,0] },
                    { "name": "Wizard", "perDay": [2,0,0,0,0,0,0,0,0,0,0] }
                ]),
            );
        let mutated = serde_json::to_string(&doc).expect("serializes");
        let export = parse_and_validate(&mutated).expect("validates");
        transform(&export).0.slot_layout()
    };
    let first = mutate(reference_export().to_owned());
    let second = mutate(reference_export().to_owned());
    assert_eq!(first, second, "FR-10: deterministic caster_key assignment");

    let live: Vec<SlotRow> = first
        .iter()
        .map(|(key, rank, index)| row(key, *rank, *index, false, None))
        .collect();
    let plan = anchor(&second, &PreparedMap::default(), &live, &[], &[], 14, 14);
    assert!(plan.seeds.is_empty(), "identical layouts re-match exactly");
    assert_eq!(plan.unchanged_rows, 3, "Wizard + Wizard#2 rows all matched");
}

#[test]
fn the_innate_block_seeds_its_cantrip() {
    let (layout, prepared, _) = fixture_anchor_inputs();
    // First import: no live rows — every position seeds, including the
    // Wellspring Gnome's innate cantrip (unprepared: prepared: [] in export).
    let plan = anchor(&layout, &prepared, &[], &[], &[], 0, 14);
    assert_eq!(
        plan.seeds.len(),
        layout.len(),
        "a full first-import seed set"
    );
    let gnome_seed = plan
        .seeds
        .iter()
        .find(|seed| seed.caster_key == "Wellspring Gnome")
        .expect("innate cantrip seeds");
    assert_eq!(gnome_seed.rank, 0);
    assert_eq!(gnome_seed.slot_index, 0);
    assert_eq!(gnome_seed.prepared, None, "innate block prepares nothing");
}

#[test]
fn prep_divergence_reports_and_live_wins() {
    let (layout, prepared, _) = fixture_anchor_inputs();
    // Live rank-1 slot 0 was prepped "Fear"; the export says "500 Toads".
    let live = vec![row("Wizard", 1, 0, false, Some("Fear"))];
    let plan = anchor(&layout, &prepared, &live, &[], &[], 14, 14);
    assert_eq!(plan.unchanged_rows, 1, "the row itself matches");
    assert_eq!(plan.diff.prep_divergence.len(), 1, "divergence named");
    let divergence = plan
        .diff
        .prep_divergence
        .first()
        .expect("divergence present");
    assert_eq!(divergence.live.as_deref(), Some("Fear"), "live value");
    assert_eq!(
        divergence.export.as_deref(),
        Some("500 Toads"),
        "export value"
    );
    assert!(
        !plan
            .seeds
            .iter()
            .any(|seed| seed.caster_key == "Wizard" && seed.rank == 1 && seed.slot_index == 0),
        "a matched row is never re-seeded (no mutation path)"
    );
}

#[test]
fn a_vanished_item_is_kept_and_named() {
    let (layout, prepared, sheet) = fixture_anchor_inputs();
    let base_items: Vec<(String, i64)> = sheet
        .equipment
        .iter()
        .map(|item| (item.name.clone(), item.qty))
        .collect();
    let live_items = vec![
        ItemDelta {
            name: "Chalk".to_owned(),
            qty_delta: -2,
        },
        ItemDelta {
            name: "Silver Chunk".to_owned(),
            qty_delta: -1,
        },
    ];
    // Silver Chunk vanished from the new export.
    let without_silver: Vec<(String, i64)> = base_items
        .iter()
        .filter(|(name, _)| name != "Silver Chunk")
        .cloned()
        .collect();
    let plan = anchor(
        &layout,
        &prepared,
        &[],
        &live_items,
        &without_silver,
        14,
        14,
    );
    let kept_items: Vec<&super::KeptEntry> = plan
        .diff
        .kept_unmatched
        .iter()
        .filter(|entry| entry.kind == "item")
        .collect();
    assert_eq!(kept_items.len(), 1, "only Silver Chunk is unmatched");
    assert_eq!(
        kept_items.first().and_then(|entry| entry.name.as_deref()),
        Some("Silver Chunk")
    );
    assert_eq!(
        kept_items.first().and_then(|entry| entry.qty_delta),
        Some(-1),
        "kept verbatim"
    );
}

#[test]
fn a_negative_effective_quantity_is_surfaced_not_corrected() {
    let (layout, prepared, sheet) = fixture_anchor_inputs();
    let base_items: Vec<(String, i64)> = sheet
        .equipment
        .iter()
        .map(|item| (item.name.clone(), item.qty))
        .collect();
    // Chalk: base 10 in the fixture; a delta of −11 drives it negative.
    let live_items = vec![ItemDelta {
        name: "Chalk".to_owned(),
        qty_delta: -11,
    }];
    let plan = anchor(&layout, &prepared, &[], &live_items, &base_items, 14, 14);
    let notice = plan
        .diff
        .notices
        .iter()
        .find(|notice| matches!(notice, super::Notice::NegativeQuantity { .. }))
        .expect("negative quantity noticed");
    match notice {
        super::Notice::NegativeQuantity {
            name,
            base_qty,
            delta,
        } => {
            assert_eq!(name, "Chalk");
            assert_eq!(*base_qty, 10);
            assert_eq!(*delta, -11);
        }
        super::Notice::MaxHpChanged { .. } | super::Notice::SectionSkipped { .. } => {
            unreachable!("matched the find above")
        }
    }
    // And at exactly zero it is NOT a notice (10 − 10 = 0 is fine).
    let zero_delta = vec![ItemDelta {
        name: "Chalk".to_owned(),
        qty_delta: -10,
    }];
    let zero_plan = anchor(&layout, &prepared, &[], &zero_delta, &base_items, 14, 14);
    assert!(
        zero_plan
            .diff
            .notices
            .iter()
            .all(|entry| !matches!(entry, super::Notice::NegativeQuantity { .. })),
        "zero displayed quantity is not negative"
    );
}

#[test]
fn a_max_hp_change_across_reimports_is_noticed() {
    let (layout, prepared, _) = fixture_anchor_inputs();
    let plan = anchor(&layout, &prepared, &[], &[], &[], 14, 22);
    assert!(
        plan.diff
            .notices
            .contains(&super::Notice::MaxHpChanged { old: 14, new: 22 }),
        "old and new maxima surfaced (FR-10)"
    );

    let same = anchor(&layout, &prepared, &[], &[], &[], 14, 14);
    assert!(same.diff.notices.is_empty(), "an unchanged max is silent");
}
