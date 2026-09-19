//! Tests for [`super`].

use super::{VERSION, health_payload};

#[test]
fn health_payload_reports_ok_with_the_build_version() {
    let payload = health_payload();

    assert_eq!(payload.status, "ok", "health status should be a stable ok");
    assert_eq!(
        payload.version,
        env!("CARGO_PKG_VERSION"),
        "the served version should be the crate version baked in at build time"
    );
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn health_payload_serializes_to_the_public_contract() {
    let json = serde_json::to_value(health_payload()).unwrap();

    assert_eq!(
        json,
        serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }),
        "the wire shape is part of the endpoint's contract"
    );
}
