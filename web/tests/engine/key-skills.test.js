import { test } from 'vitest';
import assert from 'node:assert/strict';

import { keySkills } from '../../src/lib/engine/key-skills.js';
import { partitionSkills } from '../../src/lib/engine/partition.js';

// keySkills (E14 design D4, spec FR-2): the tie-break comparator and the
// display-name resolution, pinned fixture by fixture. Every data-sourced
// assertion carries both sides of its boundary with different results — a
// single-fixture assertion also passes against the wrong rule.

/** @param {string} name @param {number} total @param {number} rank */
const row = (name, total, rank) => ({ name, total, rank });

test('equal totals: the higher rank wins the earlier slot', () => {
  // Both sides: rank-8 before rank-2 at the same total...
  assert.deepEqual(
    keySkills([row('arcana', 9, 2), row('deception', 9, 8)]).map((s) => s.name),
    ['deception', 'arcana'],
  );
  // ...and the mirrored fixture flips the order — the rule reads rank,
  // not input order.
  assert.deepEqual(
    keySkills([row('deception', 9, 8), row('arcana', 9, 2)]).map((s) => s.name),
    ['deception', 'arcana'],
  );
});

test('equal totals and ranks: name ascending — core keys and lore keys alike', () => {
  // athletics before thievery (the spec's own example pair)...
  assert.deepEqual(
    keySkills([row('thievery', 9, 2), row('athletics', 9, 2)]).map((s) => s.name),
    ['athletics', 'thievery'],
  );
  // ...and lore keys compare with the prefix included: underworld before
  // warfare.
  assert.deepEqual(
    keySkills([
      row('lore:warfare', 9, 2),
      row('lore:underworld', 9, 2),
    ]).map((s) => s.name),
    ['lore:underworld', 'lore:warfare'],
  );
});

test('three-way total tie: the exact predicted order', () => {
  // thievery's rank 4 outranks the rank-2 pair; arcana then beats
  // athletics on the name tie-break. One fixture, exact order asserted.
  assert.deepEqual(
    keySkills([
      row('athletics', 9, 2),
      row('thievery', 9, 4),
      row('arcana', 9, 2),
    ]).map((s) => s.name),
    ['thievery', 'arcana', 'athletics'],
  );
});

test('top-3 boundary: selection re-runs when a total changes (SC-4)', () => {
  const four = [
    row('deception', 10, 4),
    row('arcana', 9, 2),
    row('society', 8, 2),
    row('occultism', 7, 2), // one point short — stays out
  ];
  assert.deepEqual(
    keySkills(four).map((s) => s.name),
    ['deception', 'arcana', 'society'],
    'the fourth row stays out while its total is one point lower',
  );
  // Raise it that one point: the 8/2 tie with society resolves on the name
  // tie-break (occultism < society), so it enters and the old third drops —
  // the rule is re-evaluated over the delivered totals, not cached.
  const raised = four.map((s) => (s.name === 'occultism' ? row('occultism', 8, 2) : s));
  assert.deepEqual(
    keySkills(raised).map((s) => s.name),
    ['deception', 'arcana', 'occultism'],
    'society loses its slot to the raised row',
  );
});

test('an untrained row (rank 0) wins its slot when its total is genuinely top-3', () => {
  // No proficiency pre-filter: rank what is. The rank-0 row outranks the
  // trained rows below it on total alone.
  const skills = [
    row('deception', 10, 4),
    row('crafting', 9, 0),
    row('arcana', 8, 2),
    row('occultism', 8, 2),
  ];
  const picked = keySkills(skills);
  assert.equal(picked[1].name, 'crafting');
  assert.equal(picked[1].rank, 0);
  assert.deepEqual(
    picked.map((s) => s.name),
    ['deception', 'crafting', 'arcana'],
  );
});

test('fewer than three rows: all returned, never padded', () => {
  const two = keySkills([row('arcana', 9, 2), row('stealth', 6, 2)]);
  assert.equal(two.length, 2);
  assert.deepEqual(
    two.map((s) => s.name),
    ['arcana', 'stealth'],
  );
  const one = keySkills([row('arcana', 9, 2)]);
  assert.equal(one.length, 1);
});

test('lore-label parity: keySkills resolves labels exactly as partitionSkills does (M3)', () => {
  // The mirrored fallback is drift-prone by design; the parity test is the
  // guard. A label-carrying lore row and a label-less one, run through both
  // resolvers, must come out with the identical display string.
  const labelled = { name: 'lore:underworld', total: 9, rank: 2, label: 'Underworld' };
  const bare = { name: 'lore:Mror Holds History', total: 11, rank: 3 };
  for (const loreRow of [labelled, bare]) {
    const throughPartition = partitionSkills([loreRow]).lores[0].label;
    const throughKeySkills = keySkills([loreRow])[0].label;
    assert.equal(
      throughKeySkills,
      throughPartition,
      `label parity for ${loreRow.name}`,
    );
    assert.match(/** @type {string} */ (throughKeySkills), / Lore$/, 'the row form');
  }
  // And the core-side convention: the key capitalizes.
  assert.equal(keySkills([row('thievery', 6, 2)])[0].label, 'Thievery');
});

test('input not an array: empty result, no throw', () => {
  assert.deepEqual(keySkills(null), []);
  assert.deepEqual(keySkills(undefined), []);
  assert.deepEqual(keySkills(/** @type {any} */ ({ name: 'arcana' })), []);
});

test('the input is never mutated; provenance rides along', () => {
  const skills = [
    { name: 'arcana', total: 9, rank: 2, applied: [{ effect_id: 1 }], suppressed: [] },
    { name: 'deception', total: 10, rank: 4, applied: [], suppressed: [] },
  ];
  const picked = keySkills(skills);
  assert.deepEqual(skills.map((s) => s.name), ['arcana', 'deception'], 'input order kept');
  assert.notEqual(picked[0], skills[1], 'picked rows are copies, not the input objects');
  assert.equal(
    picked.find((/** @type {any} */ s) => s.name === 'arcana').applied.length,
    1,
    'applied list available to the hover',
  );
});
