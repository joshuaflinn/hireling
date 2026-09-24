//! Unit tests for the pure transform — the write-plan heart.

use super::*;
use crate::import::SEED_JSON;
use crate::import::model::parse_doc;

fn nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

fn doc_from_fixture(name: &str) -> PackDoc {
    let path = format!("{}/tests/fixtures/packs/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {name} must be readable: {err}"));
    let value: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid JSON");
    parse_doc(Kind::Condition, &value).expect("fixture validates as a condition")
}

fn empty_existing() -> ExistingRows {
    ExistingRows::new()
}

fn existing_row(hash: &str, version: i64) -> ExistingRow {
    ExistingRow {
        name: "Frightened".to_owned(),
        content_hash: hash.to_owned(),
        importer_version: version,
    }
}

fn real_seed() -> TierSeed {
    seed_from_json(SEED_JSON)
}

fn seed_from_json(json: &str) -> TierSeed {
    crate::import::seed::load_seed(json).expect("test seed must be valid")
}

fn seed_entry(upstream_id: &str, tier: &str, valued: Option<bool>) -> String {
    let valued_json = match valued {
        Some(flag) => format!("\"valued\": {flag},"),
        None => String::new(),
    };
    format!(
        r#"{{"conditions": [{{
            "upstream_id": "{upstream_id}",
            "name": "Frightened",
            "tier": "{tier}",
            {valued_json}
            "modifiers": [
                {{"modifier_type": "status", "stat": "all_checks_and_dcs",
                 "value_kind": "condition_value"}}
            ]
        }}]}}"#
    )
}

#[test]
fn clean_import_plans_inserts() {
    let frightened = doc_from_fixture("frightened.json");
    let concealed = doc_from_fixture("concealed.json");
    let docs = vec![frightened, concealed];
    let plan = plan_category(
        Kind::Condition,
        &docs,
        &empty_existing(),
        Some(&real_seed()),
    );
    assert_eq!(plan.inserts.len(), 2, "both docs insert on a clean corpus");
    assert!(plan.updates.is_empty(), "nothing to update yet");
    assert_eq!(plan.skipped, 0, "nothing to skip yet");
    assert!(plan.stale.is_empty(), "no stale rows on a clean corpus");
}

#[test]
fn seedless_conditions_land_display_only_and_are_reported() {
    let frightened = doc_from_fixture("frightened.json");
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&seed_from_json(r#"{"conditions": []}"#)),
    );
    assert_eq!(
        plan.unmapped,
        vec!["Frightened".to_owned()],
        "unmapped conditions are reported"
    );
    let write = plan.inserts.first().expect("insert planned");
    assert_eq!(
        nested_str(&write.data, &["import", "tier"]),
        Some("display_only"),
        "unmapped conditions land display-only — the fail-safe direction (FR-10)"
    );
    assert!(
        write.modifiers.is_none(),
        "display-only means zero mapping rows"
    );
}

#[test]
fn engine_math_condition_carries_vocabulary_only_parameterized_rows() {
    let frightened = doc_from_fixture("frightened.json");
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
    );
    let write = plan.inserts.first().expect("insert planned");
    assert_eq!(
        nested_str(&write.data, &["import", "tier"]),
        Some("engine_math"),
        "seeded engine-math condition is tiered as stored data"
    );
    let modifiers = write
        .modifiers
        .as_ref()
        .expect("engine-math carries mappings");
    let rows = modifiers.as_array().expect("mappings are an array");
    assert_eq!(rows.len(), 1, "one shared parameterized mapping (FR-9)");
    let row = rows.first().expect("one mapping exists");
    assert_eq!(
        row.get("type").and_then(Value::as_str),
        Some("status"),
        "modifier type from the seed"
    );
    assert_eq!(
        row.get("stat").and_then(Value::as_str),
        Some("all_checks_and_dcs"),
        "vocabulary stat from the seed"
    );
    assert_eq!(
        row.get("value_kind").and_then(Value::as_str),
        Some("condition_value"),
        "frightened 1 and frightened 2 share this mapping"
    );
}

#[test]
fn valued_mismatch_warns() {
    let frightened = doc_from_fixture("frightened.json");
    let inverted_seed = seed_from_json(&seed_entry("TBSHQspnbcqxsmjL", "engine_math", Some(false)));
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&inverted_seed),
    );
    assert!(
        !plan.warnings.is_empty(),
        "a seed valued-expectation mismatch must surface in the run report"
    );
}

#[test]
fn same_hash_and_version_skips_without_a_write() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        existing_row(&frightened.content_hash, IMPORTER_VERSION),
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
    );
    assert_eq!(
        plan.skipped, 1,
        "unchanged doc + same importer version skips (FR-3)"
    );
    assert!(
        plan.inserts.is_empty() && plan.updates.is_empty(),
        "a skip must not churn provenance"
    );
}

#[test]
fn changed_content_updates_in_place() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        existing_row("an-older-hash", IMPORTER_VERSION),
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
    );
    assert_eq!(plan.updates.len(), 1, "changed content updates in place");
    assert_eq!(
        plan.updates.first().expect("one update").source_id,
        frightened.source_id,
        "identity is the upstream _id"
    );
}

#[test]
fn importer_version_bump_restamps_unchanged_rows() {
    // Tier-seed changes ship with a version bump; re-running the same
    // release then refreshes tier columns honestly.
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        existing_row(&frightened.content_hash, IMPORTER_VERSION - 1),
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "an importer-version bump re-stamps rows"
    );
}

#[test]
fn absent_upstream_rows_are_reported_stale() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        existing_row(&frightened.content_hash, IMPORTER_VERSION),
    );
    existing.insert(
        "vanishedConditionId".to_owned(),
        ExistingRow {
            name: "Vanished".to_owned(),
            content_hash: "x".to_owned(),
            importer_version: IMPORTER_VERSION,
        },
    );
    let plan = plan_category(Kind::Condition, &[frightened], &existing, None);
    assert_eq!(
        plan.stale.len(),
        1,
        "rows absent upstream are kept and reported (FR-15)"
    );
    assert_eq!(
        plan.stale.first().expect("one stale row").source_id,
        "vanishedConditionId",
        "the stale row is named"
    );
}

#[test]
fn items_ignore_the_seed_and_carry_no_mappings() {
    let path = format!(
        "{}/tests/fixtures/packs/wayfinder.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read_to_string(&path).expect("fixture readable");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid JSON");
    let wayfinder = parse_doc(Kind::Item, &value).expect("fixture validates as an item");
    let plan = plan_category(
        Kind::Item,
        std::slice::from_ref(&wayfinder),
        &empty_existing(),
        Some(&real_seed()),
    );
    let write = plan.inserts.first().expect("insert planned");
    assert!(
        write.modifiers.is_none(),
        "items never carry condition mappings"
    );
    assert!(
        write
            .data
            .get("import")
            .and_then(|import| import.get("tier"))
            .is_none(),
        "tier is a condition-only concept"
    );
    assert_eq!(
        nested_str(&write.data, &["import", "publication", "license"]),
        Some("ORC"),
        "publication provenance rides in data.import"
    );
}

#[test]
fn row_payload_stamps_provenance_per_row() {
    let frightened = doc_from_fixture("frightened.json");
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
    );
    let write = plan.inserts.first().expect("insert planned");
    assert_eq!(
        nested_str(&write.data, &["import", "content_hash"]),
        Some(frightened.content_hash.as_str()),
        "the row carries its own change detector"
    );
    assert_eq!(
        write
            .data
            .get("import")
            .and_then(|import| import.get("importer_version"))
            .and_then(Value::as_i64),
        Some(IMPORTER_VERSION),
        "the row carries the importer version that wrote it"
    );
    assert!(
        write.data.get("upstream").is_some_and(Value::is_object),
        "the raw upstream document is stored for consumers"
    );
}
