//! Extraction of the two imported pack categories from the release asset.
//!
//! The release asset is a zip whose pack files are one JSON array of
//! documents per compendium pack (probed at `pf2e-8.5.1` — see the
//! contracts doc). The importer reads exactly `packs/conditions.json` and
//! `packs/equipment.json`; everything else in the archive — language files,
//! bestiaries, the sf2e system's packs — is ignored (FR-1).
//!
//! Archive integrity guards: per-entry and total uncompressed size caps and
//! a per-category duplicate-id check. A category resolving to zero
//! documents fails the run (the zero-row tripwire; FR-12) before any
//! write.

use std::io::Read as _;

use anyhow::{Context as _, anyhow};
use serde_json::Value;
use zip::ZipArchive;

use crate::import::model::{Kind, PackDoc, parse_doc};

/// Cap for one decompressed entry — the largest real pack is ~40 MB.
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
/// Cap for the whole archive's decompressed content.
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
/// Cap for archive entry count — the real asset holds ~160.
const MAX_ENTRIES: usize = 10_000;

/// One extracted category: validated documents of one kind.
#[derive(Debug)]
pub struct PackCategory {
    /// Which corpus kind these documents belong to.
    pub kind: Kind,
    /// Validated documents in pack order.
    pub docs: Vec<PackDoc>,
}

/// Extract and validate both imported categories from the zip bytes.
///
/// # Errors
///
/// Returns an error when the archive is unreadable, an entry exceeds the
/// size caps, either pack file is missing or unparseable, any document
/// fails its declared shape, or a category holds duplicate upstream ids.
pub fn extract_categories(zip_bytes: &[u8]) -> anyhow::Result<Vec<PackCategory>> {
    let reader = std::io::Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader)
        .map_err(|err| anyhow!("release asset is not a readable zip: {err}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(anyhow!(
            "release asset holds {} entries — over the {} entry guard; refusing",
            archive.len(),
            MAX_ENTRIES
        ));
    }
    let mut conditions: Option<Vec<PackDoc>> = None;
    let mut items: Option<Vec<PackDoc>> = None;
    let mut total_uncompressed: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| anyhow!("release asset entry {index} unreadable: {err}"))?;
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(anyhow!(
                "release asset entry `{}` is {} bytes — over the per-entry guard",
                entry.name(),
                entry.size()
            ));
        }
        total_uncompressed += entry.size();
        if total_uncompressed > MAX_TOTAL_BYTES {
            return Err(anyhow!(
                "release asset decompresses past the total size guard; refusing"
            ));
        }
        let name = entry.name().to_owned();
        let kind = match name.as_str() {
            "packs/conditions.json" => Some(Kind::Condition),
            "packs/equipment.json" => Some(Kind::Item),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        if entry.is_dir() {
            return Err(anyhow!("release asset entry `{name}` is a directory"));
        }
        let mut bytes = Vec::with_capacity(entry.size().try_into().unwrap_or(0));
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read release asset entry `{name}`"))?;
        if bytes.len() as u64 != entry.size() {
            return Err(anyhow!(
                "release asset entry `{name}` decompressed to {} bytes but declares {}",
                bytes.len(),
                entry.size()
            ));
        }
        let docs = parse_pack_file(kind, &name, &bytes)?;
        match kind {
            Kind::Condition => {
                if conditions.is_some() {
                    return Err(duplicate_entry(&name));
                }
                conditions = Some(docs);
            }
            Kind::Item => {
                if items.is_some() {
                    return Err(duplicate_entry(&name));
                }
                items = Some(docs);
            }
        }
    }
    let conditions = conditions.ok_or_else(|| {
        anyhow!("release asset holds no `packs/conditions.json` — zero conditions would import")
    })?;
    let items = items.ok_or_else(|| {
        anyhow!("release asset holds no `packs/equipment.json` — zero items would import")
    })?;
    Ok(vec![
        PackCategory {
            kind: Kind::Condition,
            docs: conditions,
        },
        PackCategory {
            kind: Kind::Item,
            docs: items,
        },
    ])
}

/// Parse one pack file (a JSON array of documents) into validated docs.
///
/// # Errors
///
/// Returns an error when the file is not a JSON array or any document
/// fails its declared shape, naming the document.
pub fn parse_pack_file(kind: Kind, name: &str, bytes: &[u8]) -> anyhow::Result<Vec<PackDoc>> {
    let docs: Vec<Value> = serde_json::from_slice(bytes)
        .with_context(|| format!("pack file `{name}` is not a JSON array of documents"))?;
    let mut parsed = Vec::with_capacity(docs.len());
    for doc in &docs {
        let parsed_doc = parse_doc(kind, doc)
            .map_err(|err| anyhow!("pack file `{name}` failed validation: {err}"))?;
        parsed.push(parsed_doc);
    }
    let mut seen = std::collections::HashSet::new();
    for doc in &parsed {
        if !seen.insert(doc.source_id.as_str()) {
            return Err(anyhow!(
                "pack file `{name}` holds duplicate upstream id `{}` — identity is `_id`",
                doc.source_id
            ));
        }
    }
    Ok(parsed)
}

fn duplicate_entry(name: &str) -> anyhow::Error {
    anyhow!("release asset holds `{name}` twice")
}

#[cfg(test)]
#[path = "tests/pack.rs"]
mod tests;
