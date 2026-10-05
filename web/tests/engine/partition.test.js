import { test } from 'vitest';
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

test('partition: the label prefers the wire, falling back to the key', () => {
  // Post-swap the lore row carries its display name (contract §3 `label`,
  // verbatim from the export — the canonical key lowercases and
  // underscores, so the display case lives only there). A row without one
  // still gets the MOR-50 fallback derived from the key.
  const { lores } = partitionSkills([
    { name: 'lore:Underworld', rank: 2, total: 9, label: 'Underworld' },
  ]);
  assert.equal(lores[0].label, 'Underworld Lore');
  const derived = partitionSkills([
    { name: 'lore:Underworld', rank: 2, total: 9 },
  ]).lores;
  assert.equal(derived[0].label, 'Underworld Lore');
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

// ---- post-swap: the wire carries the display name (contract §3 `label`) --

test('partition: the wire label wins — the canonical key lost the display case', () => {
  const { lores } = partitionSkills([
    { name: 'lore:mror_holds_history', rank: 4, total: 11, label: 'Mror Holds History' },
    { name: 'lore:underworld', rank: 2, total: 9, label: 'Underworld' },
  ]);
  assert.deepEqual(
    lores.map((lore) => lore.label),
    ['Mror Holds History Lore', 'Underworld Lore'],
    'the row form, from the display names the wire shipped verbatim',
  );
});

test('partition: a lore row with no wire label still derives one from the key', () => {
  const { lores } = partitionSkills([
    { name: 'lore:underworld', rank: 2, total: 9 },
    { name: 'lore:', rank: 0, total: 0, label: '' },
  ]);
  assert.equal(lores[0].label, 'underworld Lore', 'the MOR-50 fallback');
  assert.equal(lores[1].label, ' Lore', 'an empty label is no label — derive');
});
