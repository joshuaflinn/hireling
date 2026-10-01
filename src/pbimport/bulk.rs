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

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Tenths of Bulk for one corpus document's `system.bulk.value`, or `None`
/// when the document carries no numeric bulk (a corpus gap renders "—").
#[must_use]
pub fn bulk_tenths(doc: &JsonValue) -> Option<i64> {
    tenths(doc.get("system")?.get("bulk")?.get("value")?.as_f64()?)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bulk_tenths_reads_the_foundry_shape_and_refuses_garbage() {
        let doc = |value: f64| json!({"system": {"bulk": {"value": value}}});
        assert_eq!(
            bulk_tenths(&doc(0.0)),
            Some(0),
            "0 is negligible, not a gap"
        );
        assert_eq!(bulk_tenths(&doc(0.1)), Some(1), "L = one tenth");
        assert_eq!(bulk_tenths(&doc(1.0)), Some(10));
        assert_eq!(bulk_tenths(&doc(2.5)), Some(25), "1.5 + 1 = 25 tenths");
        assert_eq!(bulk_tenths(&doc(-1.0)), None, "negative bulk is not bulk");
        assert_eq!(
            bulk_tenths(&json!({"system": {"bulk": {}}})),
            None,
            "missing value is a gap"
        );
        assert_eq!(bulk_tenths(&json!({})), None, "missing system is a gap");
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
