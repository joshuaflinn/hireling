//! Property tests (plan Task 3) — the global invariants the `WEx` examples
//! cannot cover one-by-one (spec FR-10):
//!
//! - **Order independence**: shuffling the effect list cannot change the
//!   stacking result — the DB's row order is not an input (design D8: ties
//!   break by `(effect_id, ord)`, identity, never position).
//! - **Conservation**: over all stat instances, `applied ∪ suppressed`
//!   accounts for every expanded candidate — nothing silently vanishes,
//!   nothing appears twice (SC-4).
//! - **Arithmetic**: `total == base + Σ applied values`, per instance.
//! - **Expansion closure**: blanket expansion over generated sheets matches
//!   the design's member table — written out independently here, NOT read
//!   from `BLANKET_EXPANSIONS`, so editing the data table wrong fails this
//!   suite (spec: expansion sets are data with a pinned shape; `damage` and
//!   `speed` appear in no blanket, ever — WEx-5 pins one example, this pins
//!   all of them).
//!
//! CI config: ≥256 cases per property (plan Task 3 done-when), set loudly
//! below.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live at crate root by cargo convention"
)]

use std::collections::BTreeMap;

use hireling_engine::model::{ActiveEffect, Modifier, ModifierType};
use hireling_engine::stack::{Candidate, StackedStat, candidates, stack_one};
use hireling_engine::vocab::{
    Blanket, CORE_SKILLS, SingleStat, StatInstances, StatName, StatRef, expand,
};
use proptest::prelude::*;

/// The character every generated effect targets (when targeted at all).
const CHARACTER_ID: i64 = 42;
/// A source character id for generated effects.
const SOURCE_ID: i64 = 7;
/// Cases per property — the plan's CI floor, stated not implied.
const CASES: u32 = 256;

// ---------------------------------------------------------------- strategies

/// Every valid stat text: the 11 single stats, the 3 blankets, core skills
/// bare, and two lore names. Drawn from the typed API so a vocabulary edit
/// keeps the strategy in step.
fn valid_stat_texts() -> Vec<String> {
    let mut texts: Vec<String> = SingleStat::ALL
        .iter()
        .map(|single| single.as_str().to_owned())
        .collect();
    texts.extend(
        [
            Blanket::AllChecks,
            Blanket::AllDcs,
            Blanket::AllChecksAndDcs,
        ]
        .iter()
        .map(|blanket| blanket.as_str().to_owned()),
    );
    texts.extend(CORE_SKILLS.iter().map(|skill| format!("skill:{skill}")));
    texts.push("skill:lore:underworld".to_owned());
    texts.push("skill:lore:fortune_telling".to_owned());
    texts
}

/// The candidate skill-instance names a generated sheet may carry: the 18
/// core skills plus two lores.
fn skill_instance_candidates() -> Vec<String> {
    let mut instances: Vec<String> = CORE_SKILLS
        .iter()
        .map(|skill| (*skill).to_owned())
        .collect();
    instances.push("lore:lore_0".to_owned());
    instances.push("lore:lore_1".to_owned());
    instances
}

/// Builds modifiers from strategy-selected stat text. The text set is
/// generated from the typed vocabulary API, so a parse failure here is a
/// strategy bug, not runtime input — hence the expect.
#[expect(
    clippy::expect_used,
    reason = "fixture helper in a test crate: a strategy typo must fail loudly"
)]
fn modifier_strategy() -> impl Strategy<Value = Modifier> {
    (
        prop::sample::select(vec![
            ModifierType::Circumstance,
            ModifierType::Status,
            ModifierType::Item,
            ModifierType::Untyped,
        ]),
        prop::sample::select(valid_stat_texts()),
        -50_i32..=50,
    )
        .prop_map(|(modifier_type, stat, value)| Modifier {
            modifier_type,
            stat: StatName::try_from(stat).expect("strategy only emits valid stat text"),
            value,
        })
}

/// One generated effect's payload; identity is assigned later, by seed
/// position (see `build_effects`).
type EffectSeed = Vec<Modifier>;

fn effect_list_strategy() -> impl Strategy<Value = Vec<EffectSeed>> {
    prop::collection::vec(prop::collection::vec(modifier_strategy(), 0..=5), 0..=8)
}

/// A generated sheet: 0–3 strikes, 0–2 caster blocks, and an arbitrary
/// subset of the 18 core skills plus two lores.
fn instances_strategy() -> impl Strategy<Value = StatInstances> {
    (
        0_usize..=3,
        0_usize..=2,
        prop::collection::vec(any::<bool>(), CORE_SKILLS.len() + 2),
    )
        .prop_map(|(n_strikes, n_casters, skill_mask)| {
            let strikes = (0..n_strikes).map(|i| format!("weapon{i}")).collect();
            let casters = (0..n_casters).map(|i| format!("caster{i}")).collect();
            let skills: Vec<StatName> = skill_instance_candidates()
                .iter()
                .zip(&skill_mask)
                .filter_map(|(instance, keep)| {
                    keep.then(|| instance.clone())
                        .and_then(|name| StatName::from_skill_instance(&name))
                })
                .collect();
            StatInstances {
                strikes,
                casters,
                skills,
            }
        })
}

/// Build the effect list: identity (`effect_id`) assigned by SEED position,
/// so shuffling the array cannot change any effect's identity — the
/// order-independence property then isolates pure input order.
fn build_effects(seeds: &[EffectSeed]) -> Vec<ActiveEffect> {
    seeds
        .iter()
        .enumerate()
        .map(|(index, modifiers)| ActiveEffect {
            effect_id: i64::try_from(index).unwrap_or_default(),
            name: format!("effect {index}"),
            source_character_id: SOURCE_ID,
            targets: vec![CHARACTER_ID],
            modifiers: modifiers.clone(),
            duration_note: String::new(),
            active: true,
            version: 1,
            tracked_manually: false,
        })
        .collect()
}

/// Shuffle `effects` by generated sort keys — a full permutation driven by
/// the case's random state (stable on key ties, which collapses to identity
/// order). No indexing: the sort keys ride beside their effects.
fn shuffled_by_keys(effects: &[ActiveEffect], keys: &[u64]) -> Vec<ActiveEffect> {
    let mut keyed: Vec<(u64, usize, &ActiveEffect)> = std::iter::zip(
        keys.iter().copied().chain(std::iter::repeat(u64::MAX)),
        effects.iter(),
    )
    .enumerate()
    .map(|(index, (key, effect))| (key, index, effect))
    .collect();
    keyed.sort_by_key(|(key, index, _)| (*key, *index));
    keyed
        .into_iter()
        .map(|(_, _, effect)| effect.clone())
        .collect()
}

// -------------------------------------------------------------------- driver

/// Stack every instance the generated candidates land on. Base 0
/// everywhere: these properties are about stacking and accounting, not
/// assembly (Task 4's job), and the arithmetic property reads `base` off
/// the result so it stays true for any base.
fn stack_all(effects: &[ActiveEffect], instances: &StatInstances) -> Vec<(StatRef, StackedStat)> {
    let expanded = candidates(effects, CHARACTER_ID, instances);
    let mut groups: BTreeMap<StatRef, Vec<&Candidate>> = BTreeMap::new();
    for candidate in &expanded {
        groups
            .entry(candidate.stat_ref.clone())
            .or_default()
            .push(candidate);
    }
    groups
        .into_iter()
        .map(|(stat_ref, group)| {
            let stacked = stack_one(0, &group);
            (stat_ref, stacked)
        })
        .collect()
}

// ---------------------------------------------------------------- properties

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(CASES))]

    /// (a) Order independence: a shuffled effect list yields a byte-identical
    /// result. The DB returns rows in arbitrary order; the engine's answer
    /// may not depend on it (design D8).
    #[test]
    fn order_independence_shuffled_effect_lists_give_identical_output(
        seeds in effect_list_strategy(),
        instances in instances_strategy(),
        keys in prop::collection::vec(any::<u64>(), 0..=8),
    ) {
        let effects = build_effects(&seeds);
        let baseline = stack_all(&effects, &instances);
        let reshuffled = shuffled_by_keys(&effects, &keys);
        let after_shuffle = stack_all(&reshuffled, &instances);
        prop_assert_eq!(format!("{baseline:?}"), format!("{after_shuffle:?}"),
            "stacking result changed when only the effect list's order changed");
    }

    /// (b) Conservation: across all instances, every expanded candidate is
    /// applied or suppressed exactly once — nothing vanishes (SC-4).
    /// Found and killed a real double-count: an untyped +0 was emitted by
    /// both the zero loop and the untyped loop (`stack_one`), pinned by
    /// `zero_value_modifiers_apply_exactly_once` in `wex.rs`.
    #[test]
    fn conservation_applied_plus_suppressed_equals_every_expanded_candidate(
        seeds in effect_list_strategy(),
        instances in instances_strategy(),
    ) {
        let effects = build_effects(&seeds);
        let expanded = candidates(&effects, CHARACTER_ID, &instances);
        // Multiplicity per (instance, effect): one effect may carry several
        // modifiers onto the same instance; each must be accounted once.
        let mut accounted: BTreeMap<(StatRef, i64), usize> = BTreeMap::new();
        let mut per_instance: BTreeMap<StatRef, usize> = BTreeMap::new();
        for candidate in &expanded {
            *accounted
                .entry((candidate.stat_ref.clone(), candidate.effect_id))
                .or_default() += 1;
            *per_instance.entry(candidate.stat_ref.clone()).or_default() += 1;
        }
        for (stat_ref, stacked) in stack_all(&effects, &instances) {
            let expanded_here = per_instance.get(&stat_ref).copied().unwrap_or(0);
            prop_assert_eq!(
                stacked.applied.len() + stacked.suppressed.len(),
                expanded_here,
                "instance {:?}: applied+suppressed != expanded count",
                stat_ref,
            );
            for entry in &stacked.applied {
                *accounted.entry((stat_ref.clone(), entry.effect_id)).or_default() -= 1;
            }
            for entry in &stacked.suppressed {
                *accounted.entry((stat_ref.clone(), entry.effect_id)).or_default() -= 1;
            }
        }
        prop_assert!(
            accounted.values().all(|remaining| *remaining == 0),
            "unaccounted candidates remain: {accounted:?}",
        );
    }

    /// (c) Arithmetic: `total == base + Σ applied values`, per instance —
    /// suppressed modifiers contribute nothing.
    #[test]
    fn total_is_base_plus_sum_of_applied(
        seeds in effect_list_strategy(),
        instances in instances_strategy(),
    ) {
        let effects = build_effects(&seeds);
        for (stat_ref, stacked) in stack_all(&effects, &instances) {
            let applied_sum: i32 = stacked.applied.iter().map(|entry| entry.value).sum();
            prop_assert_eq!(stacked.total, stacked.base + applied_sum,
                "instance {:?}: total is not base + applied sum", stat_ref);
        }
    }

    /// (d) Expansion closure: for generated sheets, each blanket expands to
    /// exactly the design's member table — checked against an independent
    /// construction, never against `BLANKET_EXPANSIONS` itself, and never
    /// containing `damage` or `speed` instances (WEx-5's rule, generalized).
    #[test]
    fn expansion_closure_matches_the_design_member_table(
        instances in instances_strategy(),
    ) {
        for blanket in [Blanket::AllChecks, Blanket::AllDcs, Blanket::AllChecksAndDcs] {
            let got = expand(blanket, &instances);
            let expected = design_expansion(blanket, &instances);
            for stat_ref in &got {
                let text = stat_ref.as_str();
                prop_assert!(text != "damage" && text != "speed",
                    "{:?} expanded onto {:?} — damage and speed are in no blanket", blanket, text);
            }
            prop_assert_eq!(got, expected,
                "{:?} expansion over the generated sheet left the design table", blanket);
        }
    }
}

/// The design's member table, written out from `design.md`'s expansion
/// section — deliberately independent of the data table under test.
fn design_expansion(blanket: Blanket, sheet: &StatInstances) -> Vec<StatRef> {
    let checks = |target_sheet: &StatInstances| {
        let mut members: Vec<StatRef> = sheet
            .strikes
            .iter()
            .cloned()
            .map(StatRef::StrikeAttack)
            .collect();
        members.extend(
            target_sheet
                .casters
                .iter()
                .cloned()
                .map(StatRef::CasterSpellAttack),
        );
        members.push(StatRef::Global(SingleStat::Fort));
        members.push(StatRef::Global(SingleStat::Ref));
        members.push(StatRef::Global(SingleStat::Will));
        members.push(StatRef::Global(SingleStat::Perception));
        members.extend(target_sheet.skills.iter().cloned().map(StatRef::Skill));
        members
    };
    let dcs = |target_sheet: &StatInstances| {
        let mut members = vec![
            StatRef::Global(SingleStat::Ac),
            StatRef::Global(SingleStat::ClassDc),
        ];
        members.extend(
            target_sheet
                .casters
                .iter()
                .cloned()
                .map(StatRef::CasterSpellDc),
        );
        members
    };
    match blanket {
        Blanket::AllChecks => checks(sheet),
        Blanket::AllDcs => dcs(sheet),
        Blanket::AllChecksAndDcs => {
            let mut members = checks(sheet);
            members.extend(dcs(sheet));
            members
        }
    }
}
