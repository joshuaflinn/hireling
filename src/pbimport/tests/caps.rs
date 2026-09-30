//! Unit tests for the size/depth caps (FR-3) — boundaries and exact failure
//! payloads.

use serde_json::Value;

use super::{MAX_BODY_BYTES, MAX_DEPTH, check_depth, check_size, measure_depth};
use crate::pbimport::error::ImportError;
use crate::pbimport::fixtures::reference_export;

/// Wrap `levels` containers around a scalar, producing depth `levels + 1`.
fn nested(levels: usize) -> Value {
    let mut value = Value::Bool(true);
    for _ in 0..levels {
        value = serde_json::json!({ "nested": value });
    }
    value
}

#[test]
fn size_boundary_passes_and_one_byte_over_fails() {
    assert_eq!(check_size(MAX_BODY_BYTES), Ok(()), "cap itself passes");
    assert_eq!(check_size(0), Ok(()), "empty body passes the size cap");
    assert_eq!(
        check_size(MAX_BODY_BYTES + 1),
        Err(ImportError::PayloadTooLarge),
        "one byte over the cap fails"
    );
}

#[test]
fn too_large_carries_the_exact_contract_payload() {
    let err = check_size(MAX_BODY_BYTES + 1).expect_err("over-cap body rejected");
    assert_eq!(err.code(), "payload-too-large", "contract §4 code");
    assert_eq!(err.status(), 413, "contract §4 status");
    assert_eq!(
        err.message(),
        "That's too large to be a character export (limit 1 MB). \
         Make sure you exported a single character.",
        "contract §4 message, verbatim"
    );
}

#[test]
fn too_deep_carries_the_exact_contract_payload() {
    let err = check_depth(&nested(MAX_DEPTH)).expect_err("over-deep document rejected");
    assert_eq!(err.code(), "payload-too-deep", "contract §4 code");
    assert_eq!(err.status(), 400, "contract §4 status");
    assert_eq!(
        err.message(),
        "That JSON is nested too deeply to be a character export.",
        "contract §4 message, verbatim"
    );
}

#[test]
fn depth_boundary_passes_at_the_cap_and_fails_above() {
    assert!(
        check_depth(&nested(MAX_DEPTH - 1)).is_ok(),
        "exactly 64 deep passes"
    );
    assert!(check_depth(&nested(MAX_DEPTH)).is_err(), "65 deep fails");
    assert!(
        check_depth(&nested(70)).is_err(),
        "70 deep fails (the plan's regression shape)"
    );
}

#[test]
fn scalars_are_depth_one_and_containers_add_a_level() {
    assert_eq!(measure_depth(&Value::Null), 1, "null is depth 1");
    assert_eq!(
        measure_depth(&serde_json::json!(42)),
        1,
        "number is depth 1"
    );
    assert_eq!(
        measure_depth(&serde_json::json!("x")),
        1,
        "string is depth 1"
    );
    assert_eq!(
        measure_depth(&serde_json::json!([])),
        1,
        "empty array is depth 1"
    );
    assert_eq!(
        measure_depth(&serde_json::json!({})),
        1,
        "empty object is depth 1"
    );
    assert_eq!(
        measure_depth(&serde_json::json!({ "a": [1, { "b": 2 }] })),
        4,
        "object > array > object > scalar"
    );
}

#[test]
fn the_frozen_fixture_measures_eight_and_passes() {
    let fixture: Value = serde_json::from_str(reference_export()).expect("fixture parses as JSON");
    assert_eq!(
        measure_depth(&fixture),
        8,
        "contract §6: fixture depth is 8"
    );
    assert!(
        check_depth(&fixture).is_ok(),
        "the fixture is far under the cap"
    );
}
