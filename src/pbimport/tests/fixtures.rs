//! The frozen Pathbuilder export fixture (E5 Task 0) and its regression test.
//!
//! Byte-verbatim capture of the `#pbExport` JSON block from
//! `docs/reference/lorum_ipsum_dashboard.html` — the import contract
//! (`contracts/pb-export.md`). `include_str!` embeds it at compile time, so
//! the bytes every importer test runs against cannot drift from the captured
//! document. Every later pbimport test module builds on [`reference_export`].

/// The reference export, byte-for-byte as captured (5,493 bytes).
#[must_use]
pub(crate) fn reference_export() -> &'static str {
    include_str!("../../../tests/data/pb_export_reference.json")
}

#[test]
fn fixture_is_the_frozen_reference_export() {
    let export = reference_export();
    assert_eq!(export.len(), 5_493, "fixture byte length changed");

    let doc: serde_json::Value = serde_json::from_str(export).expect("fixture parses as JSON");
    assert_eq!(
        doc.get("success"),
        Some(&serde_json::json!(true)),
        "success flag"
    );
    assert_eq!(
        doc.get("build")
            .and_then(|build| build.get("name"))
            .and_then(|name| name.as_str()),
        Some("Lorum Ipsum"),
        "build name"
    );

    let build_keys = doc
        .get("build")
        .and_then(|build| build.as_object())
        .expect("build is an object");
    assert_eq!(build_keys.len(), 39, "build key count at capture time");

    let casters = doc
        .get("build")
        .and_then(|build| build.get("spellCasters"))
        .and_then(|casters| casters.as_array())
        .expect("spellCasters array");
    assert_eq!(
        casters.len(),
        2,
        "reference build carries two caster blocks"
    );
}
