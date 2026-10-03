//! Item Bulk resolution from the items corpus (E6 design §5).
//!
//! `base_sheet` carries no Bulk; E4's corpus does. The bootstrap payload
//! gains `item_bulk: { "<exact item name>": <tenths-of-bulk int | null> }`
//! for the character's imported item names, resolved by exact-name match
//! (case-insensitive, the E12 book-value precedent). Unresolved names ride
//! as `null` — the UI renders "—", never blocking, and the server logs the
//! misses for E4 follow-up. Bulk in the corpus is a Foundry decimal
//! (`0.1` = light); the map carries tenths so the client never sees
//! floating point (`L`=1, `1 Bulk`=10).

use std::collections::BTreeMap;

/// Tenths for one raw Foundry bulk decimal; negative or non-finite values
/// are not bulk.
fn tenths(bulk: f64) -> Option<i64> {
    let scaled = (bulk * 10.0).round();
    if !scaled.is_finite() || scaled < 0.0 || scaled > f64::from(i32::MAX) {
        return None;
    }
    // A character's inventory never approaches ±2\u{b9}\u{b9} tenths; the i32 window
    // keeps every downstream sum well inside i64 and makes the cast exact.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "range-checked to i32 immediately above; the cast is exact"
    )]
    Some(scaled as i64)
}

/// One `(corpus name lowercased, raw bulk decimal)` row as read from
/// `corpus_entries`.
pub type CorpusRow = (String, Option<f64>);

/// The bootstrap map: every requested name keyed, `Some(tenths)` on an
/// exact case-insensitive corpus hit, `None` otherwise. Deterministic
/// order (`BTreeMap` by the sheet's spelling).
#[must_use]
pub fn item_bulk_map(names: &[String], corpus: &[CorpusRow]) -> BTreeMap<String, Option<i64>> {
    names
        .iter()
        .map(|name| {
            let lowered = name.to_lowercase();
            let hit = corpus
                .iter()
                .find(|(candidate, _)| *candidate == lowered)
                .and_then(|(_, bulk)| bulk.as_ref().and_then(|bulk| tenths(*bulk)));
            (name.clone(), hit)
        })
        .collect()
}

/// One `(corpus name lowercased, trait names)` row as read from
/// `corpus_entries`.
pub type TraitRow = (String, Vec<String>);

/// The trait-chip map (spec §2.5: chips render from corpus traits): every
/// requested name keyed, the corpus hit's trait names or an empty list.
#[must_use]
pub fn item_trait_map(names: &[String], corpus: &[TraitRow]) -> BTreeMap<String, Vec<String>> {
    names
        .iter()
        .map(|name| {
            let lowered = name.to_lowercase();
            let hit = corpus
                .iter()
                .find(|(candidate, _)| *candidate == lowered)
                .map(|(_, traits)| traits.clone())
                .unwrap_or_default();
            (name.clone(), hit)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value as JsonValue, json};

    #[test]
    fn tenths_reads_the_foundry_decimal_and_refuses_garbage() {
        let from_doc = |value: f64| {
            item_bulk_map(&["Probe".to_owned()], &[("probe".to_owned(), Some(value))])
                .get("Probe")
                .copied()
                .flatten()
        };
        assert_eq!(from_doc(0.0), Some(0), "0 is negligible, not a gap");
        assert_eq!(from_doc(0.1), Some(1), "L = one tenth");
        assert_eq!(from_doc(1.0), Some(10));
        assert_eq!(from_doc(2.5), Some(25), "1.5 + 1 = 25 tenths");
        assert_eq!(from_doc(-1.0), None, "negative bulk is not bulk");
        assert_eq!(from_doc(f64::NAN), None, "non-finite is not bulk");
        assert_eq!(
            item_bulk_map(&["Probe".to_owned()], &[("probe".to_owned(), None)])
                .get("Probe")
                .copied()
                .flatten(),
            None,
            "a corpus gap is not zero"
        );
        assert_eq!(
            bulk_map_from_doc(&json!({"system": {"bulk": {}}})),
            None,
            "missing value is a gap"
        );
        assert_eq!(
            bulk_map_from_doc(&json!({})),
            None,
            "missing system is a gap"
        );
    }

    /// Drive the JSON-shape branch through the production map path: the
    /// bootstrap resolves `system.bulk.value` from the corpus document.
    fn bulk_map_from_doc(doc: &JsonValue) -> Option<i64> {
        let value = doc
            .get("system")
            .and_then(|system| system.get("bulk"))
            .and_then(|bulk| bulk.get("value"))
            .and_then(JsonValue::as_f64);
        item_bulk_map(&["Probe".to_owned()], &[("probe".to_owned(), value)])
            .get("Probe")
            .copied()
            .flatten()
    }

    #[test]
    fn the_trait_map_follows_the_same_case_rules() {
        let corpus = vec![("chalk".to_owned(), vec!["consumable".to_owned()])];
        let names = ["Chalk".to_owned(), "Bedroll".to_owned()];
        let map = item_trait_map(&names, &corpus);
        assert_eq!(map.get("Chalk"), Some(&vec!["consumable".to_owned()]));
        assert_eq!(map.get("Bedroll"), Some(&Vec::new()), "gaps key empty");
    }

    #[test]
    fn the_map_resolves_case_insensitively_and_leaves_gaps_null() {
        let corpus = vec![
            ("backpack".to_owned(), Some(0.1)),
            ("chalk".to_owned(), Some(0.0)),
        ];
        let names = [
            "Backpack".to_owned(),
            "Chalk".to_owned(),
            "Bedroll".to_owned(),
        ];
        let map = item_bulk_map(&names, &corpus);
        assert_eq!(map.get("Backpack"), Some(&Some(1)));
        assert_eq!(map.get("Chalk"), Some(&Some(0)), "0 is a hit, not a miss");
        assert_eq!(map.get("Bedroll"), Some(&None), "gaps stay null");
        assert_eq!(map.len(), 3, "every name is keyed");
    }
}
