//! Importer integration tests against a real Postgres (the compose-throwaway
//! pattern; a throwaway database per test).
//!
//! Fixture discipline: upstream documents are captured verbatim from release
//! `pf2e-8.5.1` (`tests/fixtures/packs/`); deliberately-broken variants are
//! labeled mutations of captured fixtures. See `tests/common/mod.rs`.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live at crate root by cargo convention"
)]

use anyhow::Context as _;
use hireling::import::import_from_bytes;
use hireling::import::model::Kind;
use hireling::import::store;
use hireling::import::transform::{CategoryPlan, RowWrite};
use std::sync::atomic::{AtomicU64, Ordering};

use sqlx::postgres::PgConnectOptions;
use sqlx::{PgPool, Row as _};

/// Process-unique suffix for throwaway database names. Wall clocks on some
/// hosts tick coarser than the test scheduler, so nanos alone collided when
/// parallel tests raced `CREATE DATABASE`; the counter makes names unique by
/// construction instead of by luck.
static DB_SEQ: AtomicU64 = AtomicU64::new(0);

/// Admin connection string used to create throwaway databases.
fn admin_url() -> String {
    std::env::var("HIRELING_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hireling:hireling@127.0.0.1:5432/postgres".to_owned())
}

/// Was the test database URL set explicitly (→ hard-fail when unreachable)?
fn explicitly_requested() -> bool {
    std::env::var_os("HIRELING_TEST_DATABASE_URL").is_some()
}

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// A throwaway database for one test; drop it when done.
struct TestDb {
    pool: PgPool,
    name: String,
    admin_opts: PgConnectOptions,
}

impl TestDb {
    fn name(&self) -> &str {
        &self.name
    }

    /// Drop the throwaway database (best effort). `WITH (FORCE)` terminates
    /// any lingering connections, so the test pool need not be closed first.
    async fn drop(self) {
        if let Ok(admin) = PgPool::connect_with(self.admin_opts).await {
            // Generated name, no user input — audited: safe to interpolate.
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )))
            .execute(&admin)
            .await
            .ok();
            admin.close().await;
        }
    }
}

/// Connect to the admin database (for CREATE/DROP DATABASE).
async fn admin_pool() -> anyhow::Result<PgPool> {
    let options: PgConnectOptions = admin_url().parse()?;
    Ok(PgPool::connect_with(options).await?)
}

/// Create a fresh throwaway database with the corpus schema.
///
/// Returns `Ok(None)` (after a notice) when no database is reachable and
/// none was explicitly requested; `Err` on harness failure — the test
/// itself decides to fail.
async fn test_db() -> anyhow::Result<Option<TestDb>> {
    let admin_opts: PgConnectOptions = admin_url().parse()?;
    let admin = match admin_pool().await {
        Ok(pool) => pool,
        Err(err) if !explicitly_requested() => {
            eprintln!(
                "skipping importer integration test: no reachable Postgres ({err}); \
                 set HIRELING_TEST_DATABASE_URL to run against a real instance"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(anyhow::anyhow!(err).context("test database requested but unreachable"));
        }
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let seq = DB_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = format!("hireling_e4_{nanos}_{seq:04}");
    // Generated name, no user input — audited: safe to interpolate.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .context("throwaway database created")?;
    admin.close().await;

    let db_url = admin_url()
        .rsplit_once('/')
        .map_or_else(|| name.clone(), |(prefix, _)| format!("{prefix}/{name}"));
    let options: PgConnectOptions = db_url.parse()?;
    let pool = PgPool::connect_with(options)
        .await
        .context("throwaway pool connects")?;
    sqlx::raw_sql(SCHEMA_SQL)
        .execute(&pool)
        .await
        .context("fixture schema applies")?;
    Ok(Some(TestDb {
        pool,
        name,
        admin_opts,
    }))
}

/// Read a captured fixture document (verbatim from release pf2e-8.5.1).
fn captured(name: &str) -> anyhow::Result<String> {
    let path = format!("{}/tests/fixtures/packs/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).context("fixture must be readable")
}

/// Build a pack file (JSON array) from fixture names.
fn pack_array(names: &[&str]) -> anyhow::Result<Vec<u8>> {
    let mut out = String::from("[");
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&captured(name)?);
    }
    out.push(']');
    Ok(out.into_bytes())
}

/// Build an in-memory zip with the given entries.
fn build_zip(entries: Vec<(&str, Vec<u8>)>) -> anyhow::Result<Vec<u8>> {
    use std::io::Write as _;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(name, options)
                .context("test zip entry starts")?;
            zip.write_all(&bytes).context("test zip entry writes")?;
        }
        zip.finish().context("test zip finishes")?;
    }
    Ok(buf.into_inner())
}

/// Release A: captured conditions + the captured wayfinder, verbatim.
fn release_a_zip() -> anyhow::Result<Vec<u8>> {
    build_zip(vec![
        (
            "packs/conditions.json",
            pack_array(&["frightened.json", "concealed.json"])?,
        ),
        ("packs/equipment.json", pack_array(&["wayfinder.json"])?),
    ])
}

/// A mutation of the captured frightened document: renamed, same `_id` —
/// the rename-in-place scenario (labeled mutation, same identity).
fn renamed_frightened() -> anyhow::Result<String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(&captured("frightened.json")?).context("fixture is JSON")?;
    let object = doc.as_object_mut().context("document is an object")?;
    object.insert("name".to_owned(), serde_json::json!("Terrified"));
    serde_json::to_string(&doc).context("mutation serializes")
}

/// A labeled mutation of the captured wayfinder with a fresh `_id`, so a
/// second item exists for removal scenarios.
fn mutant_wayfinder() -> anyhow::Result<String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(&captured("wayfinder.json")?).context("fixture is JSON")?;
    let object = doc.as_object_mut().context("document is an object")?;
    object.insert("_id".to_owned(), serde_json::json!("mutatedWayfinderId01"));
    object.insert("name".to_owned(), serde_json::json!("Test Wayfinder Mk II"));
    serde_json::to_string(&doc).context("mutation serializes")
}

/// Release B: frightened renamed (same `_id`), wayfinder REMOVED upstream,
/// a mutant item added. Proves update-in-place + the stale report.
fn release_b_zip() -> anyhow::Result<Vec<u8>> {
    let conditions = format!(
        "[{}, {}]",
        renamed_frightened()?,
        captured("concealed.json")?
    );
    let equipment = format!("[{}]", mutant_wayfinder()?);
    build_zip(vec![
        ("packs/conditions.json", conditions.into_bytes()),
        ("packs/equipment.json", equipment.into_bytes()),
    ])
}

/// A zip whose conditions pack is EMPTY — the zero-row tripwire source.
fn empty_conditions_zip() -> anyhow::Result<Vec<u8>> {
    build_zip(vec![
        ("packs/conditions.json", b"[]".to_vec()),
        ("packs/equipment.json", pack_array(&["wayfinder.json"])?),
    ])
}

/// A zip with a schema-drifted condition document (publication removed) —
/// labeled mutation of the captured fixture.
fn drifted_zip() -> anyhow::Result<Vec<u8>> {
    let mut doc: serde_json::Value =
        serde_json::from_str(&captured("frightened.json")?).context("fixture is JSON")?;
    doc.get_mut("system")
        .and_then(serde_json::Value::as_object_mut)
        .context("system is an object")?
        .remove("publication");
    let drifted = serde_json::to_string(&doc).context("mutation serializes")?;
    let conditions = format!("[{drifted}, {}]", captured("concealed.json")?);
    build_zip(vec![
        ("packs/conditions.json", conditions.into_bytes()),
        ("packs/equipment.json", pack_array(&["wayfinder.json"])?),
    ])
}

/// The checked-in tier seed, loaded from the repo (tests the real file).
fn seed_json() -> anyhow::Result<String> {
    std::fs::read_to_string(format!(
        "{}/data/seed/condition-tiers.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .context("the checked-in seed must be readable")
}

/// Count corpus rows matching a lane (all kinds).
async fn count_rows(pool: &PgPool, lane: &str) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT count(*) AS n FROM corpus_entries WHERE lane = $1")
        .bind(lane)
        .fetch_one(pool)
        .await
        .context("count query runs")?;
    Ok(row.get::<i64, _>(0))
}

/// A row's provenance as of a moment: release stamp + import epoch.
async fn stamp_of(pool: &PgPool, source_id: &str) -> anyhow::Result<(String, f64)> {
    let row = sqlx::query(
        "SELECT pack_version, EXTRACT(EPOCH FROM imported_at)::double precision AS stamp \
         FROM corpus_entries WHERE source_id = $1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .context("provenance query runs")?;
    let version: Option<String> = row.try_get("pack_version")?;
    Ok((version.unwrap_or_default(), row.get::<f64, _>("stamp")))
}

/// Snapshot every corpus row as text lines — byte-for-byte comparisons.
/// Includes both timestamps: a no-op run must leave even provenance time
/// untouched, and an update must visibly advance `updated_at`.
async fn snapshot(pool: &PgPool) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT kind, name, lane, data::text AS data, modifiers::text AS modifiers, \
                source_id, pack_version, created_by_sub, \
                imported_at::text AS imported_at, updated_at::text AS updated_at \
         FROM corpus_entries ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("snapshot query runs")?;
    Ok(rows
        .iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                row.get::<String, _>("kind"),
                row.get::<String, _>("name"),
                row.get::<String, _>("lane"),
                row.get::<String, _>("data"),
                row.get::<Option<String>, _>("modifiers")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("source_id")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("pack_version")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("created_by_sub")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("imported_at")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("updated_at")
                    .unwrap_or_default(),
            )
        })
        .collect())
}

const RELEASE_A: &str = "pf2e-8.5.1";
const RELEASE_B: &str = "pf2e-8.6.0";

/// US-1 / SC-3 / SC-4: a clean import lands rows with lanes, provenance,
/// and the tier split, and the run report shows non-zero counts.
#[tokio::test]
async fn clean_import_seeds_rows_with_provenance() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let report = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("a clean import succeeds");

    assert!(report.success, "the run reports success");
    assert_eq!(
        count_rows(&db.pool, "imported").await.expect("count"),
        3,
        "2 conditions + 1 item"
    );
    assert_eq!(
        count_rows(&db.pool, "custom").await.expect("count"),
        0,
        "importer writes no custom rows"
    );
    let counts = &report.categories;
    assert_eq!(counts.len(), 2, "both categories reported");
    assert!(
        counts.iter().all(|(_, c)| c.docs > 0 && c.inserted > 0),
        "non-zero per-table counts"
    );

    // Provenance lives on the row itself (SC-3).
    let row = sqlx::query(
        "SELECT lane, pack_version, imported_at, \
                data->'import'->>'content_hash' AS hash, \
                data->'import'->>'importer_version' AS iversion, \
                data->'import'->>'tier' AS tier, \
                data->'import'->'publication'->>'license' AS license, \
                modifiers \
         FROM corpus_entries WHERE kind = 'condition' AND source_id = 'TBSHQspnbcqxsmjL'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("frightened row exists");
    assert_eq!(
        row.get::<String, _>("lane"),
        "imported",
        "imported lane (FR-2)"
    );
    assert_eq!(
        row.get::<String, _>("pack_version"),
        RELEASE_A,
        "pack release stamped"
    );
    let stamp = sqlx::query(
        "SELECT imported_at IS NOT NULL AS has_stamp FROM corpus_entries          WHERE kind = 'condition' AND source_id = 'TBSHQspnbcqxsmjL'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("provenance query runs");
    assert!(stamp.get::<bool, _>("has_stamp"), "import date stamped");
    assert!(
        !row.get::<String, _>("hash").is_empty(),
        "content hash stamped"
    );
    assert_eq!(
        row.get::<String, _>("tier"),
        "engine_math",
        "frightened is engine math"
    );
    assert_eq!(
        row.get::<String, _>("license"),
        "ORC",
        "publication license stored"
    );
    let modifiers = row.get::<Option<serde_json::Value>, _>("modifiers");
    assert!(
        modifiers.is_some(),
        "engine-math conditions carry mapping rows"
    );

    // The display-only condition has ZERO mapping rows (FR-10).
    let concealed = sqlx::query(
        "SELECT data->'import'->>'tier' AS tier, modifiers \
         FROM corpus_entries WHERE source_id = 'DmAIPqOBomZ7H95W'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("concealed row exists");
    assert_eq!(
        concealed.get::<String, _>("tier"),
        "display_only",
        "concealed is display-only"
    );
    assert!(
        concealed
            .get::<Option<serde_json::Value>, _>("modifiers")
            .is_none(),
        "no fabricated math"
    );

    db.drop().await;
}

/// US-2 / FR-3 / SC-1: re-running the same release is a complete no-op.
#[tokio::test]
async fn rerunning_same_release_changes_zero_rows() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("first import succeeds");
    let before = snapshot(&db.pool).await.expect("snapshot");

    let report = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("second import succeeds");
    assert_eq!(
        report
            .categories
            .iter()
            .map(|(_, c)| c.skipped)
            .sum::<u64>(),
        3,
        "every row skips — zero writes on a same-release re-run (FR-3)"
    );
    assert_eq!(
        snapshot(&db.pool).await.expect("snapshot"),
        before,
        "corpus byte-for-byte identical"
    );

    db.drop().await;
}

/// US-2 / FR-15: a newer release updates changed rows in place, keeps
/// removed rows and reports them stale, and never forks duplicates.
/// Import release A, import release B, snapshotting the unchanged row's
/// provenance after each. Returns
/// `(report_b, concealed_stamp_after_a, concealed_stamp_after_b)` — both
/// stamps are read from the database at the moment named, so each caller
/// asserts freshness against the true prior state, not a stale capture.
async fn corpus_through_release_b(
    pool: &sqlx::PgPool,
) -> anyhow::Result<(hireling::import::RunReport, (String, f64), (String, f64))> {
    import_from_bytes(pool, RELEASE_A, &release_a_zip()?, &seed_json()?)
        .await
        .context("release A import in shared setup")?;
    let stamp_after_a = stamp_of(pool, "DmAIPqOBomZ7H95W").await?;
    let report = import_from_bytes(pool, RELEASE_B, &release_b_zip()?, &seed_json()?)
        .await
        .context("release B import in shared setup")?;
    let stamp_after_b = stamp_of(pool, "DmAIPqOBomZ7H95W").await?;
    Ok((report, stamp_after_a, stamp_after_b))
}

/// A→B: changed rows update in place, the UNCHANGED row is re-stamped to
/// the release that now holds it (with a fresh `imported_at`), removed rows
/// are kept and named, and nothing forks.
#[tokio::test]
async fn newer_release_updates_in_place_and_reports_stale() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let (report, (concealed_version_a, concealed_stamp_a), _stamp_after_b) =
        corpus_through_release_b(&db.pool)
            .await
            .expect("A then B imports");
    assert_eq!(
        concealed_version_a, RELEASE_A,
        "provenance stamps honestly on first import"
    );

    let conditions = report
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "condition")
        .expect("conditions reported");
    assert_eq!(
        conditions.1.updated, 2,
        "renamed frightened updates in place AND unchanged concealed is \
         re-stamped — per-row provenance must name the release this run imported"
    );
    assert_eq!(
        conditions.1.skipped, 0,
        "a release change is never a no-op for a previously stamped row"
    );
    let items = report
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "item")
        .expect("items reported");
    assert_eq!(items.1.inserted, 1, "the new mutant item inserts");
    assert_eq!(items.1.stale, 1, "removed wayfinder is reported stale");
    assert_eq!(
        report
            .stale_rows
            .iter()
            .map(|(_, id, name)| (id.as_str(), name.as_str()))
            .collect::<Vec<_>>(),
        vec![("gbwr57aT9ou8yKWT", "Wayfinder")],
        "the stale list names the kept row (FR-15)"
    );
    assert_eq!(
        count_rows(&db.pool, "imported").await.expect("count"),
        4,
        "nothing deleted, nothing forked"
    );

    let name = sqlx::query(
        "SELECT name, pack_version FROM corpus_entries WHERE source_id = 'TBSHQspnbcqxsmjL'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("frightened row still exists");
    assert_eq!(
        name.get::<String, _>("name"),
        "Terrified",
        "the rename landed in place"
    );
    assert_eq!(
        name.get::<String, _>("pack_version"),
        RELEASE_B,
        "provenance advanced to B"
    );

    let (concealed_version_b, concealed_stamp_b) = stamp_of(&db.pool, "DmAIPqOBomZ7H95W")
        .await
        .expect("concealed row after B");
    assert_eq!(
        concealed_version_b, RELEASE_B,
        "the UNCHANGED row is re-stamped to the release that now holds it"
    );
    assert!(
        concealed_stamp_b > concealed_stamp_a,
        "the re-stamp also refreshes imported_at — freshness is per row"
    );

    db.drop().await;
}

/// B→A and back again: stamps follow the requested release in both
/// directions; a stale-kept row keeps its last honest stamp; and the
/// same-release rerun still skips everything (idempotence survives
/// re-stamping).
#[tokio::test]
async fn returning_to_an_older_release_restamps_back() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let (_, (_, _unused_a_stamp), (concealed_version_b, concealed_stamp_b)) =
        corpus_through_release_b(&db.pool)
            .await
            .expect("A then B imports");
    assert_eq!(
        concealed_version_b, RELEASE_B,
        "shared setup left the unchanged row stamped by B"
    );

    let report_back = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("release A re-import succeeds");
    let conditions_back = report_back
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "condition")
        .expect("conditions reported");
    assert_eq!(
        conditions_back.1.updated, 2,
        "frightened renames back and concealed re-stamps to A"
    );
    let items_back = report_back
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "item")
        .expect("items reported");
    assert_eq!(
        items_back.1.updated, 0,
        "the returned wayfinder already carries its honest A stamp — a \
         stale-kept row was never stamped by the release that dropped it"
    );
    assert_eq!(
        items_back.1.skipped, 1,
        "wayfinder is a plain skip on return"
    );
    assert_eq!(
        items_back.1.stale, 1,
        "the mutant item is now the kept stale row"
    );

    let (concealed_version_again, concealed_stamp_again) = stamp_of(&db.pool, "DmAIPqOBomZ7H95W")
        .await
        .expect("concealed row after returning to A");
    assert_eq!(
        concealed_version_again, RELEASE_A,
        "the stamp follows the requested release in both directions"
    );
    assert!(
        concealed_stamp_again > concealed_stamp_b,
        "returning to A also refreshes the unchanged row's imported_at past \
         B's actual stamp — a pack_version-only rewrite would fail here"
    );

    // Same-release rerun after all that: everything skips again.
    let report_same = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("same-release rerun succeeds");
    let conditions_same = report_same
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "condition")
        .expect("conditions reported");
    assert_eq!(
        conditions_same.1.updated, 0,
        "same-release rerun writes nothing"
    );
    assert_eq!(
        conditions_same.1.skipped, 2,
        "same-release rerun still skips (idempotence survives re-stamping)"
    );
    assert_eq!(
        count_rows(&db.pool, "imported").await.expect("count"),
        4,
        "still nothing deleted, nothing forked"
    );

    db.drop().await;
}

/// FR-4: custom-lane rows are byte-for-byte identical across every run,
/// success or failure.
#[tokio::test]
async fn custom_rows_survive_every_run() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    sqlx::query("INSERT INTO accounts (sub, username, display_name, role) VALUES ('test-sub', 'tester', 'Tester', 'player')")
        .execute(&db.pool).await.expect("account stub inserted");
    sqlx::query(
        "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub) \
         VALUES ('item', '500 Toads', 'custom', '{\"note\": \"party homebrew\"}', 'test-sub')",
    )
    .execute(&db.pool)
    .await
    .expect("custom row inserted");
    let before = snapshot(&db.pool).await.expect("snapshot");
    assert_eq!(before.len(), 1, "only the custom row exists");

    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("release A succeeds");
    import_from_bytes(
        &db.pool,
        RELEASE_B,
        &release_b_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("release B succeeds");
    assert!(
        import_from_bytes(
            &db.pool,
            RELEASE_B,
            &empty_conditions_zip().expect("fixture zip"),
            &seed_json().expect("seed loads")
        )
        .await
        .is_err(),
        "the failing run fails"
    );

    let after = snapshot(&db.pool).await.expect("snapshot");
    assert_eq!(
        after.len(),
        5,
        "four imported rows landed beside the custom one"
    );
    assert!(
        after.first().is_some_and(|row| Some(row) == before.first()),
        "the custom row is byte-for-byte identical after every run (FR-4)"
    );

    db.drop().await;
}

/// FR-12 / SC-5: a run that would import zero rows of a populated category
/// fails loudly and the corpus is preserved.
#[tokio::test]
async fn zero_row_tripwire_preserves_corpus() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("initial import succeeds");
    let before = snapshot(&db.pool).await.expect("snapshot");

    let err = import_from_bytes(
        &db.pool,
        RELEASE_B,
        &empty_conditions_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect_err("the empty-conditions run must fail");

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("zero conditions"),
        "the error names the tripped category, got: {rendered}"
    );
    assert_eq!(
        snapshot(&db.pool).await.expect("snapshot"),
        before,
        "the corpus is untouched"
    );

    db.drop().await;
}

/// FR-13: schema drift anywhere fails the whole run before any write.
#[tokio::test]
async fn schema_drift_fails_before_any_write() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let err = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &drifted_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect_err("the drifted pack must fail validation");
    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("publication"),
        "the error names the violated expectation, got: {rendered}"
    );
    assert_eq!(
        count_rows(&db.pool, "imported").await.expect("count"),
        0,
        "zero rows written"
    );
    assert_eq!(
        count_rows(&db.pool, "custom").await.expect("count"),
        0,
        "zero rows anywhere"
    );

    db.drop().await;
}

/// FR-8: an upstream condition with no seed entry lands display-only and
/// is listed in the run report for the next seed PR.
#[tokio::test]
async fn unmapped_conditions_land_display_only_and_are_reported() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    // Release A carries only frightened + concealed, both seeded. Add a
    // labeled-mutation condition with a fresh _id: it has no seed entry.
    let mutant = {
        let mut doc: serde_json::Value =
            serde_json::from_str(&captured("frightened.json").expect("fixture"))
                .expect("fixture is JSON");
        let object = doc.as_object_mut().expect("document is an object");
        object.insert("_id".to_owned(), serde_json::json!("mutatedConditionId01"));
        object.insert("name".to_owned(), serde_json::json!("Test Sorrow"));
        serde_json::to_string(&doc).expect("mutation serializes")
    };
    let conditions = format!(
        "[{}, {}, {mutant}]",
        captured("frightened.json").expect("fixture"),
        captured("concealed.json").expect("fixture")
    );
    let zip = build_zip(vec![
        ("packs/conditions.json", conditions.into_bytes()),
        (
            "packs/equipment.json",
            pack_array(&["wayfinder.json"]).expect("pack array"),
        ),
    ])
    .expect("fixture zip");
    let report = import_from_bytes(&db.pool, RELEASE_A, &zip, &seed_json().expect("seed loads"))
        .await
        .expect("import succeeds");
    assert_eq!(
        report.unmapped_conditions,
        vec!["Test Sorrow".to_owned()],
        "the unmapped condition is named in the run report"
    );
    let tier = sqlx::query(
        "SELECT data->'import'->>'tier' AS tier, modifiers FROM corpus_entries \
         WHERE source_id = 'mutatedConditionId01'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("mutant row exists");
    assert_eq!(
        tier.get::<String, _>("tier"),
        "display_only",
        "fail-safe tier"
    );
    assert!(
        tier.get::<Option<serde_json::Value>, _>("modifiers")
            .is_none(),
        "no mappings"
    );

    db.drop().await;
}

/// Spec edge case: re-importing an OLDER release stamps honestly and the
/// run log records the freshness regression.
#[tokio::test]
async fn older_release_reimport_notes_the_regression() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    import_from_bytes(
        &db.pool,
        RELEASE_B,
        &release_b_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("release B imports first");
    let report = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed_json().expect("seed loads"),
    )
    .await
    .expect("the older release still imports (reproducibility)");
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("freshness regression")),
        "the run log records the regression, got: {:?}",
        report.notes
    );
    let version =
        sqlx::query("SELECT pack_version FROM corpus_entries WHERE source_id = 'gbwr57aT9ou8yKWT'")
            .fetch_one(&db.pool)
            .await
            .expect("wayfinder row exists");
    assert_eq!(
        version.get::<String, _>("pack_version"),
        RELEASE_A,
        "provenance stamps honestly"
    );

    db.drop().await;
}

/// SC-6: the license verdict is green with covered rows, red naming an
/// uncovered license — producible on demand, without an import.
#[tokio::test]
async fn license_verdict_flips_on_db_coverage() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let settings = hireling::config::Settings {
        log_level: "info".to_owned(),
        port: 0,
        database_url: format!(
            "{}/{}",
            admin_url()
                .rsplit_once('/')
                .map(|(prefix, _)| prefix.to_owned())
                .expect("url shape"),
            db.name()
        ),
        static_dir: "web/dist".into(),
    };

    // No rows: vacuous coverage — the artifact checks pass (real files in
    // the repo), so the verdict is green.
    let green_empty = hireling::import::run_license_verdict(&settings)
        .await
        .expect("verdict runs without any import");
    assert!(
        green_empty,
        "green with all artifacts in place and no imported rows"
    );

    for license in ["ORC", "OGL"] {
        sqlx::query(
            "INSERT INTO corpus_entries (kind, name, lane, data, source_id, pack_version, imported_at) \
             VALUES ('item', $2, 'imported', \
                     jsonb_build_object('import', jsonb_build_object('publication', \
                         jsonb_build_object('license', $1))), $3, 'pf2e-8.5.1', now())",
        )
        .bind(license)
        .bind(format!("row-{license}"))
        .bind(format!("id-{license}"))
        .execute(&db.pool)
        .await
        .expect("imported row inserted");
    }
    let green = hireling::import::run_license_verdict(&settings)
        .await
        .expect("verdict runs");
    assert!(green, "ORC and OGL rows are covered by the notice");

    sqlx::query("UPDATE corpus_entries SET data = jsonb_set(data, '{import,publication,license}', '\"CC-BY-4.0\"') WHERE source_id = 'id-ORC'")
        .execute(&db.pool)
        .await
        .expect("license flipped");
    let red = hireling::import::run_license_verdict(&settings)
        .await
        .expect("verdict runs");
    assert!(!red, "an uncovered license flips the verdict red");
}

async fn frightened_tier_and_modifiers(pool: &PgPool) -> anyhow::Result<(String, bool)> {
    let row = sqlx::query(
        "SELECT data->'import'->>'tier' AS tier, modifiers FROM corpus_entries \
         WHERE kind = 'condition' AND source_id = 'TBSHQspnbcqxsmjL'",
    )
    .fetch_one(pool)
    .await
    .context("frightened row is present")?;
    Ok((
        row.get::<String, _>("tier"),
        row.get::<Option<serde_json::Value>, _>("modifiers")
            .is_none(),
    ))
}

/// The stored tier converges to the seed's current verdict even when the
/// content hash and importer version both still match — a seed correction
/// (retiring or promoting engine math) must never depend on a human
/// remembering a version bump. FR-8/FR-10/FR-11.
#[tokio::test]
async fn seed_tier_change_converges_without_a_version_bump() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let seed = seed_json().expect("seed loads");
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed,
    )
    .await
    .expect("initial import succeeds");

    // Retire the math: frightened's seed entry flips to display-only with
    // an empty modifier list, same release, no version bump.
    let mut retired_seed = serde_json::from_str::<serde_json::Value>(&seed)
        .expect("the checked-in seed is valid JSON");
    for entry in retired_seed
        .get_mut("conditions")
        .and_then(serde_json::Value::as_array_mut)
        .expect("the seed carries a conditions array")
    {
        if entry.get("upstream_id").and_then(serde_json::Value::as_str) == Some("TBSHQspnbcqxsmjL")
        {
            entry["tier"] = serde_json::json!("display_only");
            entry["modifiers"] = serde_json::json!([]);
        }
    }
    let retired = serde_json::to_string(&retired_seed).expect("mutated seed serializes");
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &retired,
    )
    .await
    .expect("the retired-seed import succeeds");
    let (tier, no_modifiers) = frightened_tier_and_modifiers(&db.pool)
        .await
        .expect("row readable");
    assert_eq!(
        tier, "display_only",
        "the retired condition's stored tier converges to the seed"
    );
    assert!(
        no_modifiers,
        "retired math means the live modifier rows are gone (FR-10) — \
         E8 would otherwise compute math the owners just retired"
    );

    // Flip back: the mapping rows return, same release, no bump.
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed,
    )
    .await
    .expect("the restored-seed import succeeds");
    let (tier_after, math_still_gone) = frightened_tier_and_modifiers(&db.pool)
        .await
        .expect("row readable");
    assert_eq!(
        tier_after, "engine_math",
        "promotion converges just the same"
    );
    assert!(
        !math_still_gone,
        "promoted conditions carry their mapping rows again"
    );

    db.drop().await;
}

/// Review finding: a seed mapping correction (tier unchanged) must reach
/// rows whose hash, version and tier all match — the correction converges
/// on the next run without an importer-version bump.
#[tokio::test]
async fn seed_mapping_correction_converges_without_a_version_bump() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    let seed = seed_json().expect("seed loads");
    import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &seed,
    )
    .await
    .expect("initial import succeeds");

    // Correct frightened's mapping in the seed: same release, same tier,
    // no version bump — only the mapping changes.
    let mut corrected = serde_json::from_str::<serde_json::Value>(&seed)
        .expect("the checked-in seed is valid JSON");
    for entry in corrected
        .get_mut("conditions")
        .and_then(serde_json::Value::as_array_mut)
        .expect("the seed carries a conditions array")
    {
        if entry.get("upstream_id").and_then(serde_json::Value::as_str) == Some("TBSHQspnbcqxsmjL")
        {
            entry["modifiers"] = serde_json::json!([
                {
                    "modifier_type": "status",
                    "stat": "all_checks_and_dcs",
                    "value_kind": "condition_value",
                    "polarity": "positive"
                }
            ]);
        }
    }
    let corrected = serde_json::to_string(&corrected).expect("mutated seed serializes");
    let report = import_from_bytes(
        &db.pool,
        RELEASE_A,
        &release_a_zip().expect("fixture zip"),
        &corrected,
    )
    .await
    .expect("the correction run succeeds");
    let conditions = report
        .categories
        .iter()
        .find(|(k, _)| k.as_str() == "condition")
        .expect("conditions reported");
    assert_eq!(conditions.1.updated, 1, "exactly the corrected row updates");
    assert_eq!(
        conditions.1.skipped, 1,
        "the untouched condition still skips"
    );

    let stored = sqlx::query(
        "SELECT modifiers FROM corpus_entries \
         WHERE source_id = 'TBSHQspnbcqxsmjL' AND lane = 'imported'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("frightened row exists");
    let modifiers: serde_json::Value = stored.get("modifiers");
    assert_eq!(
        modifiers
            .get(0)
            .and_then(|row| row.get("polarity"))
            .and_then(serde_json::Value::as_str),
        Some("positive"),
        "the corrected mapping — not the stale one — lives in the corpus"
    );

    db.drop().await;
}

/// Review finding: the unmapped-condition gap list is release-vs-seed
/// state, not a write side effect — an idempotent re-run still lists it.
#[tokio::test]
async fn idempotent_rerun_still_reports_unmapped_conditions() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    // Release A plus one unseeded condition.
    let mutant = {
        let mut doc: serde_json::Value =
            serde_json::from_str(&captured("frightened.json").expect("fixture"))
                .expect("fixture is JSON");
        let object = doc.as_object_mut().expect("document is an object");
        object.insert("_id".to_owned(), serde_json::json!("mutatedConditionId01"));
        object.insert("name".to_owned(), serde_json::json!("Test Sorrow"));
        serde_json::to_string(&doc).expect("mutation serializes")
    };
    let conditions = format!(
        "[{}, {}, {mutant}]",
        captured("frightened.json").expect("fixture"),
        captured("concealed.json").expect("fixture")
    );
    let zip = build_zip(vec![
        ("packs/conditions.json", conditions.into_bytes()),
        (
            "packs/equipment.json",
            pack_array(&["wayfinder.json"]).expect("pack array"),
        ),
    ])
    .expect("fixture zip");
    let seed = seed_json().expect("seed loads");

    let first = import_from_bytes(&db.pool, RELEASE_A, &zip, &seed)
        .await
        .expect("first run succeeds");
    assert_eq!(
        first.unmapped_conditions,
        vec!["Test Sorrow".to_owned()],
        "the first run names the gap"
    );

    let second = import_from_bytes(&db.pool, RELEASE_A, &zip, &seed)
        .await
        .expect("second run succeeds");
    assert_eq!(
        second
            .categories
            .iter()
            .map(|(_, c)| c.skipped)
            .sum::<u64>(),
        4,
        "the re-run is a full no-op: 3 conditions + 1 item, all skipped"
    );
    assert_eq!(
        second.unmapped_conditions,
        vec!["Test Sorrow".to_owned()],
        "the gap list survives the no-op re-run (FR-11)"
    );

    db.drop().await;
}

/// The freshness check's "newest release" ordering is numeric, not
/// lexicographic — text `max()` ranks pf2e-9.9.0 above pf2e-9.10.0 and
/// pf2e-8.5.1 above pf2e-10.0.0. FR-13's honest freshness note depends on
/// the right winner.
#[tokio::test]
async fn max_imported_release_orders_numerically() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    for (source_id, version) in [
        ("condAAAAAAAAA", "pf2e-9.9.0"),
        ("condBBBBBBBBB", "pf2e-9.10.0"),
    ] {
        sqlx::query(
            "INSERT INTO corpus_entries (kind, name, lane, data, source_id, pack_version, imported_at) \
             VALUES ('condition', 'probe', 'imported', '{}', $1, $2, now())",
        )
        .bind(source_id)
        .bind(version)
        .execute(&db.pool)
        .await
        .expect("probe row inserted");
    }
    let newest = store::max_imported_release(&db.pool)
        .await
        .expect("max release readable")
        .expect("a stamped corpus has a newest release");
    assert_eq!(
        newest, "pf2e-9.10.0",
        "9.10.0 is newer than 9.9.0 — lexicographic max() would disagree"
    );
    db.drop().await;
}

/// FR-5 / SC-2: a failure mid-transaction rolls the whole category back.
/// An UPDATE that matches no row fails the apply; the INSERT planned
/// alongside it must vanish with it, and custom rows must not notice.
#[tokio::test]
async fn failed_apply_rolls_back_the_whole_category() {
    let Some(db) = test_db().await.expect("test database harness") else {
        return;
    };
    sqlx::query(
        "INSERT INTO corpus_entries (kind, name, lane, data) \
         VALUES ('condition', 'Homebrew Doom', 'custom', '{\"homebrew\": true}')",
    )
    .execute(&db.pool)
    .await
    .expect("custom row inserted");

    let plan = CategoryPlan {
        inserts: vec![RowWrite {
            source_id: "freshCondition1".to_owned(),
            name: "Fresh".to_owned(),
            data: serde_json::json!({"import": {"tier": "display_only"}}),
            modifiers: None,
        }],
        updates: vec![RowWrite {
            // Matches no imported row — the apply must fail mid-transaction.
            source_id: "vanishedFromCorpus".to_owned(),
            name: "Ghost".to_owned(),
            data: serde_json::json!({}),
            modifiers: None,
        }],
        ..Default::default()
    };
    let outcome = store::apply_category(&db.pool, Kind::Condition, RELEASE_A, &plan).await;
    assert!(
        outcome.is_err(),
        "an update matching no row fails the category loudly"
    );

    let imported = count_rows(&db.pool, "imported").await.expect("count");
    assert_eq!(
        imported, 0,
        "the planned INSERT rolled back with the transaction — never half-written (FR-5)"
    );
    let custom = sqlx::query("SELECT name, data FROM corpus_entries WHERE lane = 'custom'")
        .fetch_one(&db.pool)
        .await
        .expect("the custom row survives");
    assert_eq!(custom.get::<String, _>("name"), "Homebrew Doom");
    assert_eq!(
        custom.get::<serde_json::Value, _>("data"),
        serde_json::json!({"homebrew": true})
    );

    db.drop().await;
}
