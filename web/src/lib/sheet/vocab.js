// The composer's stat vocabulary — DERIVED, never a constant (spec
// specs/010-effect-composer FR-C3). The engine's closed vocabulary lives in
// `engine/src/vocab.rs`; this module does not copy it. The option set is read
// off the character's delivered engine output (specs/008/contracts/
// engine-output.md §3 — the `GET /api/characters/{id}/derived` body, which
// the socket delivers identically): the global singles are the derived map's
// own keys, the strike family exists when strikes exist, the spell family
// when casters do, and each skill instance the sheet carries offers its
// `skill:<name>` stat. Two characters → two pickers; the tests pin that with
// two differing fixtures.
//
// The only constants here are DISPLAY labels (cosmetics, keyed by wire text,
// with a prettify fallback) and the three engine-output §3 family key names —
// contract shape, not stat vocabulary.

/** Display labels for the global stat keys (engine-output §3). Unknown keys
 *  fall through to prettify — the option set is never limited by this table. */
/** @type {Record<string, string>} */
const GLOBAL_LABELS = {
  ac: 'AC',
  fort: 'Fortitude',
  ref: 'Reflex',
  will: 'Will',
  perception: 'Perception',
  speed: 'Speed',
  class_dc: 'Class DC',
};

/** The engine-output §3 container keys that are families, not stats. */
const FAMILY_KEYS = ['strikes', 'casters', 'skills'];

/** @param {string} text */
function prettify(text) {
  const words = text.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** `acrobatics` → `skill:acrobatics` / `Athletics`; `lore:underworld` →
 *  `skill:lore:underworld` / `Lore: Underworld`. */
/** @param {string} instanceName */
export function skillOption(instanceName) {
  const lore = instanceName.startsWith('lore:');
  const bare = lore ? instanceName.slice(5) : instanceName;
  const label = lore ? `Lore: ${prettify(bare)}` : prettify(instanceName);
  return { value: `skill:${instanceName}`, label };
}

/**
 * The stat picker's grouped options for one delivered engine output.
 *
 * @param {Record<string, *> | null} view the EngineOutput contract shape
 * @returns {Array<{group: string, options: Array<{value: string, label: string}>}>}
 */
export function statOptionsFromView(view) {
  const derived = /** @type {Record<string, *> | null} */ (view?.derived ?? null);
  if (!derived) return [];
  /** @type {Array<{group: string, options: Array<{value: string, label: string}>}>} */
  const groups = [];

  const globals = Object.keys(derived).filter((key) => !FAMILY_KEYS.includes(key));
  if (globals.length > 0) {
    groups.push({
      group: 'Checks & defenses',
      options: globals.map((key) => ({ value: key, label: GLOBAL_LABELS[key] ?? prettify(key) })),
    });
  }

  /** @type {Array<{value: string, label: string}>} */
  const strikeOptions = [];
  if (Array.isArray(derived.strikes) && derived.strikes.length > 0) {
    strikeOptions.push({ value: 'attack', label: 'Attack rolls' });
    strikeOptions.push({ value: 'damage', label: 'Damage' });
  }
  if (strikeOptions.length > 0) groups.push({ group: 'Strikes', options: strikeOptions });

  /** @type {Array<{value: string, label: string}>} */
  const spellOptions = [];
  if (Array.isArray(derived.casters) && derived.casters.length > 0) {
    spellOptions.push({ value: 'spell_attack', label: 'Spell attack' });
    spellOptions.push({ value: 'spell_dc', label: 'Spell DC' });
  }
  if (spellOptions.length > 0) groups.push({ group: 'Spells', options: spellOptions });

  const skills = Array.isArray(derived.skills) ? derived.skills : [];
  if (skills.length > 0) {
    groups.push({
      group: 'Skills',
      options: skills.map((skill) => skillOption(/** @type {any} */ (skill).name)),
    });
  }
  return groups;
}

/**
 * The closed modifier-type list — the engine's settled vocabulary
 * (`engine/src/vocab.rs` `MODIFIER_TYPES`, pinned by engine tests). There is
 * no read face that enumerates types, so the composer cites the source; the
 * write path re-validates and its rejection surfaces.
 *
 * @returns {string[]}
 */
export function modifierTypes() {
  return ['circumstance', 'status', 'item', 'untyped'];
}
