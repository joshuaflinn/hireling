//! Upstream pack document parsing and schema validation.
//!
//! The importer declares the document shapes it supports (the closed
//! contract captured in `specs/004-rules-corpus-importer/contracts/`); a
//! document that does not match fails the run before any write — schema
//! drift is handled by shipping a new importer version, never by guessing
//! at fields (FR-13).
//!
//! The contract was probed against release `pf2e-8.5.1`: every conditions
//! and equipment document in the release carries the required paths below.
//! Content identity is the upstream `_id`; the content hash is sha256 over
//! the canonical (compact, key-sorted) serialization of the parsed document.

use serde::Deserialize;
use serde_json::Value;

use crate::import::hex_digest;

/// Which corpus kind a document belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Condition documents (`packs/conditions.json`).
    Condition,
    /// Item documents (`packs/equipment.json`).
    Item,
}

impl Kind {
    /// The stored `kind` value in `corpus_entries`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Condition => "condition",
            Kind::Item => "item",
        }
    }

    /// The human plural for reports.
    #[must_use]
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Condition => "conditions",
            Kind::Item => "items",
        }
    }
}

/// The Foundry item document types observed in the equipment pack at the
/// probed release. Anything else is schema drift and fails validation.
pub const ITEM_TYPES: [&str; 9] = [
    "equipment",
    "consumable",
    "weapon",
    "treasure",
    "backpack",
    "armor",
    "ammo",
    "shield",
    "kit",
];

/// The importer version stamped on every row this build writes.
///
/// Bump when the declared document shapes, the tier seed, or the stored
/// row shapes change, so re-runs honestly refresh provenance (FR-13).
pub const IMPORTER_VERSION: i64 = 1;

/// A validated pack document, ready for transform.
#[derive(Debug, Clone)]
pub struct PackDoc {
    /// Upstream `_id` — the stable identity across releases (FR-15).
    pub source_id: String,
    /// Display name as of this release.
    pub name: String,
    /// The parsed document itself, stored in the row's `data`.
    pub doc: Value,
    /// sha256 (hex) over the canonical serialization of `doc`.
    pub content_hash: String,
}

/// A document failed its category's declared shape.
#[derive(Debug)]
pub struct DocError {
    /// Which document failed (name where identifiable).
    pub subject: String,
    /// What was violated.
    pub problem: String,
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.subject, self.problem)
    }
}

impl std::error::Error for DocError {}

#[derive(Debug, Deserialize)]
struct DocIdentity {
    #[serde(rename = "_id")]
    id: Value,
    name: Value,
    #[serde(rename = "type")]
    doc_type: Value,
}

/// Parse and validate one document of `kind` from a parsed JSON value.
///
/// # Errors
///
/// Returns a [`DocError`] naming the document and the violated expectation.
pub fn parse_doc(kind: Kind, doc: &Value) -> Result<PackDoc, DocError> {
    let identity: DocIdentity = serde_json::from_value(doc.clone()).map_err(|err| DocError {
        subject: unnamed(doc).to_owned(),
        problem: format!("unrecognized document shape: {err}"),
    })?;
    let subject = identity_name(&identity);
    if identity.id.as_str().is_none_or(str::is_empty) {
        return Err(nonempty(subject, "_id"));
    }
    if identity.name.as_str().is_none_or(str::is_empty) {
        return Err(nonempty(subject, "name"));
    }
    if doc
        .get("img")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(nonempty(subject, "img"));
    }
    validate_type(kind, subject, identity.doc_type.as_str())?;

    let system = doc
        .get("system")
        .and_then(Value::as_object)
        .ok_or_else(|| missing(subject, "system"))?;
    // Presence, not non-emptiness: 199 probed equipment docs (precious
    // materials and variants) legitimately carry an empty description.
    system
        .get("description")
        .and_then(|d| d.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| missing(subject, "system.description.value"))?;
    let publication = system
        .get("publication")
        .ok_or_else(|| missing(subject, "system.publication"))?;
    let license = publication
        .get("license")
        .and_then(Value::as_str)
        .ok_or_else(|| missing(subject, "system.publication.license"))?;
    if license.is_empty() {
        return Err(nonempty(subject, "system.publication.license"));
    }
    if publication.get("title").and_then(Value::as_str).is_none() {
        return Err(missing(subject, "system.publication.title"));
    }
    if publication
        .get("remaster")
        .and_then(Value::as_bool)
        .is_none()
    {
        return Err(missing(subject, "system.publication.remaster"));
    }
    if kind == Kind::Condition
        && system
            .get("value")
            .and_then(|v| v.get("isValued"))
            .and_then(Value::as_bool)
            .is_none()
    {
        return Err(missing(subject, "system.value.isValued"));
    }

    let canonical = serde_json::to_vec(doc).map_err(|err| DocError {
        subject: subject.to_owned(),
        problem: format!("document failed canonical serialization: {err}"),
    })?;
    Ok(PackDoc {
        source_id: identity.id.as_str().unwrap_or_default().to_owned(),
        name: subject.to_owned(),
        doc: doc.clone(),
        content_hash: hex_digest(&canonical),
    })
}

/// The publication block stored per row (license verdict input; FR-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    /// e.g. `ORC` or `OGL`.
    pub license: String,
    /// e.g. `Pathfinder Player Core` (may be empty upstream).
    pub title: String,
    /// Remaster flag from the document itself.
    pub remaster: bool,
}

/// Extract the publication block an imported row stores in its `data`.
///
/// Callers run this only on documents that already passed [`parse_doc`].
#[must_use]
pub fn publication_of(doc: &Value) -> Publication {
    let publication = doc.get("system").and_then(|s| s.get("publication"));
    Publication {
        license: string_at(publication, "license"),
        title: string_at(publication, "title"),
        remaster: publication
            .and_then(|p| p.get("remaster"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Extract `system.value.isValued` (conditions only).
#[must_use]
pub fn is_valued_of(doc: &Value) -> bool {
    doc.get("system")
        .and_then(|s| s.get("value"))
        .and_then(|v| v.get("isValued"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Extract `system.slug` when the document carries one.
#[must_use]
pub fn slug_of(doc: &Value) -> Option<String> {
    doc.get("system")
        .and_then(|s| s.get("slug"))
        .and_then(Value::as_str)
        .filter(|slug| !slug.is_empty())
        .map(str::to_owned)
}

fn validate_type(kind: Kind, subject: &str, doc_type: Option<&str>) -> Result<(), DocError> {
    let found = doc_type.map_or_else(|| "none".to_owned(), |o| format!("\"{o}\""));
    let problem = match (kind, doc_type) {
        (Kind::Condition, Some("condition")) => return Ok(()),
        (Kind::Item, Some(actual)) if ITEM_TYPES.contains(&actual) => return Ok(()),
        (Kind::Condition, _) => format!("expected type \"condition\", found {found}"),
        (Kind::Item, _) => format!(
            "type {found} is not a recognized item type (supported: {})",
            ITEM_TYPES.join(", ")
        ),
    };
    Err(DocError {
        subject: subject.to_owned(),
        problem,
    })
}

fn string_at(holder: Option<&Value>, key: &str) -> String {
    holder
        .and_then(|h| h.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn unnamed(doc: &Value) -> &str {
    doc.get("name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed document>")
}

fn identity_name(identity: &DocIdentity) -> &str {
    identity.name.as_str().unwrap_or("<unnamed document>")
}

fn missing(subject: &str, path: &str) -> DocError {
    DocError {
        subject: subject.to_owned(),
        problem: format!("required path `{path}` missing"),
    }
}

fn nonempty(subject: &str, path: &str) -> DocError {
    DocError {
        subject: subject.to_owned(),
        problem: format!("required path `{path}` must be present and non-empty"),
    }
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
