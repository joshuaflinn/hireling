import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { derive } from '../../src/lib/engine/index.js';
import * as format from '../../src/lib/engine/format.js';

// The fixture is E5's transform output for the frozen reference export —
// generated and drift-pinned by the Rust gate
// (`the_web_base_sheet_fixture_matches_the_transform`). The adapter is
// tested against the same fixture E8 lists as its own reference, so the
// swap review can diff both outputs.

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));

const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));
const character = { id: 7, base_sheet: fixture };

function deriveAt(levelAdjust = 0) {
  return derive(character, { level_adjust: levelAdjust, effects: [] });
}

// ---- format.js -------------------------------------------------------------

test('format: signed numbers use the true minus', () => {
  assert.equal(format.signed(3), '+3');
  assert.equal(format.signed(-1), '−1');
  assert.equal(format.signed(0), '+0');
});

test('format: bulk text from tenths', () => {
  assert.equal(format.bulkText(32), '3 Bulk + 2 L');
  assert.equal(format.bulkText(10), '1 Bulk');
  assert.equal(format.bulkText(2), '2 L');
  assert.equal(format.bulkText(0), 'negligible');
  assert.equal(format.bulkTextOrDash(null), '—');
  assert.equal(format.bulkTextOrDash(1), '1 L');
});

test('format: proficiency bonus and rank letters', () => {
  assert.equal(format.profBonus(2, 3), 5);
  assert.equal(format.profBonus(0, 3), 0);
  assert.equal(format.rankLetter(4), 'E');
  assert.equal(format.rankName(2), 'Trained');
  assert.equal(format.ordinal(3), '3rd');
});

// ---- the fixture's base derivation (level 3) -------------------------------

test('AC comes from the export breakdown parts', () => {
  const sheet = deriveAt();
  assert.deepEqual(
    {
      base: sheet.derived.ac.base,
      applied: sheet.derived.ac.applied,
      suppressed: sheet.derived.ac.suppressed,
    },
    { base: 16, applied: [], suppressed: [] },
    '10 + dex 1 + prof 5 + item 0 (acTotal parts)',
  );
});

test('each save: ability + proficiency(rank, level)', () => {
  const { derived } = deriveAt();
  assert.equal(derived.fort.total, 7, 'CON +2 + trained 5');
  assert.equal(derived.ref.total, 6, 'DEX +1 + trained 5');
  assert.equal(derived.will.total, 7, 'WIS +0 + expert 7');
});

test('perception and speed', () => {
  const { derived } = deriveAt();
  assert.equal(derived.perception.total, 5);
  // The fixture's speed is DATA (review finding 4): the transform carries
  // speed 25 / bonus 0 and the adapter reads it. 25 here is the fixture's
  // value, not the fallback — the next test proves the data path.
  assert.equal(derived.speed.total, 25);
});

test('speed reads the transform output, not a hardcoded fallback', () => {
  // A dwarf-like sheet: base 20 + bonus 5. Before the fix the adapter read
  // `base.raw?.attributes` (always undefined) and printed 25 for everyone.
  const dwarfSheet = { ...structuredClone(fixture), speed: { base: 20, bonus: 5 } };
  const dwarf = derive({ id: 7, base_sheet: dwarfSheet }, { level_adjust: 0, effects: [] });
  assert.equal(dwarf.derived.speed.total, 25, '20 walks + 5 bonus');

  const slowSheet = { ...structuredClone(fixture), speed: { base: 20, bonus: 0 } };
  const slow = derive({ id: 7, base_sheet: slowSheet }, { level_adjust: 0, effects: [] });
  assert.equal(slow.derived.speed.total, 20, 'a dwarf walks at 20');

  // Absent section: the 25 default both sides agree on (contract §3.3).
  const noSpeedSheet = { ...structuredClone(fixture), speed: null };
  const fallback = derive({ id: 7, base_sheet: noSpeedSheet }, { level_adjust: 0, effects: [] });
  assert.equal(fallback.derived.speed.total, 25, 'absent speed defaults to 25');
});

test('strike traits resolve through item_traits: an agile finesse weapon on a Dex character', () => {
  // Review finding 7: the hardcoded two-entry table meant a rapier
  // attacked off STR and MAP read −5/−10. Traits come from the corpus map
  // the bootstrap payload carries.
  const rapierSheet = structuredClone(fixture);
  rapierSheet.weapons = [
    { name: 'Rapier', prof: 'martial', die: 'd6', pot: 0, display: 'Rapier', damageType: 'P' },
  ];
  const sheet = derive(
    {
      id: 7,
      base_sheet: rapierSheet,
      item_traits: { rapier: ['Deadly d8', 'Finesse', 'Agile'] },
    },
    { level_adjust: 0, effects: [] },
  );
  const rapier = sheet.derived.strikes.find((strike) => strike.key === 'rapier');
  assert.deepEqual(rapier.traits, ['Deadly d8', 'Finesse', 'Agile'], 'corpus traits ride');
  assert.equal(rapier.attack.total, 1, 'finesse: DEX +1 outranks STR −1, plus prof(martial 0)');
  assert.equal(rapier.attack.base, 1);
  assert.equal(rapier.map, 4, 'agile: −4/−8');
  // Unmatched names resolve empty — the honest gap, not the old guess.
  const plainSheet = structuredClone(fixture);
  const plain = derive({ id: 7, base_sheet: plainSheet }, { level_adjust: 0, effects: [] });
  const staff = plain.derived.strikes.find((strike) => strike.key === 'staff');
  assert.deepEqual(staff.traits, [], 'no item_traits in the payload → no invented traits');
  assert.equal(staff.map, 5, 'not agile: −5/−10');
});

test('class DC: 10 + key ability + proficiency', () => {
  const { derived } = deriveAt();
  assert.equal(derived.class_dc.total, 19, '10 + INT +4 + trained 5');
});

test('every core skill and lore carries its modifier', () => {
  const { derived } = deriveAt();
  const byName = new Map(derived.skills.map((skill) => [skill.name, skill]));
  const expected = {
    acrobatics: 1,
    arcana: 9,
    athletics: -1,
    crafting: 4,
    deception: 10,
    diplomacy: 8,
    intimidation: 3,
    medicine: 0,
    nature: 0,
    occultism: 9,
    performance: 3,
    religion: 0,
    society: 9,
    stealth: 6,
    survival: 0,
    thievery: 6,
  };
  for (const [name, total] of Object.entries(expected)) {
    assert.equal(byName.get(name)?.total, total, `${name}`);
  }
  const lores = new Map(derived.lores.map((lore) => [lore.name, lore]));
  assert.equal(lores.get('lore:Underworld').total, 9, 'trained');
  assert.equal(lores.get('lore:Mror Holds History').total, 11, 'expert');
});

test('strike rows carry attack, MAP, damage, and traits — plus unarmed Fist', () => {
  const { derived } = deriveAt();
  const staff = derived.strikes.find((strike) => strike.key === 'staff');
  assert.equal(staff.attack.total, 4, 'the export itself says attack +4');
  assert.equal(staff.map, 5, 'not agile: −5/−10');
  assert.equal(staff.damage_expr, 'd4−1');
  const fist = derived.strikes.find((strike) => strike.key === 'fist');
  assert.equal(fist.attack.total, 6, 'finesse unarmed: best of STR/DEX + prof');
  assert.equal(fist.map, 4, 'agile: −4/−8');
  assert.deepEqual(fist.traits, ['Agile', 'Finesse', 'Nonlethal', 'Unarmed']);
  assert.ok(
    derived.strikes.every(
      (strike) =>
        strike.attack.applied.length === 0 && strike.attack.suppressed.length === 0,
    ),
    'base-only mode: provenance is base parts only',
  );
});

test('hp_max follows the prototype formula at levels 1, 3, and 20', () => {
  assert.equal(deriveAt(0).hp_max.total, 32, '8 + (6 + CON 2) × 3');
  assert.equal(deriveAt(-2).hp_max.total, 16, 'level 1: 8 + 8 × 1');
  assert.equal(deriveAt(17).hp_max.total, 168, 'level 20: 8 + 8 × 20');
});

test('cantrip and focus heightened rank: ceil(level / 2)', () => {
  assert.equal(deriveAt(0).cantrip_rank, 2, 'level 3');
  assert.equal(deriveAt(-2).cantrip_rank, 1, 'level 1');
  assert.equal(deriveAt(1).cantrip_rank, 2, 'level 4 rounds up from 2');
  assert.equal(deriveAt(2).cantrip_rank, 3, 'level 5');
});

test('casters: per-instance spell attack and DC', () => {
  const { derived } = deriveAt();
  const wizard = derived.casters.find((caster) => caster.caster_key === 'Wizard');
  assert.equal(wizard.spell_attack.total, 9, 'INT +4 + trained 5');
  assert.equal(wizard.spell_dc.total, 19);
  const innate = derived.casters.find(
    (caster) => caster.caster_key === 'Wellspring Gnome',
  );
  assert.equal(innate.spell_attack.total, 8, 'CHA +3 + trained 5 (innate)');
});

// ---- level_adjust re-derivation (spec §4) ----------------------------------

test('level_adjust re-derives proficiency bonus, HP, DCs, and AC', () => {
  const at = (adjust) => deriveAt(adjust);
  const plus1 = at(1);
  assert.equal(plus1.level, 4);
  assert.equal(plus1.hp_max.total, 40, 'max HP re-derives');
  assert.equal(plus1.derived.will.total, 8, 'expert ranks ride +1 level');
  assert.equal(plus1.derived.class_dc.total, 20);
  const wizard = plus1.derived.casters.find((c) => c.caster_key === 'Wizard');
  assert.equal(wizard.spell_attack.total, 10, 'spell attack scales');
  // AC is level-derived (spec §4, review finding 8): the unarmored prof
  // bonus rides the level — 10 + dex 1 + prof(2, 4) = 17.
  assert.equal(plus1.derived.ac.total, 17, 'AC re-derives with the level');
  // And it tracks the class table: at level 20 the wizard's unarmored
  // rank pulls forward to legendary (4), so AC = 10 + 1 + 24 = 35.
  assert.equal(at(17).derived.ac.total, 35, 'AC at level 20 via the class table');
  assert.equal(at(9).level, 12, 'clamped into 1..20 range inside the adapter');
});

test('level_adjust pulls the wizard class progression forward (prototype table)', () => {
  const level5 = deriveAt(2); // 3 → 5: reflex becomes expert
  assert.equal(level5.derived.ref.total, 10, 'DEX +1 + expert 8');
  const level7 = deriveAt(4); // 3 → 7: castingArcane expert
  const wizard = level7.derived.casters.find((c) => c.caster_key === 'Wizard');
  assert.equal(wizard.spell_attack.total, 15, 'INT +4 + expert (4+7)');
  // Skills, feats, boosts never re-derive beyond the proficiency line:
  assert.equal(
    level5.derived.skills.find((s) => s.name === 'stealth').total,
    8,
    'skilled ranks also ride the level (DEX +1 + trained 7)',
  );
});

// ---- the E6/E8 seam shape ---------------------------------------------------

test('base-only mode is honest about effects and provenance', () => {
  const sheet = deriveAt();
  assert.deepEqual(sheet.effects, [], 'no chips before E8');
  assert.equal(sheet.schema, 'hireling.engine.output.v1');
  assert.equal(sheet.derived.fort.applied.length, 0);
  assert.equal(sheet.derived.fort.suppressed.length, 0);
});

test('level bounds clamp to 1..20 regardless of the adjust value', () => {
  assert.equal(deriveAt(-100).level, 1);
  assert.equal(deriveAt(100).level, 20);
});
