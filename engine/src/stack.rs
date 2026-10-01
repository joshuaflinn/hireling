//! The stacking core — the whole rules engine (spec FR-3), five rules:
//!
//! 1. Bonuses of a type: the highest applies once; same-type losers are
//!    suppressed (`same-type-lower-bonus`, or `same-type-tie` on equal
//!    values).
//! 2. Penalties of a type: the worst applies once; lighter ones suppressed
//!    (`same-type-lighter-penalty`, ties as above). Bonuses and penalties
//!    of a type resolve separately and BOTH apply (WEx-4).
//! 3. Untyped: stacks fully, both directions (WEx-7/WEx-8).
//! 4. Blankets expand before stacking is evaluated (WEx-9) — expansion is
//!    data (`vocab::BLANKET_EXPANSIONS`), competition happens per stat
//!    instance.
//! 5. Ties break deterministically by `(effect_id, ord)` (design D8).
//!
//! Zero-value modifiers change no total but appear as applied `+0`
//! (spec's edge case) — they neither win nor lose contests, for any type.
//!
//! Everything here is total and deterministic: a shuffled candidate list
//! yields byte-identical output (property-tested, Task 3).

use crate::model::{
    ActiveEffect, ModifierType, ProvenanceEntry, SuppressedEntry, SuppressionReason,
};
use crate::vocab::{SingleStat, Stat, StatInstances, StatName, StatRef, expand};

/// One modifier resolved onto one concrete stat instance, ready to compete.
/// Flat data — the stacking decision is output, not just arithmetic (FR-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The concrete instance this candidate competes on.
    pub stat_ref: StatRef,
    pub modifier_type: ModifierType,
    /// Signed; penalties negative.
    pub value: i32,
    /// Identity — never the display name (spec edge case: two effects may
    /// share a name).
    pub effect_id: i64,
    /// The modifier's index within its effect; half of the tie-break key.
    pub ord: usize,
    pub effect_name: String,
    pub source_character_id: i64,
}

impl Candidate {
    /// The deterministic ordering key: `(effect_id, ord)` (D8).
    fn key(&self) -> (i64, usize) {
        (self.effect_id, self.ord)
    }
}

/// Expand every active effect targeting `character_id` into per-instance
/// candidates — pure, total. An effect that is ended (`active: false`),
/// does not target this character, or carries no modifiers contributes
/// nothing; a modifier naming a stat with no instance on this sheet (a lore
/// the character lacks) matches nothing and is dropped here (spec's edge
/// case — the WRITE path warns, the engine just never sees an instance).
#[must_use]
pub fn candidates(
    effects: &[ActiveEffect],
    character_id: i64,
    instances: &StatInstances,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for effect in effects {
        if !effect.active || !effect.targets.contains(&character_id) {
            continue;
        }
        for (ord, modifier) in effect.modifiers.iter().enumerate() {
            for stat_ref in fan_out(&modifier.stat, instances) {
                out.push(Candidate {
                    stat_ref,
                    modifier_type: modifier.modifier_type,
                    value: modifier.value,
                    effect_id: effect.effect_id,
                    ord,
                    effect_name: effect.name.clone(),
                    source_character_id: effect.source_character_id,
                });
            }
        }
    }
    out
}

/// Fan ONE modifier's addressed stat out to the concrete instances it lands
/// on for this sheet. Globals land on their single slot (including a
/// `class_dc` whose base is null — its slot still accounts for provenance);
/// per-instance stats land once per strike/caster block; a skill lands only
/// if the sheet carries that instance.
fn fan_out(stat: &StatName, instances: &StatInstances) -> Vec<StatRef> {
    // StatName is validated at its construction boundaries; stay total
    // rather than assuming.
    let Ok(parsed) = Stat::parse(stat.as_str()) else {
        return Vec::new();
    };
    match parsed {
        Stat::Single(single) => match single {
            SingleStat::Ac
            | SingleStat::Fort
            | SingleStat::Ref
            | SingleStat::Will
            | SingleStat::Perception
            | SingleStat::Speed
            | SingleStat::ClassDc => vec![StatRef::Global(single)],
            SingleStat::Attack => instances
                .strikes
                .iter()
                .cloned()
                .map(StatRef::StrikeAttack)
                .collect(),
            SingleStat::Damage => instances
                .strikes
                .iter()
                .cloned()
                .map(StatRef::StrikeDamage)
                .collect(),
            SingleStat::SpellAttack => instances
                .casters
                .iter()
                .cloned()
                .map(StatRef::CasterSpellAttack)
                .collect(),
            SingleStat::SpellDc => instances
                .casters
                .iter()
                .cloned()
                .map(StatRef::CasterSpellDc)
                .collect(),
        },
        Stat::Blanket(blanket) => expand(blanket, instances),
        Stat::Skill(name) => {
            if instances.skills.contains(&name) {
                vec![StatRef::Skill(name)]
            } else {
                // A lore the character doesn't have: no instance, no
                // output — nothing vanishes because nothing expanded.
                Vec::new()
            }
        }
    }
}

/// One stat instance's stacking result: base, total, and both provenance
/// lists. `applied + suppressed` accounts for every candidate that
/// competed here — nothing silently vanishes (SC-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackedStat {
    pub base: i32,
    pub total: i32,
    pub applied: Vec<ProvenanceEntry>,
    pub suppressed: Vec<SuppressedEntry>,
}

/// Stack one stat instance's candidates against its base value — rules
/// 1–5 above. Deterministic: the group's order does not matter; the result
/// (including provenance order, by `(effect_id, ord)`) does not change.
#[must_use]
pub fn stack_one(base: i32, group: &[&Candidate]) -> StackedStat {
    // Sort once: candidates compete and emit in (effect_id, ord) order, so
    // ties break by stable order (rule 5) and provenance order is the same
    // key regardless of input order.
    let mut ordered: Vec<&Candidate> = group.to_vec();
    ordered.sort_by_key(|candidate| candidate.key());

    let mut applied: Vec<ProvenanceEntry> = Vec::new();
    let mut suppressed: Vec<SuppressedEntry> = Vec::new();

    // Zero values: always applied, never competing (their contest result is
    // the same either way — +0 — so the honest output is "applied, +0").
    for candidate in ordered.iter().filter(|candidate| candidate.value == 0) {
        applied.push(provenance(candidate));
    }

    for modifier_type in [
        ModifierType::Circumstance,
        ModifierType::Status,
        ModifierType::Item,
    ] {
        let typed: Vec<&Candidate> = ordered
            .iter()
            .copied()
            .filter(|candidate| candidate.modifier_type == modifier_type)
            .collect();
        let bonuses: Vec<&Candidate> = typed
            .iter()
            .copied()
            .filter(|candidate| candidate.value > 0)
            .collect();
        let penalties: Vec<&Candidate> = typed
            .iter()
            .copied()
            .filter(|candidate| candidate.value < 0)
            .collect();
        select_extreme(&bonuses, true, &mut applied, &mut suppressed);
        select_extreme(&penalties, false, &mut applied, &mut suppressed);
    }

    // Untyped: stacks fully, bonuses and penalties alike (rule 3).
    for candidate in ordered
        .iter()
        .filter(|candidate| candidate.modifier_type == ModifierType::Untyped)
    {
        applied.push(provenance(candidate));
    }

    let total = base + applied.iter().map(|entry| entry.value).sum::<i32>();
    StackedStat {
        base,
        total,
        applied,
        suppressed,
    }
}

/// Run one same-type contest: bonuses take the highest, penalties the
/// worst; the FIRST extremum in stable order wins (ties break by
/// `(effect_id, ord)`, rule 5) and every other candidate is emitted
/// suppressed with its reason and the winner's effect id.
fn select_extreme(
    group: &[&Candidate],
    is_bonus: bool,
    applied: &mut Vec<ProvenanceEntry>,
    suppressed: &mut Vec<SuppressedEntry>,
) {
    let extremum = if is_bonus {
        group.iter().map(|candidate| candidate.value).max()
    } else {
        group.iter().map(|candidate| candidate.value).min()
    };
    let Some(extremum) = extremum else {
        return;
    };
    let Some(winner_position) = group
        .iter()
        .position(|candidate| candidate.value == extremum)
    else {
        return;
    };
    let winner_effect_id = group
        .get(winner_position)
        .map_or(0, |candidate| candidate.effect_id);

    for (index, candidate) in group.iter().enumerate() {
        if index == winner_position {
            applied.push(provenance(candidate));
        } else if candidate.value == extremum {
            // Equal values: the total is identical either way; the stable
            // winner is emitted applied, the twin suppressed (rule 5).
            suppressed.push(suppressed_entry(
                candidate,
                SuppressionReason::SameTypeTie,
                Some(winner_effect_id),
            ));
        } else if is_bonus {
            suppressed.push(suppressed_entry(
                candidate,
                SuppressionReason::SameTypeLowerBonus,
                Some(winner_effect_id),
            ));
        } else {
            suppressed.push(suppressed_entry(
                candidate,
                SuppressionReason::SameTypeLighterPenalty,
                Some(winner_effect_id),
            ));
        }
    }
}

/// The provenance row for an applied candidate.
fn provenance(candidate: &Candidate) -> ProvenanceEntry {
    ProvenanceEntry {
        modifier_type: candidate.modifier_type,
        value: candidate.value,
        effect_id: candidate.effect_id,
        effect_name: candidate.effect_name.clone(),
        source_character_id: candidate.source_character_id,
    }
}

/// The provenance row for a suppressed candidate: which rule, which winner.
fn suppressed_entry(
    candidate: &Candidate,
    reason: SuppressionReason,
    suppressed_by: Option<i64>,
) -> SuppressedEntry {
    SuppressedEntry {
        modifier_type: candidate.modifier_type,
        value: candidate.value,
        effect_id: candidate.effect_id,
        effect_name: candidate.effect_name.clone(),
        source_character_id: candidate.source_character_id,
        reason,
        suppressed_by_effect_id: suppressed_by,
    }
}
