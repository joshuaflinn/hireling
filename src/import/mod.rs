//! The rules corpus importer (epic E4): a one-time ETL from the Foundry VTT
//! pf2e system packs into the corpus tables.
//!
//! Pipeline: connect → fetch (pinned release, sha256-verified) → extract +
//! validate everything → plan per category (pure) → one transaction per
//! category → run report. A run that fails anywhere before a category's
//! transaction writes nothing for that category; a failing run never
//! half-writes (FR-5). Custom-lane rows are structurally untouchable
//! (FR-4). Runs are logged with per-category counts and outcome (FR-12).

pub mod args;
pub mod fetch;
pub mod license;
pub mod model;
pub mod pack;
pub mod report;
pub mod seed;
pub mod store;
pub mod transform;

pub(crate) use fetch::hex_digest;

use anyhow::{Context as _, anyhow};
use sqlx::PgPool;
use std::fmt::Write as _;

use crate::config::Settings;
use crate::import::args::{release_triple, validate_release};
use crate::import::model::{IMPORTER_VERSION, Kind};
use crate::import::pack::PackCategory;
use crate::import::report::{CategoryCounts, RunReport};
use crate::import::seed::TierSeed;
use crate::import::transform::{CategoryPlan, plan_category};

/// The checked-in, human-reviewed condition tier seed (FR-11).
pub const SEED_PATH: &str = "data/seed/condition-tiers.json";
/// The vendored seed at build time — the import run never edits it.
const SEED_JSON: &str = include_str!("../../data/seed/condition-tiers.json");

/// Run the corpus import for a pinned release.
///
/// # Errors
///
/// Returns an error when the release tag is unpinned, the seed is invalid,
/// the database is unreachable, the fetch fails or its digest does not
/// verify, any document fails validation, a category would import zero
/// documents, or the corpus refuses a write. The run report is emitted on
/// both success and failure paths.
pub async fn run_import(settings: &Settings, release: &str) -> anyhow::Result<RunReport> {
    let pool = connect(settings)
        .await
        .context("import target database unreachable (connect before fetch)")?;
    let client = fetch::build_client()?;
    let zip_bytes = fetch_pack_zip(&client, release).await?;
    import_from_bytes(&pool, release, &zip_bytes, SEED_JSON).await
}

/// The import pipeline from already-fetched archive bytes.
///
/// This is the seam integration tests run through; [`run_import`] only adds
/// the network fetch in front of it.
///
/// # Errors
///
/// Same failure classes as [`run_import`] minus the network fetch.
pub async fn import_from_bytes(
    pool: &PgPool,
    release: &str,
    zip_bytes: &[u8],
    seed_json: &str,
) -> anyhow::Result<RunReport> {
    let mut report = RunReport {
        release: release.to_owned(),
        importer_version: IMPORTER_VERSION,
        success: false,
        categories: Vec::new(),
        stale_rows: Vec::new(),
        unmapped_conditions: Vec::new(),
        warnings: Vec::new(),
        notes: Vec::new(),
        error: None,
    };
    let outcome = import_inner(pool, release, zip_bytes, seed_json, &mut report).await;
    match outcome {
        Ok(()) => {
            report.success = true;
            tracing::info!(report = %report.to_json(), "import run finished");
            println!("{}", report.render());
            Ok(report)
        }
        Err(err) => {
            report.error = Some(format!("{err:#}"));
            tracing::error!(report = %report.to_json(), "import run failed");
            eprintln!("{}", report.render());
            Err(err)
        }
    }
}

/// Fetch and verify the release asset for `tag`.
async fn fetch_pack_zip(client: &reqwest::Client, tag: &str) -> anyhow::Result<Vec<u8>> {
    validate_release(tag)?;
    let asset = fetch::fetch_release_asset(client, tag)
        .await
        .context("failed to resolve the pinned release")?;
    fetch::download_verified(client, &asset.download_url, &asset.sha256_hex)
        .await
        .context("failed to download the release asset")
}

/// The pipeline stages after the bytes are in hand.
async fn import_inner(
    pool: &PgPool,
    release: &str,
    zip_bytes: &[u8],
    seed_json: &str,
    report: &mut RunReport,
) -> anyhow::Result<()> {
    let seed = seed::load_seed(seed_json)
        .map_err(|err| anyhow!("{err} — fix the seed before importing"))?;
    let categories = pack::extract_categories(zip_bytes)
        .context("release asset extraction failed before any write")?;
    note_freshness_regression(pool, release, report).await;

    // Plan every category before the first write: one bad document or a
    // zero-document category fails the whole run with the corpus untouched
    // (FR-12, FR-13).
    let plans = plan_all(&categories, pool, &seed).await?;

    for (kind, docs, plan) in plans {
        let counts = store::apply_category(pool, kind, release, &plan).await?;
        collect_report(kind, docs, &plan, counts, report);
    }
    Ok(())
}

/// Validate + plan every category, enforcing the zero-row tripwire.
async fn plan_all(
    categories: &[PackCategory],
    pool: &PgPool,
    seed: &TierSeed,
) -> anyhow::Result<Vec<(Kind, u64, CategoryPlan)>> {
    let mut plans = Vec::with_capacity(categories.len());
    for category in categories {
        let docs = u64::try_from(category.docs.len()).unwrap_or(u64::MAX);
        if category.docs.is_empty() {
            return Err(anyhow!(
                "zero {} documents in this release while the corpus may already hold them — \
                 failing loudly instead of writing nothing (FR-12)",
                category.kind.plural()
            ));
        }
        let existing = store::existing_rows(pool, category.kind).await?;
        let seed_for_kind = match category.kind {
            Kind::Condition => Some(seed),
            Kind::Item => None,
        };
        let plan = plan_category(category.kind, &category.docs, &existing, seed_for_kind);
        plans.push((category.kind, docs, plan));
    }
    Ok(plans)
}

/// Fold one applied plan into the run report.
fn collect_report(
    kind: Kind,
    docs: u64,
    plan: &CategoryPlan,
    counts: (u64, u64),
    report: &mut RunReport,
) {
    report.categories.push((
        kind,
        CategoryCounts {
            docs,
            inserted: counts.0,
            updated: counts.1,
            skipped: plan.skipped,
            stale: u64::try_from(plan.stale.len()).unwrap_or(u64::MAX),
        },
    ));
    for stale in &plan.stale {
        report.stale_rows.push((
            kind.as_str().to_owned(),
            stale.source_id.clone(),
            stale.name.clone(),
        ));
    }
    report.unmapped_conditions.extend(plan.unmapped.clone());
    report.warnings.extend(plan.warnings.clone());
}

/// Record a freshness regression when re-importing an older release.
async fn note_freshness_regression(pool: &PgPool, release: &str, report: &mut RunReport) {
    match store::max_imported_release(pool).await {
        Ok(Some(newest)) => {
            if release_triple(release) < release_triple(&newest) {
                report.notes.push(format!(
                    "freshness regression: {release} is older than the newest imported \
                     release ({newest}); provenance stamps honestly"
                ));
            }
        }
        Ok(None) => {}
        Err(err) => {
            report.warnings.push(format!(
                "could not check for a freshness regression: {err:#}"
            ));
        }
    }
}

/// Run the license verdict; `true` is green, `false` is red (exit 1).
///
/// # Errors
///
/// Returns an error when the database cannot be reached at all — coverage
/// cannot be checked, so the verdict cannot be produced.
pub async fn run_license_verdict(settings: &Settings) -> anyhow::Result<bool> {
    let pool = connect(settings).await?;
    let violations = license::check(&pool).await?;
    if violations.is_empty() {
        println!("license verdict: GREEN — all gate checks pass");
        Ok(true)
    } else {
        eprintln!(
            "license verdict: RED — {} gate check(s) failed:",
            violations.len()
        );
        for violation in &violations {
            eprintln!("  - {}", violation.0);
        }
        Ok(false)
    }
}

/// Archive the upstream pack license texts at a pinned release.
///
/// Writes the verbatim texts plus a `SOURCE.md` naming the release, the
/// archive date, and each file's sha256. The operator commits the result.
///
/// # Errors
///
/// Returns an error for an unpinned tag, fetch failure, or unwritable
/// archive directory.
pub async fn run_license_archive(release: &str) -> anyhow::Result<()> {
    validate_release(release)?;
    let client = fetch::build_client()?;
    std::fs::create_dir_all(license::ARCHIVE_DIR)
        .with_context(|| format!("failed to create {}", license::ARCHIVE_DIR))?;
    let mut source = format!(
        "# Upstream license archive\n\n\
         release: {release}\n\
         archived: {}\n\
         source: https://github.com/{}/tree/{release}/static/licenses\n\n",
        utc_date_string(),
        fetch::UPSTREAM_REPO,
    );
    for file in license::ARCHIVED_FILES {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/static/licenses/{file}",
            fetch::UPSTREAM_REPO,
            release
        );
        let text = fetch::fetch_text(&client, &url)
            .await
            .with_context(|| format!("failed to fetch {file} from {release}"))?;
        let path = format!("{}/{file}", license::ARCHIVE_DIR);
        std::fs::write(&path, &text).with_context(|| format!("failed to write {path}"))?;
        let digest = fetch::sha256_hex(text.as_bytes());
        writeln!(source, "sha256_{file}: {digest}").ok();
        println!("archived {url} -> {path}");
    }
    let source_path = format!("{}/SOURCE.md", license::ARCHIVE_DIR);
    std::fs::write(&source_path, source)
        .with_context(|| format!("failed to write {source_path}"))?;
    println!("wrote {source_path} — commit the archive with the import");
    Ok(())
}

/// Today's UTC date as `YYYY-MM-DD` (civil-from-days, no date dependency).
fn utc_date_string() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    // Howard Hinnant's civil_from_days: days since 1970-01-01 to y/m/d.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Connect to the import target database.
async fn connect(settings: &Settings) -> anyhow::Result<PgPool> {
    let options: sqlx::postgres::PgConnectOptions = settings
        .database_url
        .parse()
        .context("HIRELING_DATABASE_URL is not a valid Postgres URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .context("failed to connect to the database")?;
    Ok(pool)
}
