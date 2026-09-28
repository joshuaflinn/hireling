//! Unit tests for envelope validation and the unknown-field walker — every
//! failure class asserted with its exact contract §4 message (SC-2), every
//! required path individually violated (the §2 rule), and the walker's
//! modeled-scope pinned against the fixture.

use serde_json::Value;

use super::{parse_and_validate, unknown_fields};
use crate::pbimport::error::ImportError;
use crate::pbimport::fixtures::reference_export;

/// The fixture as a mutable document.
fn fixture_doc() -> Value {
    serde_json::from_str(reference_export()).expect("fixture parses as JSON")
}

/// Serialize a mutated document back into a request body.
fn body_of(doc: &Value) -> String {
    serde_json::to_string(doc).expect("mutation serializes")
}

/// A minimal-but-complete required shape (contract §2): nothing beyond it.
fn minimal_export() -> String {
    serde_json::json!({
        "success": true,
        "build": {
            "name": "Minimal Marty",
            "level": 1,
            "abilities": { "str": 10, "dex": 10, "con": 10, "int": 10, "wis": 10, "cha": 10 },
            "proficiencies": {}
        }
    })
    .to_string()
}

/// Remove one path from the fixture document and return the body.
fn without(path: &[&str]) -> String {
    let mut doc = fixture_doc();
    remove(&mut doc, path);
    body_of(&doc)
}

/// Remove the key at `path` (exists in the fixture by construction).
fn remove(doc: &mut Value, path: &[&str]) {
    let (last, parents) = path.split_last().expect("non-empty path");
    let mut node = doc;
    for step in parents {
        node = node.get_mut(*step).expect("parent exists in fixture");
    }
    node.as_object_mut()
        .expect("fixture nodes along the path are objects")
        .remove(*last);
}

/// Set the key at `path` to `value` (path exists in the fixture).
fn set(doc: &mut Value, path: &[&str], value: Value) {
    let (last, parents) = path.split_last().expect("non-empty path");
    let mut node = doc;
    for step in parents {
        node = node.get_mut(*step).expect("parent exists in fixture");
    }
    node.as_object_mut()
        .expect("fixture nodes along the path are objects")
        .insert((*last).to_owned(), value);
}

#[test]
fn truncated_json_is_class_a_with_the_exact_message() {
    let truncated = reference_export()
        .get(..120)
        .expect("120 is a char boundary");
    let err = parse_and_validate(truncated).expect_err("truncated body rejected");
    assert_eq!(err, ImportError::InvalidJson, "class (a)");
    assert_eq!(err.status(), 400, "contract §4 status");
    assert_eq!(err.code(), "invalid-json", "contract §4 code");
    assert_eq!(
        err.message(),
        "That isn't valid JSON. Copy the whole export from Pathbuilder \
         (Share → Export JSON) and paste it again.",
        "contract §4 message, verbatim"
    );
}

#[test]
fn valid_json_that_is_not_pathbuilder_is_class_b_with_the_exact_message() {
    let err = parse_and_validate(r#"{"hello":"world"}"#).expect_err("not an export");
    assert_eq!(err, ImportError::NotPathbuilder, "class (b)");
    assert_eq!(err.status(), 400, "contract §4 status");
    assert_eq!(err.code(), "not-pathbuilder", "contract §4 code");
    assert_eq!(
        err.message(),
        "That's valid JSON, but it doesn't look like a Pathbuilder export \
         — the character sheet fields (name, level, abilities, \
         proficiencies) are missing.",
        "contract §4 message, verbatim"
    );
}

#[test]
fn a_false_success_envelope_is_class_b() {
    let mut doc = fixture_doc();
    set(&mut doc, &["success"], Value::Bool(false));
    let err = parse_and_validate(&body_of(&doc)).expect_err("success:false rejected");
    assert_eq!(err, ImportError::NotPathbuilder, "class (b), same message");
}

#[test]
fn each_of_the_six_required_paths_is_individually_enforced() {
    let violations: Vec<String> = vec![
        without(&["success"]),
        without(&["build"]),
        without(&["build", "name"]),
        without(&["build", "level"]),
        without(&["build", "abilities"]),
        without(&["build", "proficiencies"]),
    ];
    for body in violations {
        let err = parse_and_validate(&body).expect_err("a missing required path is class (b)");
        assert_eq!(err, ImportError::NotPathbuilder, "same code, same message");
    }
}

#[test]
fn an_empty_name_is_a_class_b_violation() {
    let mut doc = fixture_doc();
    set(&mut doc, &["build", "name"], Value::String(String::new()));
    assert_eq!(
        parse_and_validate(&body_of(&doc)),
        Err(ImportError::NotPathbuilder),
        "name must be non-empty"
    );
}

#[test]
fn levels_outside_one_to_twenty_are_class_b() {
    for level in [0_i64, 21, -3] {
        let mut doc = fixture_doc();
        set(&mut doc, &["build", "level"], Value::from(level));
        let err = parse_and_validate(&body_of(&doc)).expect_err("level outside 1–20 rejected");
        assert_eq!(
            err,
            ImportError::NotPathbuilder,
            "level {level} is class (b)"
        );
    }
}

#[test]
fn a_non_integer_level_is_class_b() {
    let mut doc = fixture_doc();
    set(&mut doc, &["build", "level"], serde_json::json!(2.5));
    assert_eq!(
        parse_and_validate(&body_of(&doc)),
        Err(ImportError::NotPathbuilder),
        "level must be an integer"
    );
}

#[test]
fn abilities_missing_any_score_is_class_b() {
    for score in ["str", "dex", "con", "int", "wis", "cha"] {
        let body = without(&["build", "abilities", score]);
        let err = parse_and_validate(&body).expect_err("a missing score is class (b)");
        assert_eq!(
            err,
            ImportError::NotPathbuilder,
            "missing {score} is class (b)"
        );
    }
}

#[test]
fn a_non_numeric_ability_score_is_class_b() {
    let mut doc = fixture_doc();
    set(
        &mut doc,
        &["build", "abilities", "wis"],
        Value::String("ten".to_owned()),
    );
    assert_eq!(
        parse_and_validate(&body_of(&doc)),
        Err(ImportError::NotPathbuilder),
        "scores must be numeric"
    );
}

#[test]
fn the_frozen_fixture_validates_with_zero_unknown_fields() {
    let export = parse_and_validate(reference_export()).expect("the fixture validates");
    assert!(
        unknown_fields(&export).is_empty(),
        "SC-1: the fixture has zero unknown fields"
    );
}

#[test]
fn a_martial_export_without_spellcasters_validates() {
    let body = without(&["build", "spellCasters"]);
    assert!(
        parse_and_validate(&body).is_ok(),
        "spellCasters is optional (contract §2): martial characters import"
    );
}

#[test]
fn a_minimal_required_shape_only_export_validates() {
    assert!(
        parse_and_validate(&minimal_export()).is_ok(),
        "nothing beyond the required shape may be demanded"
    );
}

#[test]
fn injected_unknown_fields_are_named_but_do_not_fail() {
    let mut doc = fixture_doc();
    set(&mut doc, &["build", "futureField"], Value::Bool(true));
    set(
        &mut doc,
        &["build", "attributes", "newThing"],
        Value::String("surprise".to_owned()),
    );
    set(&mut doc, &["spellcastingFuture"], Value::Bool(true));

    let export = parse_and_validate(&body_of(&doc)).expect("class (c) never fails validation");
    let mut paths = unknown_fields(&export);
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "build.attributes.newThing".to_owned(),
            "build.futureField".to_owned(),
            "spellcastingFuture".to_owned(),
        ],
        "dotted paths, top-level and nested (FR-6)"
    );
}
