// Base-only derivation (E6 design §4) — the bounded adapter debt that stands
// in for E8's engine. `deriveBase(baseSheet, liveState)` computes every
// derived number the sheet renders from `base_sheet` inputs plus
// `level_adjust`, with zero effects: `applied`/`suppressed` are always empty
// arrays and `effects` is always `[]` until the engine adapter lands. The
// reference implementation is the post-#33 prototype's `computeCtx`
// (docs/reference/lorum_ipsum_dashboard.html, line 1364); the output shape is
// the E8 engine-output contract
// (specs/008-buff-effect-engine/contracts/engine-output.md §3).
//
// Resolved deviation: hpMax applies Constitution and per-level bonuses at
// every level — the `PF2e` rule — and E5's transform now computes the same
// formula (contract pb-export §3.3, amended by E6's review), so the stored
// anchor, the first-import seed and this adapter all agree on one number.
//
// DELETION DATE: E8's adapter swap deletes this file (design §4) — it is not
// kept in parallel with the engine.

import { abilityMod, profBonus } from './format.js';

/** The six core skills' ability mapping (key → attr), in render order. */
const SKILL_KEYS = [
  ['acrobatics', 'dex'],
  ['arcana', 'int'],
  ['athletics', 'str'],
  ['crafting', 'int'],
  ['deception', 'cha'],
  ['diplomacy', 'cha'],
  ['intimidation', 'cha'],
  ['medicine', 'wis'],
  ['nature', 'wis'],
  ['occultism', 'int'],
  ['performance', 'cha'],
  ['religion', 'wis'],
  ['society', 'int'],
  ['stealth', 'dex'],
  ['survival', 'wis'],
  ['thievery', 'dex'],
];

/**
 * A derived stat in the contract's shape — base-only mode: no effects.
 * @param {number} base
 * @param {number} total
 * @returns {{ base: number, total: number, applied: never[], suppressed: never[] }}
 */
function stat(base, total) {
  return { base, total, applied: [], suppressed: [] };
}

/**
 * The wizard class-progression table the prototype ports (Player Core):
 * ranks the export predates but a higher display level grants.
 * @param {Record<string, number>} proficiencies
 * @param {number} level
 * @param {number} exportLevel
 */
function wizardProgression(proficiencies, level, exportLevel) {
  if (level <= exportLevel) return;
  /** @param {string} key @param {number} rank */
  const up = (key, rank) => {
    proficiencies[key] = Math.max(proficiencies[key] || 0, rank);
  };
  if (level >= 5) up('reflex', 4);
  if (level >= 7) up('castingArcane', 4);
  if (level >= 9) up('fortitude', 4);
  if (level >= 11) {
    up('perception', 4);
    up('simple', 4);
    up('unarmed', 4);
  }
  if (level >= 13) up('unarmored', 4);
  if (level >= 15) up('castingArcane', 6);
  if (level >= 17) up('will', 6);
  if (level >= 19) up('castingArcane', 8);
}

/**
 * Extra strike damage from legendary-ish weapon ranks at level 13+ (the
 * prototype's `spec`): rank 4 → +2, 6 → +3, 8 → +4.
 * @param {number} rank
 * @param {number} level
 * @returns {number}
 */
function masteryDamage(rank, level) {
  if (level < 13) return 0;
  return { 4: 2, 6: 3, 8: 4 }[rank] || 0;
}

/**
 * The base-sheet sections the base math reads. Structural truth lives in
 * the Rust transform (data-model §2); this records only the JS view of it.
 *
 * @typedef {object} BaseSheet
 * @property {{ level: number, keyability?: string | null, class?: string | null,
 *   ancestry?: string | null, heritage?: string | null, background?: string | null,
 *   alignment?: string | null, deity?: string | null, size_name?: string | null,
 *   languages: string[], name: string }} identity
 * @property {{ str: number, dex: number, con: number, int: number, wis: number, cha: number }} abilities
 * @property {{ ancestryhp: number, classhp: number, bonushp: number, bonushp_per_level: number, max_hp: number }} hp
 * @property {{ base: number, bonus: number } | null} speed
 * @property {{ acAbilityBonus?: number, acProfBonus?: number, acItemBonus?: number, acTotal?: number, shieldBonus?: number | null } | null} ac
 * @property {Array<Record<string, *>>} armor
 * @property {Record<string, number>} proficiencies
 * @property {Array<{ name: string, rank: number }>} lores
 * @property {Array<{ caster_key: string, magic_tradition?: string | null, spellcasting_type?: string | null,
 *   ability?: string | null, proficiency?: number | null, innate: boolean, focus_points: number,
 *   per_day: number[] }>} spellcasters
 * @property {Array<{ name: string, qty: number }>} equipment
 * @property {Array<Record<string, *>>} weapons
 * @property {{ attributes?: { speed?: number, speedBonus?: number } } & Record<string, *>} raw
 * @property {number} focus_points
 */

/**
 * Derive the sheet's numbers, base-only mode.
 *
 * @param {{ id: number, base_sheet: BaseSheet,
 *   item_traits?: Record<string, string[]> }} character the bootstrap payload
 *   (`item_traits` is the corpus trait map the server resolves at bootstrap)
 * @param {{ level_adjust?: number, effects?: Array<object> }} liveState the
 *   sync state this character owns, extracted by sheet/state.js
 * @returns {object} the engine-output contract's DerivedSheet (schema
 *   `hireling.engine.output.v1`) plus `level`, `cantrip_rank`, `focus_max`,
 *   and the per-strike render inputs (traits, damage expression)
 */
export function deriveBase(character, liveState = {}) {
  const base = character.base_sheet;
  const exportLevel = base.identity.level;
  const level = Math.min(20, Math.max(1, exportLevel + (liveState.level_adjust || 0)));

  const scores = base.abilities;
  const mods = /** @type {Record<string, number>} */ ({
    str: abilityMod(scores.str),
    dex: abilityMod(scores.dex),
    con: abilityMod(scores.con),
    int: abilityMod(scores.int),
    wis: abilityMod(scores.wis),
    cha: abilityMod(scores.cha),
  });

  // Proficiency ranks: the export's table, bumped by the class progression
  // the prototype applies when displaying above the export's level.
  const ranks = { ...base.proficiencies };
  if (base.identity.class === 'Wizard') {
    wizardProgression(ranks, level, exportLevel);
  }
  /** @param {number} rank */
  const pb = (rank) => profBonus(rank, level);

  // ---- HP max: ancestry + bonus, plus (class + CON + perLevel) × level
  // (prototype hpMax; CON counts at level 1 — see the header note: the
  // transform's stored anchor computes the same number now).
  const hpInputs = base.hp;
  const hpMax =
    hpInputs.ancestryhp +
    hpInputs.bonushp +
    (hpInputs.classhp + mods.con + hpInputs.bonushp_per_level) * level;

  // ---- AC from the export's inputs (spec §5); the proficiency bonus
  // re-derives from the worn armor's category so a level-adjust visibly
  // moves AC (spec §4, review finding 8 — the export's frozen acProfBonus
  // pinned the export level and froze AC forever). At the export's level
  // this equals acTotal exactly; above it, it tracks the class table.
  const wornArmor = Array.isArray(base.armor)
    ? base.armor.find((/** @type {any} */ piece) => piece && piece.worn)
    : null;
  const armorProf = wornArmor?.prof || 'unarmored';
  const acTotal = base.ac ?? {};
  const ac =
    10 +
    (acTotal.acAbilityBonus ?? 0) +
    pb(ranks[armorProf] || 0) +
    (acTotal.acItemBonus ?? 0) +
    (acTotal.shieldBonus ?? 0);

  // ---- Saves / Perception / Speed / Class DC.
  const saves = {
    fort: mods.con + pb(ranks.fortitude || 0),
    ref: mods.dex + pb(ranks.reflex || 0),
    will: mods.wis + pb(ranks.will || 0),
  };
  const perception = mods.wis + pb(ranks.perception || 0);
  // Speed is data, not a fallback (review finding 4): the transform carries
  // the export's speed/speedBonus; 25 is only the absent-section default.
  const speedValue = (base.speed?.base ?? 25) + (base.speed?.bonus ?? 0);
/** @type {number | null} */
  const classDc =
    base.identity.keyability && ranks.classDC
      ? 10 + mods[base.identity.keyability] + pb(ranks.classDC)
      : null;

  // ---- Skills + lores.
  const skills = SKILL_KEYS.map(([key, attr]) => {
    const rank = ranks[key] || 0;
    return {
      name: key,
      ability: attr,
      rank,
      total: mods[attr] + pb(rank),
      applied: [],
      suppressed: [],
    };
  });
  const lores = (base.lores ?? []).map((lore) => {
    const rank = lore.rank;
    return {
      name: `lore:${lore.name}`,
      label: `${lore.name} Lore`,
      ability: 'int',
      rank,
      total: mods.int + pb(rank),
      applied: [],
      suppressed: [],
    };
  });

  // ---- Casters: spell attack / DC per caster block (contract: per instance).
  const casters = (base.spellcasters ?? []).map((caster) => {
    const traditionKey = `casting${capitalize(caster.magic_tradition ?? '')}`;
    const rank = caster.innate
      ? caster.proficiency || 0
      : Math.max(caster.proficiency || 0, ranks[traditionKey] || 0);
    const ability = caster.ability ?? 'int';
    const attack = mods[ability] + pb(rank);
    return {
      caster_key: caster.caster_key,
      ability,
      spell_attack: stat(mods[ability] + pb(rank), attack),
      spell_dc: stat(10 + attack, 10 + attack),
      // render inputs (base passthrough)
      tradition: caster.magic_tradition,
      spellcasting_type: caster.spellcasting_type,
      innate: caster.innate,
    };
  });

  // ---- Strikes: the weapons table + unarmed Fist. Attack math per the
  // prototype (finesse → best of Str/Dex); MAP is −5/−10, agile −4/−8.
  // Traits come from the corpus through the bootstrap's `item_traits` map;
  // the unarmed Fist is not a corpus item and keeps its fixed trait row.
  // The map is keyed by the sheet's OWN spelling — `item_trait_map` keys
  // each requested name as the sheet writes it. Same convention as bulk.js.
  const damageTypeNames = /** @type {Record<string, string>} */ ({
    B: 'bludgeoning',
    P: 'piercing',
    S: 'slashing',
  });
  const itemTraits = character.item_traits ?? {};
  const unarmedTraits = ['Agile', 'Finesse', 'Nonlethal', 'Unarmed'];
  /** @param {string} name @param {Record<string, *>} weapon */
  const strikeRow = (name, weapon) => {
    const traits = itemTraits[String(name ?? '')] ?? [];
    const finesse = traits.includes('Finesse');
    const agile = traits.includes('Agile');
    const rank = ranks[weapon.prof] || 0;
    const attackBase = finesse
      ? Math.max(mods.str, mods.dex)
      : mods.str;
    const attack = attackBase + pb(rank) + (weapon.pot || 0);
    const damageBonus = mods.str + masteryDamage(rank, level);
    return {
      key: name.toLowerCase(),
      label: weapon.display || weapon.name || name,
      attack: stat(attack, attack),
      damage_flat: stat(damageBonus, damageBonus),
      // render inputs (base passthrough)
      map: agile ? 4 : 5,
      damage_expr: `${weapon.die}${damageBonus ? signedBonus(damageBonus) : ''}`,
      damage_type: weapon.damageType,
      damage_type_name: damageTypeNames[weapon.damageType] ?? weapon.damageType,
      traits,
    };
  };
  const strikes = (base.weapons ?? []).map((weapon) =>
    strikeRow(weapon.name, weapon),
  );
  const unarmedRank = ranks.unarmed || 0;
  const unarmedAttack = Math.max(mods.str, mods.dex) + pb(unarmedRank);
  const unarmedBonus = mods.str + masteryDamage(unarmedRank, level);
  strikes.push({
    key: 'fist',
    label: 'Fist',
    attack: stat(unarmedAttack, unarmedAttack),
    damage_flat: stat(unarmedBonus, unarmedBonus),
    map: 4,
    damage_expr: `d4${signedBonus(unarmedBonus)}`,
    damage_type: 'B',
    damage_type_name: 'bludgeoning',
    traits: unarmedTraits,
  });

  return {
    schema: 'hireling.engine.output.v1',
    character_id: character.id,
    level,
    derived: {
      ac: stat(ac, ac),
      fort: stat(saves.fort, saves.fort),
      ref: stat(saves.ref, saves.ref),
      will: stat(saves.will, saves.will),
      perception: stat(perception, perception),
      speed: stat(speedValue, speedValue),
      class_dc: {
        base: classDc,
        total: classDc,
        applied: [],
        suppressed: [],
      },
      strikes,
      casters,
      skills,
      lores,
    },
    hp_max: stat(hpMax, hpMax),
    // Render inputs the sheet needs alongside the numbers (base passthrough;
    // not engine output — deleted with this file at the E8 swap).
    cantrip_rank: Math.ceil(level / 2),
    focus_max: base.focus_points ?? 0,
    hero_max: 3,
    attributes: mods,
    ability_scores: {
      str: scores.str,
      dex: scores.dex,
      con: scores.con,
      int: scores.int,
      wis: scores.wis,
      cha: scores.cha,
    },
    key_ability: base.identity.keyability ?? null,
    // E8 owns effect chips; base-only mode renders the strip hidden (spec
    // §2.8) — `effects: []` is the contract's honest empty.
    effects: [],
  };
}

/** @param {string} text */
function capitalize(text) {
  return text ? text[0].toUpperCase() + text.slice(1) : text;
}

/** @param {number} n */
function signedBonus(n) {
  return n < 0 ? `\u{2212}${Math.abs(n)}` : `+${n}`;
}
