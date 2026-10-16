//! Unit tests for archive extraction and pack parsing.

use super::*;
use crate::import::model::ALL_KINDS;

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/packs/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("fixture {name} must be readable: {err}"))
}

/// Build an in-memory zip with the given entries (name → bytes).
fn build_zip(entries: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(name, options)
                .expect("test zip entry starts");
            std::io::Write::write_all(&mut zip, &bytes).expect("test zip entry writes");
        }
        zip.finish().expect("test zip finishes");
    }
    buf.into_inner()
}

fn captured_array(docs: &[&str]) -> Vec<u8> {
    let names: Vec<String> = docs
        .iter()
        .map(|name| format!("{}/tests/fixtures/packs/{name}", env!("CARGO_MANIFEST_DIR")))
        .collect();
    let mut out = String::from("[");
    for (index, path) in names.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&std::fs::read_to_string(path).expect("fixture readable"));
    }
    out.push(']');
    out.into_bytes()
}

/// Every v2 pack file present with one captured document each (creatures:
/// one in Monster Core, the other bestiary files empty arrays).
fn full_entries() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("packs/spells.json", captured_array(&["spell-daze.json"])),
        (
            "packs/feats.json",
            captured_array(&["feat-charming-liar.json"]),
        ),
        (
            "packs/class-features.json",
            captured_array(&["classfeature-school-of-mentalism.json"]),
        ),
        ("packs/actions.json", captured_array(&["action-seek.json"])),
        (
            "packs/pathfinder-monster-core.json",
            captured_array(&["creature-skeleton-guard.json"]),
        ),
        ("packs/pathfinder-monster-core-2.json", b"[]".to_vec()),
        ("packs/pathfinder-npc-core.json", b"[]".to_vec()),
        ("packs/pathfinder-bestiary.json", b"[]".to_vec()),
        ("packs/pathfinder-bestiary-2.json", b"[]".to_vec()),
        ("packs/pathfinder-bestiary-3.json", b"[]".to_vec()),
        (
            "packs/journals.json",
            fixture_bytes("journals-gm-screen.json"),
        ),
    ]
}

#[test]
fn extracts_both_categories_and_ignores_everything_else() {
    let mut entries = full_entries();
    entries.extend(vec![
        (
            "packs/conditions.json",
            captured_array(&["frightened.json", "concealed.json"]),
        ),
        ("packs/equipment.json", captured_array(&["wayfinder.json"])),
        (
            "packs/kingmaker-bestiary.json",
            b"[{\"_id\": \"nope\"}]".to_vec(),
        ),
        ("lang/en.json", b"{}".to_vec()),
        ("packs/sf2e-stuff.json", b"[]".to_vec()),
    ]);
    let zip = build_zip(entries);
    let categories = extract_categories(&zip).expect("well-formed archive extracts");
    assert_eq!(
        categories.len(),
        ALL_KINDS.len(),
        "one category per imported kind"
    );
    let kinds: Vec<Kind> = categories.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, ALL_KINDS.to_vec(), "categories in import order");
    for category in categories.iter().skip(2) {
        assert_eq!(
            category.docs.len(),
            1,
            "{} lands its one captured doc",
            category.kind.plural()
        );
    }
    let conditions = categories.first().expect("two categories exist");
    let items = categories.get(1).expect("two categories exist");
    assert_eq!(conditions.kind, Kind::Condition, "conditions first");
    assert_eq!(conditions.docs.len(), 2, "both condition docs land");
    assert_eq!(items.kind, Kind::Item, "items second");
    assert_eq!(items.docs.len(), 1, "wayfinder lands");
    let ids: Vec<&str> = conditions
        .docs
        .iter()
        .map(|doc| doc.source_id.as_str())
        .collect();
    assert!(
        ids.contains(&"TBSHQspnbcqxsmjL"),
        "the captured frightened document is present verbatim"
    );
}

#[test]
fn missing_pack_files_fail_before_any_write() {
    let zip = build_zip(vec![(
        "packs/conditions.json",
        captured_array(&["frightened.json"]),
    )]);
    let err = extract_categories(&zip).expect_err("a missing equipment pack must fail the run");
    assert!(
        err.to_string().contains("packs/equipment.json"),
        "error must name the missing pack, got: {err}"
    );
}

#[test]
fn zero_document_category_is_extracted_and_fails_at_the_tripwire() {
    // An empty array parses fine here; the run-level tripwire (mod.rs)
    // refuses to plan an empty category. Extraction itself must still
    // reject a MISSING file (above) — the two failure modes are distinct.
    let mut entries = full_entries();
    entries.extend(vec![
        ("packs/conditions.json", b"[]".to_vec()),
        ("packs/equipment.json", captured_array(&["wayfinder.json"])),
    ]);
    let zip = build_zip(entries);
    let categories = extract_categories(&zip).expect("empty array still extracts");
    let conditions = categories.first().expect("both categories exist");
    assert!(conditions.docs.is_empty(), "the empty pack carries no docs");
}

#[test]
fn broken_document_fails_naming_the_doc() {
    let mut frightened: serde_json::Value =
        serde_json::from_slice(&fixture_bytes("frightened.json")).expect("fixture is JSON");
    frightened
        .get_mut("system")
        .and_then(serde_json::Value::as_object_mut)
        .expect("system exists")
        .remove("publication");
    let bytes = serde_json::to_vec(&vec![frightened]).expect("array serializes");
    let err = parse_pack_file(Kind::Condition, "packs/conditions.json", &bytes)
        .expect_err("a schema-drifted document fails validation");
    assert!(
        err.to_string().contains("publication"),
        "error names the violated expectation, got: {err}"
    );
}

#[test]
fn duplicate_ids_within_a_pack_fail() {
    let doc = fixture_bytes("frightened.json");
    let doubled = format!(
        "[{}, {}]",
        String::from_utf8(doc.clone()).expect("utf8"),
        String::from_utf8(doc).expect("utf8")
    );
    let err = parse_pack_file(Kind::Condition, "packs/conditions.json", doubled.as_bytes())
        .expect_err("duplicate upstream ids must fail loudly");
    assert!(
        err.to_string().contains("duplicate upstream id"),
        "error must name the duplication, got: {err}"
    );
}

#[test]
fn non_array_pack_file_fails() {
    let err = parse_pack_file(Kind::Item, "packs/equipment.json", b"{}")
        .expect_err("a pack file is an array of documents");
    assert!(
        err.to_string().contains("JSON array"),
        "error must state the expected shape, got: {err}"
    );
}

#[test]
fn creature_packs_skip_hazards_by_type() {
    let mut hazard: serde_json::Value =
        serde_json::from_slice(&fixture_bytes("creature-skeleton-guard.json"))
            .expect("fixture is JSON");
    let object = hazard.as_object_mut().expect("fixture is an object");
    object.insert("type".into(), serde_json::Value::String("hazard".into()));
    object.insert(
        "_id".into(),
        serde_json::Value::String("hazardXXXXXXXXXX".into()),
    );
    let guard: serde_json::Value =
        serde_json::from_slice(&fixture_bytes("creature-skeleton-guard.json"))
            .expect("fixture is JSON");
    let bytes = serde_json::to_vec(&vec![guard, hazard]).expect("array serializes");
    let docs = parse_pack_file(
        Kind::Creature,
        "packs/pathfinder-monster-core-2.json",
        &bytes,
    )
    .expect("a hazard in a bestiary pack is skipped, not fatal");
    assert_eq!(docs.len(), 1, "only the npc lands");
}

#[test]
fn creature_publication_reads_from_system_details() {
    let guard: serde_json::Value =
        serde_json::from_slice(&fixture_bytes("creature-skeleton-guard.json"))
            .expect("fixture is JSON");
    let publication = crate::import::model::publication_of(&guard);
    assert_eq!(
        publication.license, "ORC",
        "npc publication lives at system.details"
    );
}

#[test]
fn duplicate_creature_ids_across_files_fail() {
    let mut entries = full_entries();
    entries.retain(|(name, _)| *name != "packs/pathfinder-monster-core-2.json");
    entries.extend(vec![
        (
            "packs/conditions.json",
            captured_array(&["frightened.json"]),
        ),
        ("packs/equipment.json", captured_array(&["wayfinder.json"])),
        (
            "packs/pathfinder-monster-core-2.json",
            captured_array(&["creature-skeleton-guard.json"]),
        ),
    ]);
    let err = extract_categories(&build_zip(entries)).expect_err("cross-file id repeat is fatal");
    assert!(
        err.to_string().contains("duplicate upstream id"),
        "got: {err}"
    );
}

#[test]
fn rules_journal_flattens_to_pages_with_synthesized_publication() {
    let docs = parse_pack_file(
        Kind::RulesPage,
        "packs/journals.json",
        &fixture_bytes("journals-gm-screen.json"),
    )
    .expect("the GM Screen journal flattens");
    assert_eq!(docs.len(), 1, "one text page");
    let doc = docs.first().expect("one page");
    assert_eq!(doc.name, "Basic Actions");
    assert_eq!(
        doc.doc.pointer("/system/publication/synthesized"),
        Some(&serde_json::Value::Bool(true)),
        "the stamped publication says it was synthesized"
    );
}
