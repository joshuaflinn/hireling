//! Unit tests for archive extraction and pack parsing.

use super::*;

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

#[test]
fn extracts_both_categories_and_ignores_everything_else() {
    let zip = build_zip(vec![
        (
            "packs/conditions.json",
            captured_array(&["frightened.json", "concealed.json"]),
        ),
        ("packs/equipment.json", captured_array(&["wayfinder.json"])),
        ("packs/spells.json", b"[{\"_id\": \"nope\"}]".to_vec()),
        ("lang/en.json", b"{}".to_vec()),
        ("packs/sf2e-stuff.json", b"[]".to_vec()),
    ]);
    let categories = extract_categories(&zip).expect("well-formed archive extracts");
    assert_eq!(categories.len(), 2, "exactly the two imported categories");
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
    let zip = build_zip(vec![
        ("packs/conditions.json", b"[]".to_vec()),
        ("packs/equipment.json", captured_array(&["wayfinder.json"])),
    ]);
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
