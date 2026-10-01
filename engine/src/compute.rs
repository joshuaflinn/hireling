//! `EngineOutput` assembly (spec FR-4) — the per-character computation:
//! base slots + expanded candidates in; every derived slot with applied AND
//! suppressed provenance, plus effect chips, out. Shapes are pinned by the
//! engine-output contract; if code and that document disagree, the document
//! wins until a PR changes it.
//!
//! Pure and total: no I/O, no clock, deterministic ordering (contract —
//! consumers may memo). Blankets never appear in output: they are expanded
//! to concrete instances by the stacking core before assembly.

use std::collections::BTreeMap;

use crate::model::{
    ActiveEffect, BaseStats, CasterOutput, Chip, Derived, EngineOutput, NullableStatOutput,
    OUTPUT_SCHEMA, SkillOutput, StatOutput, StrikeOutput,
};
use crate::stack::{Candidate, StackedStat, candidates, stack_one};
use crate::vocab::{SingleStat, StatName, StatRef};

/// Compute one character's derived numbers and effect chips.
///
/// `source_names` maps source character ids to DISPLAY names — resolved by
/// the host (the engine never sees the accounts table); a missing name
/// renders as an empty chip name rather than failing the whole sheet.
#[must_use]
pub fn compute(
    character_id: i64,
    base: &BaseStats,
    effects: &[ActiveEffect],
    source_names: &BTreeMap<i64, String>,
) -> EngineOutput {
    let instances = base.stat_instances();
    let expanded = candidates(effects, character_id, &instances);

    // Group once by instance; every slot then reads its own group.
    let mut groups: BTreeMap<StatRef, Vec<&Candidate>> = BTreeMap::new();
    for candidate in &expanded {
        groups
            .entry(candidate.stat_ref.clone())
            .or_default()
            .push(candidate);
    }

    let derived = derived_for(base, &groups);

    // Chips: every ACTIVE effect targeting THIS character, sorted by
    // effect_id (contract: deterministic ordering).
    let mut chips: Vec<Chip> = effects
        .iter()
        .filter(|effect| effect.active && effect.targets.contains(&character_id))
        .map(|effect| Chip {
            effect_id: effect.effect_id,
            name: effect.name.clone(),
            source_name: source_names
                .get(&effect.source_character_id)
                .cloned()
                .unwrap_or_default(),
            duration_note: effect.duration_note.clone(),
            active: effect.active,
            version: effect.version,
            modifiers: effect.modifiers.clone(),
            tracked_manually: effect.tracked_manually,
        })
        .collect();
    chips.sort_unstable_by_key(|chip| chip.effect_id);

    EngineOutput {
        schema: OUTPUT_SCHEMA.to_owned(),
        character_id,
        derived,
        effects: chips,
    }
}

/// Stack one slot against its base; an instance with no candidates is the
/// identity (base through, empty provenance).
fn stacked_for(
    groups: &BTreeMap<StatRef, Vec<&Candidate>>,
    stat_ref: &StatRef,
    base_value: i32,
) -> StackedStat {
    match groups.get(stat_ref) {
        Some(group) => stack_one(base_value, group),
        None => stack_one(base_value, &[]),
    }
}

/// Every derived slot for one sheet, from the grouped candidates.
fn derived_for(base: &BaseStats, groups: &BTreeMap<StatRef, Vec<&Candidate>>) -> Derived {
    Derived {
        ac: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Ac),
            base.stats.ac,
        )),
        fort: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Fort),
            base.stats.fort,
        )),
        reflex: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Ref),
            base.stats.reflex,
        )),
        will: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Will),
            base.stats.will,
        )),
        perception: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Perception),
            base.stats.perception,
        )),
        speed: stat_output(stacked_for(
            groups,
            &StatRef::Global(SingleStat::Speed),
            base.stats.speed,
        )),
        class_dc: nullable_output(
            stacked_for(
                groups,
                &StatRef::Global(SingleStat::ClassDc),
                base.stats.class_dc.unwrap_or(0),
            ),
            base.stats.class_dc,
        ),
        strikes: base
            .stats
            .strikes
            .iter()
            .map(|strike| StrikeOutput {
                key: strike.key.clone(),
                attack: stat_output(stacked_for(
                    groups,
                    &StatRef::StrikeAttack(strike.key.clone()),
                    strike.attack,
                )),
                damage_flat: stat_output(stacked_for(
                    groups,
                    &StatRef::StrikeDamage(strike.key.clone()),
                    strike.damage_flat,
                )),
            })
            .collect(),
        casters: base
            .stats
            .casters
            .iter()
            .map(|caster| CasterOutput {
                caster_key: caster.caster_key.clone(),
                spell_attack: stat_output(stacked_for(
                    groups,
                    &StatRef::CasterSpellAttack(caster.caster_key.clone()),
                    caster.spell_attack,
                )),
                spell_dc: stat_output(stacked_for(
                    groups,
                    &StatRef::CasterSpellDc(caster.caster_key.clone()),
                    caster.spell_dc,
                )),
            })
            .collect(),
        skills: base
            .stats
            .skills
            .iter()
            .map(|skill| {
                // A non-canonical sheet name has no instance and no
                // matching modifier: the row still renders (base through,
                // empty provenance) — the extractor's normalization is
                // pinned by the golden test in engine_host.
                let stacked = StatName::from_skill_instance(&skill.name)
                    .map(|name| stacked_for(groups, &StatRef::Skill(name), skill.total));
                match stacked {
                    Some(stacked) => SkillOutput {
                        name: skill.name.clone(),
                        total: stacked.total,
                        applied: stacked.applied,
                        suppressed: stacked.suppressed,
                    },
                    None => SkillOutput {
                        name: skill.name.clone(),
                        total: skill.total,
                        applied: Vec::new(),
                        suppressed: Vec::new(),
                    },
                }
            })
            .collect(),
    }
}

/// The stacked slot as contract output.
fn stat_output(stacked: StackedStat) -> StatOutput {
    StatOutput {
        base: stacked.base,
        total: stacked.total,
        applied: stacked.applied,
        suppressed: stacked.suppressed,
    }
}

/// A nullable slot as contract output: nulls carried through (nothing
/// invented), provenance accounted regardless (SC-4).
fn nullable_output(stacked: StackedStat, base: Option<i32>) -> NullableStatOutput {
    NullableStatOutput {
        base,
        total: base.map(|_| stacked.total),
        applied: stacked.applied,
        suppressed: stacked.suppressed,
    }
}
