//! The pure heart: validated export → `base_sheet` (data-model §2).
//!
//! Normalized sections for everything downstream consumes — identity,
//! abilities, HP inputs with the derived max, AC inputs, proficiencies,
//! spellcasting with slot layouts, equipment with resolved containers,
//! weapons, armor, money, focus, companions — plus verbatim passthrough of
//! the unmodeled-but-known sections under `raw`. A known section whose type
//! drifted is skipped whole (contract §5): degraded sections, never a lost
//! character. Unknown keys never enter the sheet — the walker names them,
//! the rebuild drops them.
//!
//! Pure module: `ValidExport` in, sheet + skip notices out, no I/O.

use serde::Serialize;
use serde_json::Value;

use crate::pbimport::model::ValidExport;

/// The `base_sheet` schema version owned by this shape (data-model §2).
pub const BASE_SHEET_SCHEMA: &str = "hireling.base_sheet.v1";

/// Sections that could not be imported as themselves (contract §5), plus
/// item-level container notices. Surfaced in the post-import diff.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SectionSkips {
    /// Known sections whose type drifted; each imports empty. Labeled by
    /// their `base_sheet` section names (data-model §2).
    pub sections: Vec<String>,
    /// Item names whose container UUID matched no container entry.
    pub unresolved_containers: Vec<String>,
}

impl BaseSheet {
    /// Every slot position the sheet's casters grant, as `(caster_key, rank,
    /// slot_index)` — the anchor key set (data-model §2, contract §3.6).
    /// Derived, never stored.
    #[must_use]
    pub fn slot_layout(&self) -> Vec<(String, i64, i64)> {
        let mut layout = Vec::new();
        for caster in &self.spellcasters {
            for (rank, count) in caster.per_day.iter().enumerate() {
                let rank = i64::try_from(rank).unwrap_or(0);
                for slot_index in 0..*count.max(&0) {
                    layout.push((caster.caster_key.clone(), rank, slot_index));
                }
            }
        }
        layout
    }

    /// The export's prepared spell at each layout position (FR-12 seeding):
    /// keyed `(caster_key, rank)`, position-indexed, `None` beyond the named
    /// list.
    #[must_use]
    pub fn prepared_at(&self, caster_key: &str, rank: i64, slot_index: i64) -> Option<&str> {
        let caster = self
            .spellcasters
            .iter()
            .find(|caster| caster.caster_key == caster_key)?;
        let list = caster.prepared.iter().find(|list| list.rank == rank)?;
        let index = usize::try_from(slot_index).ok()?;
        list.spells.get(index).map(String::as_str)
    }
}

/// The normalized character sheet stored in `characters.base_sheet`
/// (data-model §2 — field-level truth for this shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaseSheet {
    pub schema: String,
    pub identity: Identity,
    pub abilities: Abilities,
    pub hp: Hp,
    pub ac: Option<Value>,
    /// The export's `attributes` block, verbatim (E8: the speed source —
    /// design math table). E5 consumed it for hp only and dropped the rest;
    /// E8's extractor needs `speed + speedBonus`, so the section is now
    /// captured. Additive field: older rows carry `None` (degraded-empty,
    /// contract §5 — never a lost import).
    pub attributes: Option<Value>,
    pub proficiencies: Value,
    pub specific_proficiencies: Option<Value>,
    pub lores: Vec<Lore>,
    pub spellcasters: Vec<Caster>,
    pub equipment: Vec<InventoryItem>,
    pub containers: Vec<Container>,
    pub weapons: Option<Value>,
    pub armor: Option<Value>,
    pub money: Option<Value>,
    pub focus: Option<Value>,
    pub focus_points: i64,
    pub companions: Vec<Companion>,
    pub raw: Value,
}

/// Contract §3.1 identity, verbatim; `snake_case` where E5 renames.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Identity {
    pub name: String,
    pub class: Option<String>,
    pub dual_class: Option<String>,
    pub level: i64,
    pub xp: Option<i64>,
    pub ancestry: Option<String>,
    pub heritage: Option<String>,
    pub background: Option<String>,
    pub alignment: Option<String>,
    pub deity: Option<String>,
    pub age: Option<String>,
    pub gender: Option<String>,
    pub size: Option<i64>,
    pub size_name: Option<String>,
    pub keyability: Option<String>,
    pub languages: Vec<String>,
}

/// The six scores plus the passthrough breakdown (contract §3.2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Abilities {
    pub str: i64,
    pub dex: i64,
    pub con: i64,
    pub int: i64,
    pub wis: i64,
    pub cha: i64,
    pub breakdown: Option<Value>,
}

/// HP inputs and the derived maximum (contract §3.3 formula).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hp {
    pub ancestryhp: i64,
    pub classhp: i64,
    pub bonushp: i64,
    pub bonushp_per_level: i64,
    /// ancestryhp + classhp + bonushp + bonushpPerLevel × (level − 1).
    pub max_hp: i64,
}

/// One caster block, normalized (contract §3.6).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Caster {
    /// The anchor base: `name`, or `name#2`/`name#3`… on duplicates (FR-10).
    pub caster_key: String,
    pub name: Option<String>,
    pub magic_tradition: Option<String>,
    pub spellcasting_type: Option<String>,
    pub ability: Option<String>,
    pub proficiency: Option<i64>,
    pub innate: bool,
    pub focus_points: i64,
    /// `perDay` normalized to exactly 11 entries (ranks 0–10).
    pub per_day: Vec<i64>,
    /// The export's `spells` lists.
    pub known: Vec<SpellList>,
    /// The export's `prepared` lists.
    pub prepared: Vec<SpellList>,
}

/// One rank's spell list (contract §3.6: `{spellLevel, list}`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpellList {
    pub rank: i64,
    pub spells: Vec<String>,
}

/// One equipment entry, container resolved to a name (contract §3.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InventoryItem {
    pub name: String,
    pub qty: i64,
    /// The resolved container name, or None when the item has no container
    /// or its UUID matched no entry (dangling UUIDs are noticed separately).
    pub container: Option<String>,
    pub invested: bool,
}

/// One container (contract §3.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    pub name: String,
    /// `bagOfHolding` — the extradimensional flag.
    pub extradimensional: bool,
    pub backpack: bool,
    pub augmentations: bool,
}

/// One lore, normalized from the export's `[name, rank]` pair (§3.5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lore {
    pub name: String,
    pub rank: i64,
}

/// One companion, normalized from `familiars` (§3.10, data-model §2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Companion {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub name: Option<String>,
    pub abilities: Value,
    pub equipment: Value,
}

/// Build the normalized sheet from a validated export, with skip notices
/// for type-drifted sections (contract §5) and dangling container UUIDs.
#[must_use]
pub fn transform(export: &ValidExport) -> (BaseSheet, SectionSkips) {
    let build = export.build();
    let mut skips = SectionSkips::default();

    let identity = identity(build);
    let abilities = abilities(build);
    let hp = hp(build, identity.level);
    let ac = verbatim_section(build, "acTotal", &mut skips);
    let attributes = verbatim_section(build, "attributes", &mut skips);
    let proficiencies = build
        .get("proficiencies")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let specific_proficiencies = verbatim_section(build, "specificProficiencies", &mut skips);
    let lores = lores(build, &mut skips);
    let spellcasters = spellcasters(build, &mut skips);
    let (equipment, containers) = inventory(build, &mut skips);
    let weapons = verbatim_section(build, "weapons", &mut skips);
    let armor = verbatim_section(build, "armor", &mut skips);
    let money = verbatim_section(build, "money", &mut skips);
    let focus = verbatim_section(build, "focus", &mut skips);
    let focus_points = build
        .get("focusPoints")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let companions = companions(build);
    let raw = raw_sections(build);

    let sheet = BaseSheet {
        schema: BASE_SHEET_SCHEMA.to_owned(),
        identity,
        abilities,
        hp,
        ac,
        attributes,
        proficiencies,
        specific_proficiencies,
        lores,
        spellcasters,
        equipment,
        containers,
        weapons,
        armor,
        money,
        focus,
        focus_points,
        companions,
        raw,
    };
    (sheet, skips)
}

/// A section pulled verbatim when it carries the expected container type;
/// `None` (and a skip notice) when the section is absent or type-drifted.
/// Absent and drifted both land as "empty" — a degraded section, never a
/// lost import (contract §5).
fn verbatim_section(build: &Value, key: &str, skips: &mut SectionSkips) -> Option<Value> {
    match build.get(key) {
        None => None,
        Some(value @ (Value::Object(_) | Value::Array(_))) => Some(value.clone()),
        Some(_) => {
            skips.sections.push(sheet_section_label(key));
            None
        }
    }
}

/// The `base_sheet` section label a build key maps to — the export's own name
/// except where the sheet renames it (data-model §2).
fn sheet_section_label(build_key: &str) -> String {
    match build_key {
        "acTotal" => "ac".to_owned(),
        "specificProficiencies" => "specific_proficiencies".to_owned(),
        other => other.to_owned(),
    }
}

fn string_at(build: &Value, key: &str) -> Option<String> {
    build.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn int_at(build: &Value, key: &str) -> Option<i64> {
    build.get(key).and_then(Value::as_i64)
}

fn identity(build: &Value) -> Identity {
    let languages = build
        .get("languages")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Identity {
        name: string_at(build, "name").unwrap_or_default(),
        class: string_at(build, "class"),
        dual_class: string_at(build, "dualClass"),
        level: int_at(build, "level").unwrap_or(0),
        xp: int_at(build, "xp"),
        ancestry: string_at(build, "ancestry"),
        heritage: string_at(build, "heritage"),
        background: string_at(build, "background"),
        alignment: string_at(build, "alignment"),
        deity: string_at(build, "deity"),
        age: string_at(build, "age"),
        gender: string_at(build, "gender"),
        size: int_at(build, "size"),
        size_name: string_at(build, "sizeName"),
        keyability: string_at(build, "keyability"),
        languages,
    }
}

fn abilities(build: &Value) -> Abilities {
    let scores = build.get("abilities");
    let score = |key: &str| {
        scores
            .and_then(|abilities| abilities.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    Abilities {
        str: score("str"),
        dex: score("dex"),
        con: score("con"),
        int: score("int"),
        wis: score("wis"),
        cha: score("cha"),
        breakdown: scores
            .and_then(|abilities| abilities.get("breakdown"))
            .cloned(),
    }
}

/// Max HP per contract §3.3: ancestryhp + classhp + bonushp +
/// bonushpPerLevel × (level − 1), floored at zero.
fn hp(build: &Value, level: i64) -> Hp {
    let attributes = build.get("attributes");
    let input = |key: &str| {
        attributes
            .and_then(|attributes| attributes.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let ancestryhp = input("ancestryhp");
    let classhp = input("classhp");
    let bonushp = input("bonushp");
    let bonushp_per_level = input("bonushpPerLevel");
    let levels = (level - 1).max(0);
    let max_hp = (ancestryhp + classhp + bonushp + bonushp_per_level * levels).max(0);
    Hp {
        ancestryhp,
        classhp,
        bonushp,
        bonushp_per_level,
        max_hp,
    }
}

fn lores(build: &Value, skips: &mut SectionSkips) -> Vec<Lore> {
    let parsed = build.get("lores").and_then(Value::as_array).map(|entries| {
        entries
            .iter()
            .filter_map(|entry| {
                let pair = entry.as_array()?;
                let name = pair.first()?.as_str()?.to_owned();
                let rank = pair.get(1)?.as_i64()?;
                Some(Lore { name, rank })
            })
            .collect::<Vec<Lore>>()
    });
    match parsed {
        Some(lores) => lores,
        // Absent lores import as empty; a non-array lores is a drifted
        // section (contract §5).
        None if build.get("lores").is_none() => Vec::new(),
        None => {
            skips.sections.push("lores".to_owned());
            Vec::new()
        }
    }
}

fn spellcasters(build: &Value, skips: &mut SectionSkips) -> Vec<Caster> {
    let parsed = build.get("spellCasters").and_then(Value::as_array);
    let Some(blocks) = parsed else {
        if build.get("spellCasters").is_some() {
            skips.sections.push("spellcasters".to_owned());
        }
        return Vec::new();
    };
    let mut casters: Vec<Caster> = blocks.iter().map(normalize_caster).collect();
    let keys = caster_keys(casters.iter().map(|caster| caster.name.as_deref()));
    for (caster, key) in casters.iter_mut().zip(keys) {
        caster.caster_key = key;
    }
    casters
}

/// `caster_key` per FR-10: the block's name when unique within the export;
/// otherwise the first occurrence keeps the bare name and later duplicates
/// get `name#2`, `name#3`, … by array order. Deterministic across re-imports.
fn caster_keys<'a>(names: impl Iterator<Item = Option<&'a str>>) -> Vec<String> {
    let names: Vec<&str> = names.map(|name| name.unwrap_or("")).collect();
    let mut assigned: Vec<&str> = Vec::new();
    let mut keys = Vec::new();
    for name in &names {
        let occurrences = names
            .iter()
            .filter(|candidate| **candidate == *name)
            .count();
        let key = if occurrences > 1 {
            let ordinal = assigned.iter().filter(|seen| **seen == *name).count();
            if ordinal == 0 {
                // The first occurrence keeps the bare name (FR-10).
                (*name).to_owned()
            } else {
                format!("{name}#{}", ordinal + 1)
            }
        } else {
            (*name).to_owned()
        };
        assigned.push(name);
        keys.push(key);
    }
    keys
}

fn normalize_caster(block: &Value) -> Caster {
    Caster {
        caster_key: String::new(),
        name: string_at(block, "name"),
        magic_tradition: string_at(block, "magicTradition"),
        spellcasting_type: string_at(block, "spellcastingType"),
        ability: string_at(block, "ability"),
        proficiency: int_at(block, "proficiency"),
        innate: block
            .get("innate")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        focus_points: int_at(block, "focusPoints").unwrap_or(0),
        per_day: normalize_per_day(block),
        known: spell_lists(block, "spells"),
        prepared: spell_lists(block, "prepared"),
    }
}

/// `perDay` (int[11]) normalized to exactly 11 non-negative entries —
/// rank 0–10 is both the export's contract and the slot table's CHECK.
fn normalize_per_day(block: &Value) -> Vec<i64> {
    let mut per_day = vec![0_i64; 11];
    if let Some(entries) = block.get("perDay").and_then(Value::as_array) {
        for (rank, entry) in entries.iter().enumerate().take(11) {
            if let Some(slot) = per_day.get_mut(rank) {
                *slot = entry.as_i64().unwrap_or(0).max(0);
            }
        }
    }
    per_day
}

fn spell_lists(block: &Value, key: &str) -> Vec<SpellList> {
    block
        .get(key)
        .and_then(Value::as_array)
        .map(|lists| {
            lists
                .iter()
                .filter_map(|list| {
                    let rank = list.get("spellLevel")?.as_i64()?;
                    let spells = list
                        .get("list")
                        .and_then(Value::as_array)?
                        .iter()
                        .map(|spell| spell.as_str().unwrap_or_default().to_owned())
                        .collect();
                    Some(SpellList { rank, spells })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn inventory(build: &Value, skips: &mut SectionSkips) -> (Vec<InventoryItem>, Vec<Container>) {
    let containers = containers(build, skips);
    let container_names: Vec<(String, String)> = containers
        .iter()
        .map(|container| (container.uuid.clone(), container.inner.name.clone()))
        .collect();
    let items = items(build, &container_names, skips);
    (items, containers.into_iter().map(|c| c.inner).collect())
}

struct ContainerEntry {
    uuid: String,
    inner: Container,
}

fn containers(build: &Value, skips: &mut SectionSkips) -> Vec<ContainerEntry> {
    let Some(entries) = build.get("equipmentContainers").and_then(Value::as_object) else {
        if build.get("equipmentContainers").is_some() {
            skips.sections.push("containers".to_owned());
        }
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(uuid, entry)| {
            let name = entry.get("containerName")?.as_str()?.to_owned();
            let flag = |key: &str| entry.get(key).and_then(Value::as_bool).unwrap_or(false);
            Some(ContainerEntry {
                uuid: uuid.clone(),
                inner: Container {
                    name,
                    extradimensional: flag("bagOfHolding"),
                    backpack: flag("backpack"),
                    augmentations: flag("augmentations"),
                },
            })
        })
        .collect()
}

/// Equipment entries are positional arrays (contract §3.7):
/// `[name, qty, containerUUID?, "Invested"?]`.
fn items(
    build: &Value,
    container_names: &[(String, String)],
    skips: &mut SectionSkips,
) -> Vec<InventoryItem> {
    let Some(entries) = build.get("equipment").and_then(Value::as_array) else {
        if build.get("equipment").is_some() {
            skips.sections.push("equipment".to_owned());
        }
        return Vec::new();
    };
    let mut items = Vec::new();
    for entry in entries {
        let Some(fields) = entry.as_array() else {
            skips.sections.push("equipment".to_owned());
            return Vec::new();
        };
        let Some(name) = fields.first().and_then(Value::as_str) else {
            skips.sections.push("equipment".to_owned());
            return Vec::new();
        };
        let qty = fields.get(1).and_then(Value::as_i64).unwrap_or(0);
        // Positional: [name, qty, containerUUID?, "Invested"?]. The third
        // slot is a container reference only when it is not the Invested
        // marker itself.
        let third = fields.get(2).and_then(Value::as_str);
        let invested = third.is_some_and(|marker| marker == "Invested")
            || fields
                .get(3)
                .and_then(Value::as_str)
                .is_some_and(|marker| marker == "Invested");
        let referenced = third.filter(|reference| *reference != "Invested");
        let container = referenced.and_then(|uuid| {
            container_names
                .iter()
                .find(|(known, _)| known == uuid)
                .map(|(_, container_name)| container_name.clone())
        });
        if referenced.is_some() && container.is_none() {
            skips.unresolved_containers.push(name.to_owned());
        }
        items.push(InventoryItem {
            name: name.to_owned(),
            qty,
            container,
            invested,
        });
    }
    items
}

fn companions(build: &Value) -> Vec<Companion> {
    build
        .get("familiars")
        .and_then(Value::as_array)
        .map(|familiars| {
            familiars
                .iter()
                .map(|familiar| Companion {
                    kind: string_at(familiar, "type"),
                    name: string_at(familiar, "name"),
                    abilities: familiar.get("abilities").cloned().unwrap_or(Value::Null),
                    equipment: familiar.get("equipment").cloned().unwrap_or(Value::Null),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The unmodeled-but-known sections stored verbatim (data-model §2 `raw`).
fn raw_sections(build: &Value) -> Value {
    let raw: serde_json::Map<String, Value> = [
        "feats",
        "specials",
        "resistances",
        "rituals",
        "formula",
        "mods",
        "inventorMods",
        "pets",
    ]
    .into_iter()
    .filter_map(|key| build.get(key).map(|value| (key.to_owned(), value.clone())))
    .collect();
    Value::Object(raw)
}

#[cfg(test)]
#[path = "tests/transform.rs"]
mod tests;
