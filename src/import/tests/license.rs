//! Unit tests for the license gate's parsing and coverage rules.

use super::*;
use std::fmt::Write as _;

fn notice_with_coverage(imported: &str) -> String {
    format!(
        "# License Notice\n\n\
         Imported rows are ORC or OGL licensed.\n\
         Lanes: core, imported, custom.\n\n\
         ```text\n\
         license-coverage:\n\
         imported: {imported}\n\
         core: reserved\n\
         custom: CUP, party-owned\n\
         ```\n"
    )
}

#[test]
fn notice_with_all_lanes_and_licenses_passes() {
    let notice = notice_with_coverage("ORC, OGL 1.0a");
    assert!(
        check_notice(&notice).is_empty(),
        "a notice naming ORC, OGL, and all three lanes is well-formed"
    );
}

#[test]
fn notice_missing_a_lane_fails_naming_it() {
    let notice = "# License Notice\n\nORC and OGL for imported and custom rows.\n";
    let violations = check_notice(notice);
    assert!(
        violations.iter().any(|v| v.0.contains("core")),
        "the missing lane must be named, got: {violations:?}"
    );
}

#[test]
fn coverage_block_parses_case_insensitively() {
    let covered =
        parse_covered_licenses(&notice_with_coverage("ORC, OGL 1.0a")).expect("the block parses");
    let imported = covered.get("imported").expect("imported lane present");
    assert_eq!(imported.len(), 2, "two covered licenses");
    assert!(
        covered.contains_key("core") && covered.contains_key("custom"),
        "every lane in the block is stored"
    );
}

#[test]
fn ogl_value_is_covered_by_ogl_1_0a_listing() {
    let covered = vec!["orc".to_owned(), "ogl 1.0a".to_owned()];
    assert!(license_covered("ORC", &covered), "exact match covers");
    assert!(
        license_covered("OGL", &covered),
        "a bare OGL row is covered by a listed OGL 1.0a — the listing extends the value"
    );
    assert!(
        !license_covered("CC-BY-4.0", &covered),
        "an uncovered license must flip the verdict red"
    );
    assert!(
        !license_covered("OGL 1.0a", &["ogl".to_owned()]),
        "a notice covering bare OGL does NOT cover OGL 1.0a rows — narrowing is a red flag"
    );
}

#[test]
fn archive_check_green_with_matching_source() {
    let dir = scratch_archive();
    let violations = check_archive_at(&dir);
    assert!(
        violations.is_empty(),
        "an intact archive with matching sha256s passes: {violations:?}"
    );
    std::fs::remove_dir_all(&dir).expect("scratch cleaned up");
}

#[test]
fn archive_check_red_without_source_md() {
    let dir = scratch_archive();
    std::fs::remove_file(dir.join("SOURCE.md")).expect("SOURCE.md removed");
    let violations = check_archive_at(&dir);
    assert!(
        violations.iter().any(|v| v.0.contains("SOURCE.md")),
        "a missing SOURCE.md must be named, got: {violations:?}"
    );
    std::fs::remove_dir_all(&dir).expect("scratch cleaned up");
}

#[test]
fn archive_check_red_when_a_file_drifts() {
    let dir = scratch_archive();
    std::fs::write(dir.join("ORCLicense.md"), "tampered").expect("drift written");
    let violations = check_archive_at(&dir);
    assert!(
        violations.iter().any(|v| v.0.contains("ORCLicense.md")),
        "a drifted archive file must be named, got: {violations:?}"
    );
    std::fs::remove_dir_all(&dir).expect("scratch cleaned up");
}

/// Build a throwaway archive layout with a correct SOURCE.md.
fn scratch_archive() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hireling-license-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is sane")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir created");
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("licenses/foundry-pf2e");
    for file in ARCHIVED_FILES {
        let bytes = std::fs::read(repo.join(file)).expect("repo archive file readable");
        std::fs::write(dir.join(file), bytes).expect("scratch archive file written");
    }
    let mut source = String::from("release: pf2e-8.5.1\n");
    for file in ARCHIVED_FILES {
        let bytes = std::fs::read(dir.join(file)).expect("scratch file readable");
        writeln!(source, "sha256_{file}: {}", sha256_hex(&bytes)).expect("String writes");
    }
    std::fs::write(dir.join("SOURCE.md"), source).expect("SOURCE.md written");
    dir
}

#[test]
fn sha256_hex_is_stable_hex() {
    let digest = sha256_hex(b"hireling");
    assert_eq!(digest.len(), 64, "sha256 hex is 64 chars");
    assert!(
        digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "digest is hex"
    );
}
