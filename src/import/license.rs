//! The license gate (FR-16..FR-18): notice file, archived upstream license
//! texts, and per-row license coverage in the corpus.
//!
//! The verdict is producible on demand without re-importing; it reads the
//! database but never writes. Green gates nothing private — a red verdict
//! binds public exposure only (spec US-4).

use std::path::Path;

use anyhow::Context as _;
use sqlx::PgPool;

use crate::import::fetch::sha256_hex;

/// Where the notice file lives.
pub const NOTICE_PATH: &str = "NOTICE.md";
/// Where the upstream license archive lives.
pub const ARCHIVE_DIR: &str = "licenses/foundry-pf2e";
/// The two upstream license texts the importer's releases carry.
pub const ARCHIVED_FILES: [&str; 2] = ["ORCLicense.md", "OpenGameLicense.md"];

/// One failed check, named for a human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation(pub String);

/// Run every gate check; returns one violation per failure.
///
/// # Errors
///
/// Returns an error only when the database cannot be reached at all —
/// coverage then cannot be checked, which is itself a red condition, but
/// the caller decides; the violation list still carries the artifact
/// failures.
pub async fn check(pool: &PgPool) -> anyhow::Result<Vec<Violation>> {
    let mut violations = Vec::new();
    let notice_text = std::fs::read_to_string(NOTICE_PATH)
        .with_context(|| format!("failed to read {NOTICE_PATH}"));
    let covered = match &notice_text {
        Ok(text) => {
            violations.extend(check_notice(text));
            parse_covered_licenses(text)
        }
        Err(err) => {
            violations.push(Violation(format!("{err:#}")));
            None
        }
    };
    violations.extend(check_archive());
    match covered {
        Some(covered) => violations.extend(check_db_coverage(pool, &covered).await?),
        None => violations.push(Violation(format!(
            "{NOTICE_PATH} has no parsable `license-coverage:` block — imported-row \
             license coverage cannot be checked"
        ))),
    }
    Ok(violations)
}

/// Check the notice file exists, names ORC and OGL, and covers every lane.
fn check_notice(notice: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let lowered = notice.to_lowercase();
    for required in ["orc", "ogl", "core", "imported", "custom"] {
        if !lowered.contains(required) {
            violations.push(Violation(format!(
                "{NOTICE_PATH} does not mention `{required}`"
            )));
        }
    }
    violations
}

/// Parse the machine-readable coverage block.
///
/// The block is a fenced section whose body carries one `lane: licenses`
/// line per lane, e.g. `imported: ORC, OGL 1.0a`.
#[must_use]
pub fn parse_covered_licenses(
    notice: &str,
) -> Option<std::collections::HashMap<String, Vec<String>>> {
    let mut covered = std::collections::HashMap::new();
    let mut in_block = false;
    for line in notice.lines().map(str::trim) {
        if line == "license-coverage:" {
            in_block = true;
            continue;
        }
        if !in_block {
            continue;
        }
        if line.starts_with("```") {
            break;
        }
        let Some((lane, licenses)) = line.split_once(':') else {
            continue;
        };
        let lane = lane.trim().to_lowercase();
        if lane.is_empty() {
            continue;
        }
        let list = licenses
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        covered.insert(lane, list);
    }
    if covered.is_empty() {
        None
    } else {
        Some(covered)
    }
}

/// Is a corpus license value covered by a notice coverage list?
///
/// `OGL` is covered by a listed `OGL 1.0a` (a listed name that extends the
/// value covers it); the reverse is not true — a notice covering bare
/// `OGL` does not cover `OGL 1.0a` rows.
#[must_use]
pub fn license_covered(license: &str, covered: &[String]) -> bool {
    let license = license.trim().to_lowercase();
    covered
        .iter()
        .any(|name| name == &license || name.starts_with(&format!("{license} ")))
}

/// Verify the archived license files exist and match their recorded
/// sha256s in SOURCE.md.
fn check_archive() -> Vec<Violation> {
    check_archive_at(Path::new(ARCHIVE_DIR))
}

/// The archive check against an explicit directory (the repo layout in
/// production, a scratch layout in tests).
fn check_archive_at(dir: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    let source_md_path = dir.join("SOURCE.md");
    let Ok(source_md) = std::fs::read_to_string(&source_md_path) else {
        return vec![Violation(format!(
            "{} is missing — the archive does not name its source release",
            source_md_path.display()
        ))];
    };
    for file in ARCHIVED_FILES {
        let path = dir.join(file);
        let Ok(bytes) = std::fs::read(&path) else {
            violations.push(Violation(format!(
                "archived license file `{}` is missing",
                path.display()
            )));
            continue;
        };
        let actual = sha256_hex(&bytes);
        let recorded = format!("sha256_{file}:");
        let matches = source_md.lines().map(str::trim).any(|line| {
            line.strip_prefix(&recorded)
                .is_some_and(|rest| rest.trim() == actual)
        });
        if !matches {
            violations.push(Violation(format!(
                "archived license file `{}` does not match its sha256 in {}",
                path.display(),
                source_md_path.display()
            )));
        }
    }
    violations
}

/// Check every distinct publication license across imported rows is
/// covered by the notice's `imported` list (FR-18 step 3).
async fn check_db_coverage(
    pool: &PgPool,
    covered: &std::collections::HashMap<String, Vec<String>>,
) -> anyhow::Result<Vec<Violation>> {
    let mut violations = Vec::new();
    let imported_covered = covered
        .get("imported")
        .ok_or_else(|| anyhow::anyhow!("coverage block carries no `imported` lane"))?;
    let counts = crate::import::store::imported_license_counts(pool).await?;
    for (license, count) in counts {
        if license.is_empty() {
            violations.push(Violation(format!(
                "{count} imported corpus row(s) carry no publication license — \
                 rows predating import stamping must be refreshed"
            )));
        } else if !license_covered(&license, imported_covered) {
            violations.push(Violation(format!(
                "{count} imported corpus row(s) are licensed `{license}`, which {NOTICE_PATH} \
                 does not cover for the imported lane"
            )));
        }
    }
    Ok(violations)
}

#[cfg(test)]
#[path = "tests/license.rs"]
mod tests;
