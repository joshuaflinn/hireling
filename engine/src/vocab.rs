//! The closed stat vocabulary — spec FR-2, the one source (design D2).
//!
//! A modifier addresses exactly one of: one of the 11 single stats, one of
//! the 3 blanket targets, or `skill:<name>` (core skills bare, lores as
//! `lore:<name>`). Anything else is a loud validation error at the boundary,
//! never a silent no-op. Blanket expansions are DATA in [`BLANKET_EXPANSIONS`]
//! and [`expand`] — never per-stat match arms (review criterion; spec FR-2).
//!
//! Canonical text: lowercase, `snake_case`, no spaces. [`Stat::parse`]
//! rejects anything else so a typo can never become an unstorable-looking
//! but silently-matching stat string in `effect_modifiers.stat` or on the
//! wire.

use std::fmt;

/// The 11 single stats (spec's vocabulary table; also E4's seed list).
pub const SINGLE_STATS: [&str; 11] = [
    "ac",
    "fort",
    "ref",
    "will",
    "perception",
    "speed",
    "attack",
    "damage",
    "spell_attack",
    "spell_dc",
    "class_dc",
];

/// The 18 core skill keys — the export's skill-like proficiency keys (design
/// Task 1: "the export's 18"). The 16 the prototype renders plus `piloting`
/// and `computers`, which Pathbuilder always exports.
pub const CORE_SKILLS: [&str; 18] = [
    "acrobatics",
    "arcana",
    "athletics",
    "crafting",
    "deception",
    "diplomacy",
    "intimidation",
    "medicine",
    "nature",
    "occultism",
    "performance",
    "religion",
    "society",
    "stealth",
    "survival",
    "thievery",
    "piloting",
    "computers",
];

/// The fixed ability score each core skill rolls with — the prototype's
/// SKILLS table, extended with the two unrendered exports (piloting: dex,
/// computers: int). Extraction and display may need the pairing; stacking
/// never does (values are precomputed totals in `BaseStats`).
pub const CORE_SKILL_ABILITY: [(&str, &str); 18] = [
    ("acrobatics", "dex"),
    ("arcana", "int"),
    ("athletics", "str"),
    ("crafting", "int"),
    ("deception", "cha"),
    ("diplomacy", "cha"),
    ("intimidation", "cha"),
    ("medicine", "wis"),
    ("nature", "wis"),
    ("occultism", "int"),
    ("performance", "cha"),
    ("religion", "wis"),
    ("society", "int"),
    ("stealth", "dex"),
    ("survival", "wis"),
    ("thievery", "dex"),
    ("piloting", "dex"),
    ("computers", "int"),
];

/// A stat name as stored on the wire and in the database: canonical text,
/// parseable by [`Stat::parse`]. Newtype so a raw `String` can't sneak past
/// validation into `effect_modifiers.stat`.
///
/// Invariant: inside [`Stat::Skill`] the text is the FULL stat string —
/// `"skill:acrobatics"`, `"skill:lore:underworld"` — so `as_str()` is
/// always the wire form. The instance name a sheet carries
/// (`BaseStats.skills[].name`) is the same text minus its `skill:` prefix
/// (see [`StatName::instance_name`]).
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct StatName(String);

impl StatName {
    /// The canonical wire/DB text (`"skill:acrobatics"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The text a `BaseStats.skills[].name` carries for this instance —
    /// the wire string minus its `skill:` prefix (`"acrobatics"`,
    /// `"lore:underworld"`).
    #[must_use]
    pub fn instance_name(&self) -> &str {
        self.0.strip_prefix("skill:").unwrap_or(&self.0)
    }
}

impl fmt::Display for StatName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One addressable stat a modifier may name. Closed: the 11 single stats,
/// the 3 blankets, and `skill:<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stat {
    /// One of [`SINGLE_STATS`].
    Single(SingleStat),
    /// One of the three blanket targets — expanded before stacking
    /// ([`expand`]), never evaluated against the blanket itself.
    Blanket(Blanket),
    /// `skill:<name>` — core skills bare, lores as `lore:<name>` (design D9).
    Skill(StatName),
}

/// The 11 single stats, typed. [`SingleStat::ALL`] mirrors [`SINGLE_STATS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SingleStat {
    Ac,
    Fort,
    Ref,
    Will,
    Perception,
    Speed,
    Attack,
    Damage,
    SpellAttack,
    SpellDc,
    ClassDc,
}

impl SingleStat {
    /// All eleven, in [`SINGLE_STATS`] order.
    pub const ALL: [SingleStat; 11] = [
        SingleStat::Ac,
        SingleStat::Fort,
        SingleStat::Ref,
        SingleStat::Will,
        SingleStat::Perception,
        SingleStat::Speed,
        SingleStat::Attack,
        SingleStat::Damage,
        SingleStat::SpellAttack,
        SingleStat::SpellDc,
        SingleStat::ClassDc,
    ];

    /// The canonical wire/DB text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SingleStat::Ac => "ac",
            SingleStat::Fort => "fort",
            SingleStat::Ref => "ref",
            SingleStat::Will => "will",
            SingleStat::Perception => "perception",
            SingleStat::Speed => "speed",
            SingleStat::Attack => "attack",
            SingleStat::Damage => "damage",
            SingleStat::SpellAttack => "spell_attack",
            SingleStat::SpellDc => "spell_dc",
            SingleStat::ClassDc => "class_dc",
        }
    }

    /// Parse from canonical text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        SINGLE_STATS
            .iter()
            .position(|candidate| *candidate == text)
            .and_then(|index| Self::ALL.get(index))
            .copied()
    }

    /// Whether this stat is per-instance (Q1: strikes / caster blocks) —
    /// expansion fans one modifier out across every instance on the sheet.
    #[must_use]
    pub fn is_per_instance(self) -> bool {
        matches!(
            self,
            SingleStat::Attack | SingleStat::Damage | SingleStat::SpellAttack | SingleStat::SpellDc
        )
    }
}

/// The three blanket targets (spec's vocabulary table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Blanket {
    /// Every d20 check: attacks, spell attacks, saves, Perception, skills.
    /// NOT damage, NOT speed.
    AllChecks,
    /// Every DC: `ac`, `class_dc`, `spell_dc` (AC is a DC — Player Core).
    AllDcs,
    /// The union of the two sets above — *frightened*'s footprint.
    AllChecksAndDcs,
}

impl Blanket {
    /// Parse from canonical text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "all_checks" => Some(Self::AllChecks),
            "all_dcs" => Some(Self::AllDcs),
            "all_checks_and_dcs" => Some(Self::AllChecksAndDcs),
            _ => None,
        }
    }

    /// The canonical wire/DB text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Blanket::AllChecks => "all_checks",
            Blanket::AllDcs => "all_dcs",
            Blanket::AllChecksAndDcs => "all_checks_and_dcs",
        }
    }
}

/// The four modifier types the engine stacks over (E2's CHECK, seed data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierType {
    Circumstance,
    Status,
    Item,
    Untyped,
}

impl ModifierType {
    /// Parse from canonical text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "circumstance" => Some(Self::Circumstance),
            "status" => Some(Self::Status),
            "item" => Some(Self::Item),
            "untyped" => Some(Self::Untyped),
            _ => None,
        }
    }

    /// The canonical wire/DB text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ModifierType::Circumstance => "circumstance",
            ModifierType::Status => "status",
            ModifierType::Item => "item",
            ModifierType::Untyped => "untyped",
        }
    }
}

impl Stat {
    /// Parse one canonical stat string, loud on anything outside the closed
    /// vocabulary (FR-2). The reason text is human-readable: it surfaces as
    /// the write path's `rejected` reason.
    ///
    /// # Errors
    ///
    /// A descriptive string for unknown stats, non-canonical names
    /// (uppercase, spaces), a bare `skill:` / `skill:lore:` prefix, or a
    /// lore name missing its `lore:` prefix.
    pub fn parse(text: &str) -> Result<Self, String> {
        if let Some(single) = SingleStat::parse(text) {
            return Ok(Self::Single(single));
        }
        if let Some(blanket) = Blanket::parse(text) {
            return Ok(Self::Blanket(blanket));
        }
        if let Some(name) = text.strip_prefix("skill:") {
            return Self::parse_skill_name(name).map(Self::Skill);
        }
        Err(format!(
            "`{text}` is outside the closed stat vocabulary \
             (single stats, blanket targets, or `skill:<name>`)"
        ))
    }

    /// Validate a skill name after `skill:`: non-empty, canonical lowercase,
    /// and a lore name always carries its `lore:` prefix.
    fn parse_skill_name(name: &str) -> Result<StatName, String> {
        if name.is_empty() {
            return Err("`skill:` must be followed by a skill name".to_owned());
        }
        let (prefix, bare) = match name.strip_prefix("lore:") {
            Some(bare) => ("lore:", bare),
            None => ("", name),
        };
        if bare.is_empty() {
            return Err("`skill:lore:` must be followed by the lore's name".to_owned());
        }
        if !canonical(bare) {
            return Err(format!(
                "`skill:{name}` is not canonical: use lowercase letters, digits, \
                 and underscores (the character's own skill list is the source of names)"
            ));
        }
        Ok(StatName(format!("skill:{prefix}{bare}")))
    }

    /// The canonical wire/DB text (`"ac"`, `"skill:acrobatics"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Single(single) => single.as_str(),
            Self::Blanket(blanket) => blanket.as_str(),
            Self::Skill(name) => name.as_str(),
        }
    }
}

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical-name charset: ASCII lowercase letters, digits, underscores.
fn canonical(name: &str) -> bool {
    name.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    })
}

/// The per-instance stat axes of ONE character sheet (Q1/Q2 settled): which
/// strikes, which caster blocks, which skills exist here. The expansion and
/// the per-instance fan-out are functions of this set — a modifier naming a
/// stat with no instance on the sheet matches nothing (spec's edge case).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatInstances {
    /// Strike keys, in `BaseStats.strikes` order.
    pub strikes: Vec<String>,
    /// Caster keys, in `BaseStats.casters` order.
    pub casters: Vec<String>,
    /// Skill names, canonical (`acrobatics`, `lore:underworld`), in
    /// `BaseStats.skills` order — exactly the character's own set.
    pub skills: Vec<StatName>,
}

/// One concrete stat instance on a sheet — the address a modifier expands
/// to before stacking. `as_str()` names the stat vocabulary entry; the
/// instance rides the variant (which strike, which caster, which skill).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatRef {
    /// A sheet-global single stat (`ac`, saves, perception, speed,
    /// `class_dc`).
    Global(SingleStat),
    /// One strike's attack roll.
    StrikeAttack(String),
    /// One strike's flat damage.
    StrikeDamage(String),
    /// One caster block's spell attack.
    CasterSpellAttack(String),
    /// One caster block's spell DC.
    CasterSpellDc(String),
    /// One skill instance (core or lore) the sheet carries.
    Skill(StatName),
}

impl StatRef {
    /// The stat vocabulary text this instance computes
    /// (`"attack"`, `"skill:lore:underworld"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            StatRef::Global(single) => single.as_str(),
            StatRef::StrikeAttack(_) => "attack",
            StatRef::StrikeDamage(_) => "damage",
            StatRef::CasterSpellAttack(_) => "spell_attack",
            StatRef::CasterSpellDc(_) => "spell_dc",
            StatRef::Skill(name) => name.as_str(),
        }
    }
}

/// The blanket expansion table — DATA, consumed by [`expand`] (FR-2: never
/// per-stat special cases). A member is either a single stat or a skill
/// instance of the sheet; per-instance stats (`attack`, `spell_attack`) fan out
/// over every instance at expansion time.
///
/// Order here is output order everywhere: consumers may memo on it
/// (contract: deterministic ordering).
pub const BLANKET_EXPANSIONS: [(Blanket, &[BlanketMember]); 3] = [
    (
        Blanket::AllChecks,
        &[
            BlanketMember::PerInstance(SingleStat::Attack),
            BlanketMember::PerInstance(SingleStat::SpellAttack),
            BlanketMember::Single(SingleStat::Fort),
            BlanketMember::Single(SingleStat::Ref),
            BlanketMember::Single(SingleStat::Will),
            BlanketMember::Single(SingleStat::Perception),
            BlanketMember::Skills,
        ],
    ),
    (
        Blanket::AllDcs,
        &[
            BlanketMember::Single(SingleStat::Ac),
            BlanketMember::Single(SingleStat::ClassDc),
            BlanketMember::PerInstance(SingleStat::SpellDc),
        ],
    ),
    (
        Blanket::AllChecksAndDcs,
        &[
            BlanketMember::PerInstance(SingleStat::Attack),
            BlanketMember::PerInstance(SingleStat::SpellAttack),
            BlanketMember::Single(SingleStat::Fort),
            BlanketMember::Single(SingleStat::Ref),
            BlanketMember::Single(SingleStat::Will),
            BlanketMember::Single(SingleStat::Perception),
            BlanketMember::Skills,
            BlanketMember::Single(SingleStat::Ac),
            BlanketMember::Single(SingleStat::ClassDc),
            BlanketMember::PerInstance(SingleStat::SpellDc),
        ],
    ),
];

/// One row of a blanket's expansion table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlanketMember {
    /// A sheet-global single stat: one instance per sheet.
    Single(SingleStat),
    /// A per-instance stat: every strike / caster block on the sheet.
    PerInstance(SingleStat),
    /// Every skill instance the sheet carries (core skills are present on
    /// every sheet; lores join per character — Q2).
    Skills,
}

/// Expand a blanket into its concrete member instances for ONE sheet's
/// instance set — pure, total, deterministic. `damage` and `speed` appear in
/// no table (WEx-5 asserts the absence).
#[must_use]
pub fn expand(blanket: Blanket, instances: &StatInstances) -> Vec<StatRef> {
    let mut expanded = Vec::new();
    let Some((_, members)) = BLANKET_EXPANSIONS
        .iter()
        .find(|(candidate, _)| *candidate == blanket)
    else {
        return expanded;
    };
    for member in *members {
        match *member {
            BlanketMember::Single(single) => expanded.push(StatRef::Global(single)),
            BlanketMember::PerInstance(single) => match single {
                SingleStat::Attack => {
                    expanded.extend(instances.strikes.iter().cloned().map(StatRef::StrikeAttack));
                }
                SingleStat::Damage => {
                    expanded.extend(instances.strikes.iter().cloned().map(StatRef::StrikeDamage));
                }
                SingleStat::SpellAttack => {
                    expanded.extend(
                        instances
                            .casters
                            .iter()
                            .cloned()
                            .map(StatRef::CasterSpellAttack),
                    );
                }
                SingleStat::SpellDc => {
                    expanded.extend(
                        instances
                            .casters
                            .iter()
                            .cloned()
                            .map(StatRef::CasterSpellDc),
                    );
                }
                SingleStat::Ac
                | SingleStat::Fort
                | SingleStat::Ref
                | SingleStat::Will
                | SingleStat::Perception
                | SingleStat::Speed
                | SingleStat::ClassDc => {
                    expanded.push(StatRef::Global(single));
                }
            },
            BlanketMember::Skills => {
                expanded.extend(instances.skills.iter().cloned().map(StatRef::Skill));
            }
        }
    }
    expanded
}
