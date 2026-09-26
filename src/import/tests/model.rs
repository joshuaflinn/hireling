//! Unit tests for document parsing and schema validation.
//!
//! Fixture documents are verbatim captures from release `pf2e-8.5.1`
//! (see `tests/fixtures/packs/`); broken variants are labeled mutations.

use serde_json::Value;

use super::*;

fn set_type(doc: &mut Value, item_type: &str) {
    doc.as_object_mut()
        .expect("document is an object")
        .insert("type".to_owned(), Value::String(item_type.to_owned()));
}

fn system_object_mut(doc: &mut Value) -> &mut serde_json::Map<String, Value> {
    doc.get_mut("system")
        .and_then(Value::as_object_mut)
        .expect("system is an object")
}

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/packs/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {name} must be readable: {err}"));
    serde_json::from_str(&raw).expect("fixture must be valid JSON")
}

#[test]
fn captured_condition_parses() {
    let doc = parse_doc(Kind::Condition, &fixture("frightened.json"));
    let doc = doc.expect("the captured frightened document must validate");
    assert_eq!(
        doc.source_id, "TBSHQspnbcqxsmjL",
        "identity is the upstream _id"
    );
    assert_eq!(doc.name, "Frightened", "name comes from the document");
}

#[test]
fn captured_item_parses() {
    let doc = parse_doc(Kind::Item, &fixture("wayfinder.json"));
    let doc = doc.expect("the captured wayfinder document must validate");
    assert_eq!(
        doc.source_id, "gbwr57aT9ou8yKWT",
        "identity is the upstream _id"
    );
}

#[test]
fn content_hash_is_stable_and_covers_content() {
    let doc = fixture("frightened.json");
    let first = parse_doc(Kind::Condition, &doc).expect("fixture validates");
    let second = parse_doc(Kind::Condition, &doc).expect("fixture validates");
    assert_eq!(
        first.content_hash, second.content_hash,
        "the same document must hash identically"
    );
    let mut changed = doc.clone();
    changed
        .as_object_mut()
        .expect("document is an object")
        .insert("name".to_owned(), Value::String("Terrified".to_owned()));
    let renamed = parse_doc(Kind::Condition, &changed).expect("mutation validates");
    assert_ne!(
        first.content_hash, renamed.content_hash,
        "a content change must change the hash — it is the whole update decision"
    );
}

#[test]
fn condition_rejects_non_condition_type() {
    let mut doc = fixture("frightened.json");
    set_type(&mut doc, "equipment");
    let err = parse_doc(Kind::Condition, &doc).expect_err("wrong type must fail");
    assert!(
        err.to_string().contains("condition"),
        "error must state the expected type, got: {err}"
    );
}

#[test]
fn item_accepts_all_probed_item_types() {
    let mut doc = fixture("wayfinder.json");
    for item_type in ITEM_TYPES {
        set_type(&mut doc, item_type);
        let parsed = parse_doc(Kind::Item, &doc);
        assert!(
            parsed.is_ok(),
            "item type `{item_type}` was probed in the equipment pack and must validate"
        );
    }
}

#[test]
fn item_rejects_unknown_types() {
    let mut doc = fixture("wayfinder.json");
    set_type(&mut doc, "spell");
    let err = parse_doc(Kind::Item, &doc).expect_err("unknown item types are drift");
    assert!(
        err.to_string().contains("recognized item type"),
        "error must name the closed item type set, got: {err}"
    );
}

#[test]
fn condition_requires_is_valued() {
    let mut doc = fixture("frightened.json");
    system_object_mut(&mut doc)
        .get_mut("value")
        .and_then(Value::as_object_mut)
        .expect("value block exists")
        .remove("isValued");
    let err = parse_doc(Kind::Condition, &doc).expect_err("conditions must carry isValued");
    assert!(
        err.to_string().contains("isValued"),
        "error must name the missing path, got: {err}"
    );
}

#[test]
fn item_does_not_require_level_or_price() {
    // Probed reality: kits carry no level; one legacy doc carries an empty
    // price. They are items with honest provenance, not corrupt docs.
    let mut doc = fixture("wayfinder.json");
    system_object_mut(&mut doc).remove("level");
    system_object_mut(&mut doc).remove("price");
    let parsed = parse_doc(Kind::Item, &doc);
    assert!(
        parsed.is_ok(),
        "level/price are consumer concerns, not import invariants"
    );
}

#[test]
fn missing_publication_fails() {
    let mut doc = fixture("wayfinder.json");
    system_object_mut(&mut doc).remove("publication");
    let err = parse_doc(Kind::Item, &doc).expect_err("publication is required");
    assert!(
        err.to_string().contains("publication"),
        "error must name the missing block, got: {err}"
    );
}

#[test]
fn extraction_helpers_read_the_document() {
    let doc = fixture("frightened.json");
    let publication = publication_of(&doc);
    assert_eq!(publication.license, "ORC", "fixture is ORC-licensed");
    assert!(publication.remaster, "fixture is remaster content");
    assert_eq!(
        publication.title, "Pathfinder Player Core",
        "title comes from the publication block"
    );
    assert!(is_valued_of(&doc), "frightened is a valued condition");
    assert_eq!(
        slug_of(&doc).as_deref(),
        Some("frightened"),
        "the fixture carries system.slug"
    );
}
