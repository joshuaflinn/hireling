import test from 'node:test';
import assert from 'node:assert/strict';

import { partitionSkills } from '../../src/lib/engine/partition.js';

// The partition is E6's half of E8's lore fold (MOR-50 Q2): the engine
// ships lores inside `derived.skills` as `lore:<name>` entries; the sheet
// renders them as a separate labelled list. Pure transform, pinned here so
// the adapter swap cannot change what the pane shows.

test('partition: lores split out of the skills array, order kept', () => {
  const skills = [
    { name: 'thievery', ability: 'dex', rank: 2, total: 6 },
    { name: 'lore:Underworld', ability: 'int', rank: 2, total: 9 },
    { name: 'arcana', ability: 'int', rank: 2, total: 9 },
    { name: 'lore:Mror Holds History', ability: 'int', rank: 3, total: 11 },
  ];
  const { core, lores } = partitionSkills(skills);
  assert.deepEqual(
    core.map((skill) => skill.name),
    ['thievery', 'arcana'],
  );
  assert.deepEqual(
    lores.map((lore) => lore.name),
    ['lore:Underworld', 'lore:Mror Holds History'],
  );
});

test('partition: labels derive from the key, never trusted from the wire', () => {
  const { lores } = partitionSkills([
    { name: 'lore:Underworld', rank: 2, total: 9, label: 'whatever the wire said' },
  ]);
  assert.equal(lores[0].label, 'Underworld Lore');
});

test('partition: input is not mutated; provenance and identity ride along', () => {
  const lore = {
    name: 'lore:Sailing',
    rank: 1,
    total: 5,
    applied: [{ source: 'inspired' }],
    suppressed: [],
  };
  const athletics = { name: 'athletics', rank: 0, total: -1 };
  const input = [lore, athletics];
  const { core, lores } = partitionSkills(input);

  // Lore rows are copies with the label added — the original is untouched.
  assert.deepEqual(input, [
    { name: 'lore:Sailing', rank: 1, total: 5, applied: [{ source: 'inspired' }], suppressed: [] },
    { name: 'athletics', rank: 0, total: -1 },
  ]);
  assert.equal(lores[0].label, 'Sailing Lore');
  assert.equal(lores[0].applied.length, 1);
  assert.deepEqual(lores[0].suppressed, []);

  // Core rows pass through by reference.
  assert.equal(core[0], athletics);
});

test('partition: a skills array without lores yields an empty lore list', () => {
  const { core, lores } = partitionSkills([{ name: 'stealth', rank: 2, total: 6 }]);
  assert.equal(core.length, 1);
  assert.deepEqual(lores, []);
});

test('partition: nullish input degrades to two empty lists', () => {
  assert.deepEqual(partitionSkills(null), { core: [], lores: [] });
  assert.deepEqual(partitionSkills(undefined), { core: [], lores: [] });
});
