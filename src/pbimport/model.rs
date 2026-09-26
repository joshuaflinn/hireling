//! Envelope parsing, required-shape validation, and the unknown-field walker.
//!
//! Three of the four failure classes live here (contract §4): class (a) —
//! the body is not JSON at all — and class (b) — JSON that violates the
//! six-path required shape — reject the import; class (c) — unknown fields —
//! never does. [`unknown_fields`] names every unknown key path on an
//! accepted document so the orchestrator can log and count them (FR-6).
//!
//! The required shape is the whole class-(b) detector (contract §2):
//! `success` truthy, `build` object, string non-empty `name`, integer
//! `level` 1–20, `abilities` object with six numeric scores, `proficiencies`
//! object. Nothing else is required — notably `spellCasters` — so martial
//! characters import cleanly. Drift tolerance beyond this shape is class
//! (c), never a version pin.
//!
//! Pure module: text in, verdict + typed envelope out, no I/O.

use serde_json::Value;

use crate::pbimport::error::ImportError;

/// An accepted export: parsed JSON guaranteed to carry the required shape
/// (contract §2). The whole document is retained — optional sections are
/// read (or skipped) by the transform, and the walker needs the full tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidExport {
    /// The parsed document, required shape already verified.
    pub(crate) value: Value,
}
impl ValidExport {
    /// The `build` object. Guaranteed present by validation.
    #[must_use]
    pub fn build(&self) -> &Value {
        self.value.get("build").unwrap_or(&Value::Null)
    }
}

/// Parse a request body and validate the required shape, in order:
/// class (a) parse errors, then class (b) shape violations (contract §4).
///
/// # Errors
///
/// [`ImportError::InvalidJson`] when the body is not JSON at all;
/// [`ImportError::NotPathbuilder`] when the required shape is violated.
pub fn parse_and_validate(body: &str) -> Result<ValidExport, ImportError> {
    let value = parse_body(body)?;
    validate_shape(&value)?;
    Ok(ValidExport { value })
}

/// Class (a) alone: the body parsed as JSON, whatever its shape.
///
/// # Errors
///
/// [`ImportError::InvalidJson`] when the body is not JSON at all.
pub fn parse_body(body: &str) -> Result<Value, ImportError> {
    serde_json::from_str(body)
        .ok()
        .ok_or(ImportError::InvalidJson)
}

/// Class (b) alone, on an already-parsed document — the orchestrator runs
/// the depth cap between parse and shape, per contract §4's order
/// (size → depth → parse (a) → shape (b)).
/// # Errors
///
/// [`ImportError::NotPathbuilder`] when the required shape is violated.
pub fn validate_shape_of(value: &Value) -> Result<(), ImportError> {
    validate_shape(value)
}

/// The class-(b) detector: the six-path required shape (contract §2).
fn validate_shape(doc: &Value) -> Result<(), ImportError> {
    let not_pb = ImportError::NotPathbuilder;
    let obj = doc.as_object().ok_or(not_pb)?;

    if obj.get("success") != Some(&Value::Bool(true)) {
        return Err(not_pb);
    }
    let build = obj.get("build").and_then(Value::as_object).ok_or(not_pb)?;

    let name = build.get("name").and_then(Value::as_str);
    if name.is_none_or(str::is_empty) {
        return Err(not_pb);
    }

    let level = build.get("level").and_then(Value::as_i64);
    if !level.is_some_and(|level| (1..=20).contains(&level)) {
        return Err(not_pb);
    }

    let abilities = build.get("abilities").and_then(Value::as_object);
    let Some(abilities) = abilities else {
        return Err(not_pb);
    };
    let six: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
    if six
        .iter()
        .any(|score| abilities.get(*score).is_none_or(|value| !value.is_number()))
    {
        return Err(not_pb);
    }

    if !build.get("proficiencies").is_some_and(Value::is_object) {
        return Err(not_pb);
    }
    Ok(())
}

/// Every unknown key path on an accepted document, dotted, in stable order
/// (FR-6). Only structures with a modeled key-set are walked; passthrough
/// sections are known-opaque and never produce unknowns themselves.
#[must_use]
pub fn unknown_fields(export: &ValidExport) -> Vec<String> {
    let mut unknowns = Vec::new();
    if let Some(obj) = export.value.as_object() {
        for (key, value) in obj {
            match key.as_str() {
                "success" => {}
                "build" => unknowns.extend(walk_build(value)),
                _ => unknowns.push(key.clone()),
            }
        }
    }
    unknowns
}

/// The `build` object's 39 known keys at capture time (contract §3).
const KNOWN_BUILD_KEYS: &[&str] = &[
    "abilities",
    "acTotal",
    "age",
    "alignment",
    "ancestry",
    "armor",
    "attributes",
    "background",
    "class",
    "deity",
    "dualClass",
    "equipment",
    "equipmentContainers",
    "familiars",
    "feats",
    "focus",
    "focusPoints",
    "formula",
    "gender",
    "heritage",
    "inventorMods",
    "keyability",
    "languages",
    "level",
    "lores",
    "mods",
    "money",
    "name",
    "pets",
    "proficiencies",
    "resistances",
    "rituals",
    "size",
    "sizeName",
    "specials",
    "specificProficiencies",
    "spellCasters",
    "weapons",
    "xp",
];

fn walk_build(build: &Value) -> Vec<String> {
    let Some(obj) = build.as_object() else {
        return Vec::new();
    };
    let mut unknowns = Vec::new();
    for (key, value) in obj {
        match key.as_str() {
            "abilities" => unknowns.extend(walk_flat_object(
                value,
                "build.abilities",
                &["breakdown", "cha", "con", "dex", "int", "str", "wis"],
            )),
            "attributes" => unknowns.extend(walk_flat_object(
                value,
                "build.attributes",
                &[
                    "ancestryhp",
                    "bonushp",
                    "bonushpPerLevel",
                    "classhp",
                    "speed",
                    "speedBonus",
                ],
            )),
            "acTotal" => unknowns.extend(walk_flat_object(
                value,
                "build.acTotal",
                &[
                    "acAbilityBonus",
                    "acItemBonus",
                    "acProfBonus",
                    "acTotal",
                    "shieldBonus",
                ],
            )),
            "money" => {
                unknowns.extend(walk_flat_object(
                    value,
                    "build.money",
                    &["cp", "gp", "pp", "sp"],
                ));
            }
            "specificProficiencies" => unknowns.extend(walk_flat_object(
                value,
                "build.specificProficiencies",
                &["expert", "legendary", "master", "trained"],
            )),
            "spellCasters" => unknowns.extend(walk_spellcasters(value)),
            "equipmentContainers" => unknowns.extend(walk_containers(value)),
            _ if KNOWN_BUILD_KEYS.contains(&key.as_str()) => {}
            _ => unknowns.push(format!("build.{key}")),
        }
    }
    unknowns
}

/// Unknown keys of one flat object section: known keys pass (values are
/// scalars or opaque passthrough), anything else is reported.
fn walk_flat_object(value: &Value, prefix: &str, known: &[&str]) -> Vec<String> {
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, _)| format!("{prefix}.{key}"))
        .collect()
}

/// Caster blocks: modeled key-set per block (contract §3.6), with the
/// spells/prepared lists walked one level deeper.
fn walk_spellcasters(value: &Value) -> Vec<String> {
    const BLOCK_KEYS: &[&str] = &[
        "ability",
        "blendedSpells",
        "focusPoints",
        "innate",
        "magicTradition",
        "name",
        "perDay",
        "prepared",
        "proficiency",
        "spellcastingType",
        "spells",
    ];
    const LIST_ELEMENT_KEYS: &[&str] = &["list", "spellLevel"];

    let Some(blocks) = value.as_array() else {
        return Vec::new();
    };
    let mut unknowns = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let Some(obj) = block.as_object() else {
            continue;
        };
        for (key, entry) in obj {
            if !BLOCK_KEYS.contains(&key.as_str()) {
                unknowns.push(format!("build.spellCasters.{index}.{key}"));
            } else if key == "spells" || key == "prepared" {
                let Some(lists) = entry.as_array() else {
                    continue;
                };
                for (position, element) in lists.iter().enumerate() {
                    unknowns.extend(walk_flat_object(
                        element,
                        &format!("build.spellCasters.{index}.{key}.{position}"),
                        LIST_ELEMENT_KEYS,
                    ));
                }
            }
        }
    }
    unknowns
}

/// Equipment containers: uuid → modeled container object (contract §3.7).
fn walk_containers(value: &Value) -> Vec<String> {
    const CONTAINER_KEYS: &[&str] = &["augmentations", "backpack", "bagOfHolding", "containerName"];
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };
    let mut unknowns = Vec::new();
    for (uuid, container) in obj {
        unknowns.extend(walk_flat_object(
            container,
            &format!("build.equipmentContainers.{uuid}"),
            CONTAINER_KEYS,
        ));
    }
    unknowns
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
