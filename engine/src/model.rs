//! The wire-facing serde types — the engine-output contract
//! (`specs/008-buff-effect-engine/contracts/engine-output.md`), verbatim.
//! Field names and shapes are binding: if code and that document disagree,
//! the document wins until a PR changes it.

use serde::{Deserialize, Serialize};

// Re-exported: consumers speak of modifier types through the contract
// module (`model::ModifierType`) — one vocabulary, one path.
pub use crate::vocab::ModifierType;
use crate::vocab::{StatInstances, StatName};

/// `BaseStats.schema` — the input shape's version string (contract §1).
pub const BASE_SCHEMA: &str = "hireling.engine.base.v1";
/// `EngineOutput.schema` — the output shape's version string (contract §3).
pub const OUTPUT_SCHEMA: &str = "hireling.engine.output.v1";

/// One modifier on an effect — the contract's `{type, stat, value}` (§2).
/// Negative values are penalties; the type is one of the closed four.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifier {
    /// Wire key is `"type"` (`type` is a Rust keyword).
    #[serde(rename = "type")]
    pub modifier_type: ModifierType,
    /// The addressed stat — canonical vocabulary text, validated at the
    /// serde boundary (an unparseable stat can never deserialize).
    pub stat: StatName,
    /// Signed: bonuses positive, penalties negative, `+0` visible in
    /// provenance.
    pub value: i32,
}

/// One active effect — the E7 snapshot/diff shape verbatim (contract §2),
/// plus the corpus chip flag (§4) the write path freezes at apply time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveEffect {
    pub effect_id: i64,
    pub name: String,
    pub source_character_id: i64,
    /// Roster character ids; companions/minions are not targetable (PRD).
    pub targets: Vec<i64>,
    /// Ordered (ord = index); stacking ties break by `(effect_id, ord)`.
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub duration_note: String,
    pub active: bool,
    pub version: i64,
    /// True for a display-only corpus condition: badge chip, zero math —
    /// the engine never fabricates modifiers for it (spec US-3).
    #[serde(default)]
    pub tracked_manually: bool,
}

/// A strike's base values (contract §1): the attack total, the flat part a
/// damage modifier adjusts, and the render inputs the row displays
/// (contract §3 rides them on the strike object — no parallel array).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrikeBase {
    /// Sheet-stable key (the weapon name; duplicates get `name#2` — E5).
    pub key: String,
    /// Display name.
    pub label: String,
    pub attack: i32,
    /// The damage expression as the sheet renders it — the export's die
    /// verbatim plus the signed flat (`"d4−1"`, U+2212 negative), never
    /// parsed. The extract golden pins the reference's exact strings.
    pub damage: String,
    /// The numeric flat part of `damage` (the `−1`).
    pub damage_flat: i32,
    /// The multiple-attack-penalty STEP: 4 for Agile strikes, else 5
    /// (the row renders `−map / −2·map`).
    pub map: i32,
    /// The export's damage-type letter code (`"B"`).
    pub damage_type: String,
    /// Its display name (`"bludgeoning"`).
    pub damage_type_name: String,
    /// Trait chip names, verbatim (the POC weapon-trait map).
    pub traits: Vec<String>,
}

/// One caster block's base values (contract §1), per block instance (Q1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillBase {
    /// Contract text WITHOUT the `skill:` prefix: `"acrobatics"`,
    /// `"lore:underworld"` — exactly what `BaseStats.skills[].name` carries.
    pub name: String,
    pub total: i32,
}

/// One caster block's base values (contract §1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasterBase {
    pub caster_key: String,
    pub spell_attack: i32,
    pub spell_dc: i32,
    /// Verbatim from the block (contract §3): an innate caster ranks by her
    /// own block, and the sheet renders the badge.
    pub innate: bool,
}

/// The sheet's stat block (contract §1 `stats`). `class_dc` is `None` when
/// the character has none — the slot still exists in output, with nulls
/// carried through and provenance still accounted (contract rules, SC-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub ac: i32,
    pub fort: i32,
    /// The Reflex save; wire key `"ref"`.
    #[serde(rename = "ref")]
    pub reflex: i32,
    pub will: i32,
    pub perception: i32,
    /// Land speed in feet.
    pub speed: i32,
    pub class_dc: Option<i32>,
    pub strikes: Vec<StrikeBase>,
    pub casters: Vec<CasterBase>,
    pub skills: Vec<SkillBase>,
}

/// Engine input, per character (contract §1): one resolved base value per
/// stat instance, plain data — plus the render inputs that carry no
/// modifier math and ride the same extraction (design D3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseStats {
    pub schema: String,
    /// `base_sheet.identity.level + live level_adjust`, clamped 1..=20.
    pub level: i32,
    pub stats: Stats,
    /// Render inputs with no modifier math (contract §3 `render_base`):
    /// computed by the extractor, copied to the output verbatim. Defaults
    /// so older in-process JSON still parses; the extractor always sets it.
    #[serde(default)]
    pub render_base: RenderBase,
}

/// The six ability MODIFIERS (`⌊(score − 10) / 2⌋`), render inputs only —
/// no modifier can target them (closed `StatName`), so no provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Attributes {
    pub r#str: i32,
    pub dex: i32,
    pub con: i32,
    pub int: i32,
    pub wis: i32,
    pub cha: i32,
}

/// Render inputs with no modifier math (contract §3 `render_base`): the
/// extractor computes them server-side; consumers render, never compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RenderBase {
    /// Effective display level (clamped 1..=20) — same value as
    /// [`BaseStats::level`], carried so the render input set is complete.
    pub level: i32,
    /// The prototype's formula: ancestry + bonus HP, plus (class HP +
    /// CON mod + per-level bonus) at EVERY level — unlike the stored
    /// `base_sheet.hp.max_hp` anchor (which floors the per-level term at
    /// the export level); the deviation is named in design D3 and pinned
    /// by the extraction golden.
    pub hp_max: i32,
    /// Verbatim `base_sheet.focus_points`.
    pub focus_max: i32,
    /// Constant 3 at POC (Hero Points).
    pub hero_max: i32,
    /// `⌈level / 2⌉` — the cantrip rank the spell slots pane renders.
    pub cantrip_rank: i32,
    pub attributes: Attributes,
}

impl BaseStats {
    /// The character's stat instance set (Q1/Q2): strike keys, caster keys,
    /// and the sheet's skill names lifted into full `skill:`-text stats.
    /// Blanket expansion and per-instance fan-out are functions of this set.
    #[must_use]
    pub fn stat_instances(&self) -> StatInstances {
        StatInstances {
            strikes: self.stats.strikes.iter().map(|s| s.key.clone()).collect(),
            casters: self
                .stats
                .casters
                .iter()
                .map(|c| c.caster_key.clone())
                .collect(),
            skills: self
                .stats
                .skills
                .iter()
                .filter_map(|skill| StatName::from_skill_instance(&skill.name))
                .collect(),
        }
    }
}

/// Why a modifier lost its stacking contest (contract §3 — engine output,
/// never UI inference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    /// A same-type bonus lost to a higher one.
    SameTypeLowerBonus,
    /// A same-type penalty lost to a worse (lighter) one.
    SameTypeLighterPenalty,
    /// Equal values: the stable winner is `(effect_id, ord)` order (D8) —
    /// the total is identical either way, the output is not.
    SameTypeTie,
}

/// One applied modifier's provenance (contract §3 `applied`): every
/// modifier that contributed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    #[serde(rename = "type")]
    pub modifier_type: ModifierType,
    pub value: i32,
    pub effect_id: i64,
    pub effect_name: String,
    pub source_character_id: i64,
}

/// One suppressed modifier's provenance (contract §3 `suppressed`): which
/// rule suppressed it, and which effect won.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressedEntry {
    #[serde(rename = "type")]
    pub modifier_type: ModifierType,
    pub value: i32,
    pub effect_id: i64,
    pub effect_name: String,
    pub source_character_id: i64,
    pub reason: SuppressionReason,
    /// The deterministically-chosen winner (ties: by effect id).
    pub suppressed_by_effect_id: Option<i64>,
}

/// One stat instance's derived output: base, total, and both provenance
/// lists. `applied + suppressed` accounts for every modifier that expanded
/// to this instance — nothing silently vanishes (SC-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatOutput {
    pub base: i32,
    pub total: i32,
    pub applied: Vec<ProvenanceEntry>,
    pub suppressed: Vec<SuppressedEntry>,
}

/// The `class_dc` slot when the character has none (contract §3): nulls are
/// carried through — nothing is invented — while the provenance lists still
/// account for every modifier addressed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NullableStatOutput {
    pub base: Option<i32>,
    pub total: Option<i32>,
    pub applied: Vec<ProvenanceEntry>,
    pub suppressed: Vec<SuppressedEntry>,
}

/// One strike's derived output (contract §3): attack and flat damage, plus
/// the display fields the row renders — riding the strike object, verbatim
/// passthrough from the input (no math, no provenance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrikeOutput {
    pub key: String,
    /// Display name.
    pub label: String,
    /// MAP step (4 Agile, else 5) — the row renders `−map / −2·map`.
    pub map: i32,
    /// The damage expression as rendered (`"d4−1"`), verbatim from the
    /// input — never parsed, never recomputed.
    pub damage_expr: String,
    /// The export's damage-type letter code.
    pub damage_type: String,
    /// Its display name.
    pub damage_type_name: String,
    /// Trait chip names, verbatim.
    pub traits: Vec<String>,
    pub attack: StatOutput,
    pub damage_flat: StatOutput,
}

/// One caster block's derived output (contract §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasterOutput {
    pub caster_key: String,
    /// Verbatim from the block — the sheet's innate badge.
    pub innate: bool,
    pub spell_attack: StatOutput,
    pub spell_dc: StatOutput,
}

/// One skill's derived output (contract §3 `skills`): name + total +
/// provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillOutput {
    /// Contract text, no `skill:` prefix.
    pub name: String,
    pub total: i32,
    pub applied: Vec<ProvenanceEntry>,
    pub suppressed: Vec<SuppressedEntry>,
}

/// The derived-number half of [`EngineOutput`] (contract §3 `derived`):
/// every slot the sheet renders, shaped exactly as the contract JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Derived {
    pub ac: StatOutput,
    pub fort: StatOutput,
    /// Wire key `"ref"`.
    #[serde(rename = "ref")]
    pub reflex: StatOutput,
    pub will: StatOutput,
    pub perception: StatOutput,
    pub speed: StatOutput,
    pub class_dc: NullableStatOutput,
    pub strikes: Vec<StrikeOutput>,
    pub casters: Vec<CasterOutput>,
    pub skills: Vec<SkillOutput>,
}

/// An effect chip (contract §3 `effects` / §4): every active effect
/// targeting this character, what the sheet's badge row renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chip {
    pub effect_id: i64,
    pub name: String,
    /// The source character's DISPLAY name (resolved by the host — the
    /// engine never sees the accounts table).
    pub source_name: String,
    pub duration_note: String,
    pub active: bool,
    pub version: i64,
    pub modifiers: Vec<Modifier>,
    /// True ⇒ display-only condition: badge, zero math (§4).
    pub tracked_manually: bool,
}

/// The engine's total output for one character (contract §3 — THE binding
/// shape). `derived` carries every number the sheet renders with its full
/// math; `effects` carries the chips; `render_base` the modifier-free
/// render inputs. Consumers render; nobody computes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineOutput {
    pub schema: String,
    pub character_id: i64,
    pub derived: Derived,
    pub effects: Vec<Chip>,
    /// Render inputs with no modifier math (contract §3 `render_base`) —
    /// outside `derived` deliberately: closed `StatName`, no provenance, no
    /// hover. Verbatim from the extraction.
    pub render_base: RenderBase,
}
