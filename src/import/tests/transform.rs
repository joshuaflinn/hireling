//! Unit tests for the pure transform — the write-plan heart.

use super::*;
use crate::import::SEED_JSON;
use crate::import::model::parse_doc;

/// The release every pre-existing test row is stamped with; plan calls
/// below import the same release unless a test says otherwise.
const TEST_RELEASE: &str = "pf2e-test-release";

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
        tier: Some("engine_math".to_owned()),
        modifiers: None,
        pack_version: Some(TEST_RELEASE.to_owned()),
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
    let modifiers_json = r#"[{"modifier_type": "status", "stat": "all_checks_and_dcs",
                 "value_kind": "condition_value", "polarity": "negative"}]"#;
    format!(
        r#"{{"conditions": [{{
            "upstream_id": "{upstream_id}",
            "name": "Frightened",
            "tier": "{tier}",
            {valued_json}
            "modifiers": {modifiers_json}
        }}]}}"#
    )
}

/// A one-condition seed whose modifier list is spelled out — display-only
/// entries must carry an empty list (the seed validator enforces that).
fn seed_single(upstream_id: &str, tier: &str, modifiers_json: &str) -> String {
    format!(
        r#"{{"conditions": [{{
            "upstream_id": "{upstream_id}",
            "name": "Frightened",
            "tier": "{tier}",
            "modifiers": {modifiers_json}
        }}]}}"#
    )
}

const FRIGHTENED_ID: &str = "TBSHQspnbcqxsmjL";

/// The stored modifier rows of the first write a plan produces, for use as
/// an existing row's mapping payload in skip-gate tests.
fn planned_modifiers(plan: &CategoryPlan) -> Option<Value> {
    plan.inserts
        .first()
        .expect("insert planned")
        .modifiers
        .clone()
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
        TEST_RELEASE,
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
        TEST_RELEASE,
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
        TEST_RELEASE,
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
    assert_eq!(
        row.get("polarity").and_then(Value::as_str),
        Some("negative"),
        "the stored row carries the sign explicitly — `status` is a stacking \
         type, not a sign, and a positive frightened value must not read as a bonus"
    );
}

#[test]
fn frightened_values_resolve_to_signed_penalties() {
    let frightened = doc_from_fixture("frightened.json");
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
        TEST_RELEASE,
    );
    let mapping = planned_modifiers(&plan)
        .expect("engine-math carries mappings")
        .as_array()
        .expect("mappings are an array")
        .first()
        .expect("one mapping")
        .clone();
    assert_eq!(
        resolve_mapping_value(&mapping, 1),
        Some(-1),
        "frightened 1 is −1, straight from the stored mapping"
    );
    assert_eq!(
        resolve_mapping_value(&mapping, 2),
        Some(-2),
        "frightened 2 is −2 — same stored row, no per-value special case"
    );
    assert_eq!(resolve_mapping_value(&mapping, 4), Some(-4));
    let mut bonus = mapping.clone();
    bonus
        .as_object_mut()
        .expect("mapping is an object")
        .insert("polarity".to_owned(), Value::String("positive".to_owned()));
    assert_eq!(
        resolve_mapping_value(&bonus, 2),
        Some(2),
        "a positive polarity resolves through the same contract"
    );
    let mut signless = mapping;
    signless
        .as_object_mut()
        .expect("mapping is an object")
        .remove("polarity");
    assert_eq!(
        resolve_mapping_value(&signless, 2),
        None,
        "a mapping without polarity cannot resolve — the engine never guesses a sign"
    );
}

#[test]
fn constant_mappings_resolve_to_their_signed_value() {
    // The seed parser accepts constants only with a signed value and no
    // polarity; the stored row must therefore name `value_kind` for the
    // canonical reader to reach that value. Proven on what the writer
    // actually wrote, so the pair cannot drift.
    let fixture = doc_from_fixture("frightened.json");
    let seed = seed_from_json(&seed_single(
        FRIGHTENED_ID,
        "engine_math",
        r#"[{
            "modifier_type": "item",
            "stat": "all_checks_and_dcs",
            "value_kind": "constant",
            "value": -2
        }]"#,
    ));
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&fixture),
        &empty_existing(),
        Some(&seed),
        TEST_RELEASE,
    );
    let mapping = planned_modifiers(&plan)
        .expect("engine-math carries mappings")
        .as_array()
        .expect("mappings are an array")
        .first()
        .expect("one mapping")
        .clone();
    assert_eq!(
        mapping.get("value_kind").and_then(Value::as_str),
        Some("constant"),
        "the stored constant row names its value kind — the reader requires it"
    );
    assert_eq!(
        resolve_mapping_value(&mapping, 7),
        Some(-2),
        "a constant mapping resolves to its signed value regardless of the \
         condition's current value"
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
        TEST_RELEASE,
    );
    assert!(
        !plan.warnings.is_empty(),
        "a seed valued-expectation mismatch must surface in the run report"
    );
}

#[test]
fn same_hash_and_version_skips_without_a_write() {
    let frightened = doc_from_fixture("frightened.json");
    // The stored row is what a previous run of THIS seed wrote — tier and
    // mapping rows included — so the skip gate has full agreement.
    let first = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
        TEST_RELEASE,
    );
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("engine_math".to_owned()),
            modifiers: planned_modifiers(&first),
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
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
fn seed_tier_flip_to_display_only_replans_despite_matching_hash_and_version() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    // The row was imported as engine_math; hash and version still match.
    existing.insert(
        frightened.source_id.clone(),
        existing_row(&frightened.content_hash, IMPORTER_VERSION),
    );
    // The owners retire the math in the seed — without a version bump.
    let retired = seed_from_json(&seed_single(FRIGHTENED_ID, "display_only", "[]"));
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&retired),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "a seed tier flip must re-plan the row even when hash and version match — \
         otherwise retired math survives forever on human discipline alone"
    );
    assert_eq!(plan.skipped, 0, "a divergent tier is not a skip");
    let write = plan.updates.first().expect("update planned");
    assert!(
        write.modifiers.is_none(),
        "retiring the math means the modifier rows go too (FR-10)"
    );
    assert_eq!(
        nested_str(&write.data, &["import", "tier"]),
        Some("display_only"),
        "the stored tier converges to the seed's verdict"
    );
}

#[test]
fn seed_tier_flip_to_engine_math_replans_with_mapping_rows() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    // The row was imported before the condition was seeded: display-only.
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("display_only".to_owned()),
            modifiers: None,
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "promoting a condition to engine-math must re-plan it"
    );
    let write = plan.updates.first().expect("update planned");
    assert!(
        write.modifiers.is_some(),
        "the promoted condition carries its mapping rows"
    );
    assert_eq!(
        nested_str(&write.data, &["import", "tier"]),
        Some("engine_math")
    );
}

#[test]
fn unmapped_condition_with_matching_stored_tier_still_skips() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("display_only".to_owned()),
            modifiers: None,
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&seed_from_json(r#"{"conditions": []}"#)),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.skipped, 1,
        "no seed entry + stored display-only agree — the no-op case stays a no-op"
    );
    assert!(plan.updates.is_empty(), "agreement never churns a row");
}

#[test]
fn stored_mapping_change_replans_despite_matching_hash_version_and_tier() {
    let frightened = doc_from_fixture("frightened.json");
    // A row written before the seed carried polarity: hash, importer
    // version and tier all agree with the current run, but the stored
    // mapping predates the seed's current verdict.
    let stale_row = ExistingRow {
        name: frightened.name.clone(),
        content_hash: frightened.content_hash.clone(),
        importer_version: IMPORTER_VERSION,
        tier: Some("engine_math".to_owned()),
        modifiers: Some(serde_json::json!([
            {
                "type": "status",
                "stat": "all_checks_and_dcs",
                "value": null,
                "value_kind": "condition_value"
            }
        ])),
        pack_version: Some(TEST_RELEASE.to_owned()),
    };
    let mut existing = empty_existing();
    existing.insert(frightened.source_id.clone(), stale_row);
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "a seed mapping correction must re-plan the row even when hash, version \
         and tier match — otherwise the correction never reaches the corpus"
    );
    assert_eq!(plan.skipped, 0, "a divergent mapping is not a skip");
    let row = plan
        .updates
        .first()
        .expect("update planned")
        .modifiers
        .as_ref()
        .expect("engine-math carries mappings")
        .as_array()
        .expect("mappings are an array")
        .first()
        .expect("one mapping")
        .clone();
    assert_eq!(
        row.get("polarity").and_then(Value::as_str),
        Some("negative"),
        "the corrected mapping — not the stale one — is what lands"
    );
}

#[test]
fn different_requested_release_restamps_an_unchanged_row() {
    // Hash, importer version, tier and mappings all agree — but the row
    // was stamped by another release. Per-row provenance must name the
    // release this run imported, so the row is re-stamped, never skipped.
    let frightened = doc_from_fixture("frightened.json");
    let first = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
        TEST_RELEASE,
    );
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("engine_math".to_owned()),
            modifiers: planned_modifiers(&first),
            pack_version: Some("pf2e-8.4.0".to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "an unchanged row stamped by another release is re-stamped"
    );
    assert_eq!(plan.skipped, 0, "a release mismatch is never a no-op");
}

#[test]
fn unstamped_row_is_never_skipped() {
    // A NULL pack_version cannot prove the row came from this release —
    // the fail-safe direction is to re-stamp it honestly.
    let frightened = doc_from_fixture("frightened.json");
    let first = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
        TEST_RELEASE,
    );
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("engine_math".to_owned()),
            modifiers: planned_modifiers(&first),
            pack_version: None,
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "a row without a release stamp is re-stamped"
    );
    assert_eq!(plan.skipped, 0);
}

#[test]
fn unchanged_stored_mapping_still_skips() {
    let frightened = doc_from_fixture("frightened.json");
    let first = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &empty_existing(),
        Some(&real_seed()),
        TEST_RELEASE,
    );
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("engine_math".to_owned()),
            modifiers: planned_modifiers(&first),
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.skipped, 1,
        "an identical stored mapping must stay a no-op (FR-3)"
    );
    assert_eq!(
        plan.updates,
        [] as [RowWrite; 0],
        "a no-op rerun must plan no row writes (FR-3)"
    );
}

#[test]
fn skipped_rerun_still_reports_unmapped_conditions() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: Some("display_only".to_owned()),
            modifiers: None,
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&seed_from_json(r#"{"conditions": []}"#)),
        TEST_RELEASE,
    );
    assert_eq!(plan.skipped, 1, "the row itself is a no-op");
    assert_eq!(
        plan.unmapped,
        vec!["Frightened".to_owned()],
        "the gap is release-vs-seed state, not a write side effect — \
         every run report lists it (FR-11)"
    );
}

#[test]
fn missing_stored_tier_never_skips() {
    let frightened = doc_from_fixture("frightened.json");
    let mut existing = empty_existing();
    existing.insert(
        frightened.source_id.clone(),
        ExistingRow {
            name: frightened.name.clone(),
            content_hash: frightened.content_hash.clone(),
            importer_version: IMPORTER_VERSION,
            tier: None,
            modifiers: None,
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        std::slice::from_ref(&frightened),
        &existing,
        Some(&real_seed()),
        TEST_RELEASE,
    );
    assert_eq!(
        plan.updates.len(),
        1,
        "a row without stored tier metadata is re-stamped honestly, never skipped"
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
        TEST_RELEASE,
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
        TEST_RELEASE,
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
            tier: Some("display_only".to_owned()),
            modifiers: None,
            pack_version: Some(TEST_RELEASE.to_owned()),
        },
    );
    let plan = plan_category(
        Kind::Condition,
        &[frightened],
        &existing,
        None,
        TEST_RELEASE,
    );
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
        TEST_RELEASE,
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
        TEST_RELEASE,
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
