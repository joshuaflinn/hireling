//! The run report (FR-12, SC-8): everything a run did, in one place —
//! release, importer version, per-category counts, stale rows, unmapped
//! conditions, warnings, outcome. Rendered for humans on stdout and
//! logged as JSON for scrapers; the report answers "how many rows per
//! table, what release, what outcome, what went stale" without inspecting
//! the database.

use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::import::model::Kind;

/// Per-category counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    /// Documents seen in the release.
    pub docs: u64,
    /// Rows inserted.
    pub inserted: u64,
    /// Rows updated in place.
    pub updated: u64,
    /// Rows skipped (hash + importer version unchanged).
    pub skipped: u64,
    /// Corpus rows absent from this release — kept, reported.
    pub stale: u64,
}

/// One full run's report.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    /// The pinned release tag.
    pub release: String,
    /// The importer version that ran.
    pub importer_version: i64,
    /// Whether the run completed successfully.
    pub success: bool,
    /// Per-category counts, in import order (conditions, items).
    pub categories: Vec<(Kind, CategoryCounts)>,
    /// Kept-but-stale rows: `(kind, source_id, name)`.
    pub stale_rows: Vec<(String, String, String)>,
    /// Conditions that landed display-only for lack of a seed entry.
    pub unmapped_conditions: Vec<String>,
    /// Non-fatal observations.
    pub warnings: Vec<String>,
    /// Notes — e.g. a freshness regression on older-release re-imports.
    pub notes: Vec<String>,
    /// Failure summary when the run failed.
    pub error: Option<String>,
}

impl RunReport {
    /// Structured JSON for scrapers.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "event": "import_run",
            "release": self.release,
            "importer_version": self.importer_version,
            "outcome": if self.success { "success" } else { "failed" },
            "categories": self.categories.iter().map(|(kind, counts)| json!({
                "kind": kind.as_str(),
                "docs": counts.docs,
                "inserted": counts.inserted,
                "updated": counts.updated,
                "skipped": counts.skipped,
                "stale": counts.stale,
            })).collect::<Vec<_>>(),
            "stale_rows": self.stale_rows.iter().map(|(kind, source_id, name)| json!({
                "kind": kind,
                "source_id": source_id,
                "name": name,
            })).collect::<Vec<_>>(),
            "unmapped_conditions": self.unmapped_conditions,
            "warnings": self.warnings,
            "notes": self.notes,
            "error": self.error,
        })
    }

    /// Human-readable summary for the terminal.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let outcome = if self.success { "SUCCESS" } else { "FAILED" };
        writeln!(
            out,
            "import run: release {} importer v{} — {outcome}",
            self.release, self.importer_version
        )
        .ok();
        for (kind, counts) in &self.categories {
            writeln!(
                out,
                "  {:<10} docs {:>5}  inserted {:>5}  updated {:>5}  skipped {:>5}  stale {:>5}",
                kind.plural(),
                counts.docs,
                counts.inserted,
                counts.updated,
                counts.skipped,
                counts.stale
            )
            .ok();
        }
        self.render_lists(&mut out);
        writeln!(out, "(end of report)").ok();
        out
    }

    fn render_lists(&self, out: &mut String) {
        if !self.stale_rows.is_empty() {
            writeln!(out, "  stale (kept, not deleted):").ok();
            for (kind, source_id, name) in &self.stale_rows {
                writeln!(out, "    {kind} {source_id} `{name}`").ok();
            }
        }
        if !self.unmapped_conditions.is_empty() {
            writeln!(
                out,
                "  unmapped conditions (landed display-only; extend \
                 data/seed/condition-tiers.json by PR):"
            )
            .ok();
            for name in &self.unmapped_conditions {
                writeln!(out, "    {name}").ok();
            }
        }
        for warning in &self.warnings {
            writeln!(out, "  warning: {warning}").ok();
        }
        for note in &self.notes {
            writeln!(out, "  note: {note}").ok();
        }
        if let Some(error) = &self.error {
            writeln!(out, "  error: {error}").ok();
        }
    }
}
