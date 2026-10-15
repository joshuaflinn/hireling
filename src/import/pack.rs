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

use crate::import::model::{ALL_KINDS, Kind, PackDoc, parse_doc};

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
    let mut found: std::collections::HashMap<&'static str, Vec<PackDoc>> =
        std::collections::HashMap::new();
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
        let Some((kind, file)) = kind_for_entry(&name) else {
            continue;
        };
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
        if found.contains_key(file) {
            return Err(duplicate_entry(&name));
        }
        found.insert(file, parse_pack_file(kind, &name, &bytes)?);
    }
    let mut categories = Vec::with_capacity(ALL_KINDS.len());
    for kind in ALL_KINDS {
        let mut docs = Vec::new();
        for file in kind.pack_files() {
            let file_docs = found.remove(file).ok_or_else(|| {
                anyhow!(
                    "release asset holds no `{file}` — zero {} would import",
                    kind.plural()
                )
            })?;
            docs.extend(file_docs);
        }
        // Identity is `_id` per kind; a creature pack set that repeats an id
        // across files is as fatal as a repeat within one file.
        let mut seen = std::collections::HashSet::new();
        for doc in &docs {
            if !seen.insert(doc.source_id.clone()) {
                return Err(anyhow!(
                    "{} hold duplicate upstream id `{}` across pack files",
                    kind.plural(),
                    doc.source_id
                ));
            }
        }
        categories.push(PackCategory { kind, docs });
    }
    Ok(categories)
}

/// Which kind (and which declared pack file) a zip entry name belongs to.
fn kind_for_entry(name: &str) -> Option<(Kind, &'static str)> {
    ALL_KINDS.into_iter().find_map(|kind| {
        kind.pack_files()
            .iter()
            .find(|file| **file == name)
            .map(|file| (kind, *file))
    })
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
        // Bestiary packs mix in hazards (1 in Monster Core 2 at pf2e-8.5.1);
        // they are a different document class, not schema drift — skipped
        // by type, never by guessing at fields.
        if kind == Kind::Creature && doc.get("type").and_then(Value::as_str) != Some("npc") {
            continue;
        }
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
