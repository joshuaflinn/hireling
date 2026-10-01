import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import InventoryPanel from '../../src/lib/sheet/components/InventoryPanel.svelte';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));

const itemBulk = Object.fromEntries([
  ['Backpack', 1],
  ['Bedroll', null],
  ['Chalk', null],
  ['Flint and Steel', null],
  ['Rope', 1],
  ['Rations', 2],
  ['Torch', 1],
  ['Waterskin', null],
  ['Soap', null],
  ['Writing Set', null],
  ['Sack', 1],
  ['Lock (Simple)', null],
  ['Manacles (Simple)', null],
  ['Cold Iron Ingot', null],
  ['Silver Chunk', 1],
  ['Oil of Weightlessness', null],
]);
const itemTraits = { Rope: ['trade'], Torch: ['light'], Staff: ['magical', 'staff'] };
const qtyMap = {
  Backpack: { qty: 1, pending: false },
  Bedroll: { qty: 1, pending: false },
  Chalk: { qty: 8, pending: false },
  'Flint and Steel': { qty: 1, pending: false },
  Rope: { qty: 1, pending: false },
  Rations: { qty: 2, pending: false },
  Torch: { qty: 5, pending: false },
  Waterskin: { qty: 1, pending: false },
  Soap: { qty: 1, pending: false },
  'Writing Set': { qty: 1, pending: false },
  Sack: { qty: 5, pending: false },
  'Lock (Simple)': { qty: 1, pending: false },
  'Manacles (Simple)': { qty: 1, pending: false },
  'Cold Iron Ingot': { qty: 7, pending: false },
  'Silver Chunk': { qty: 2, pending: false },
  'Oil of Weightlessness': { qty: 1, pending: false },
};

test('the inventory renders groups, rollups, chips, coins, and the total', () => {
  const { body } = render(InventoryPanel, {
    props: {
      baseSheet: fixture,
      itemBulk,
      itemTraits,
      qtyMap,
      money: { value: { pp: 0, gp: 24, sp: 2, cp: 4 }, pending: false },
      editable: true,
      offline: false,
      onqty: () => {},
      onmoney: () => {},
    },
  });
  assert.match(body, /Backpack/, 'container group header');
  assert.match(body, /extradimensional/, "the sack's exclusion is labeled");
  assert.match(body, /Carried/, 'the top-level group');
  assert.match(body, /Total carried: <b>/, 'the rollup total');
  assert.match(body, /trade/, 'trait chips render from the corpus map');
  assert.match(body, /GP/, 'coin bar');
  assert.match(body, /Rope/);
});

test('view-only inventory: no inputs, quantities as text', () => {
  const { body } = render(InventoryPanel, {
    props: {
      baseSheet: fixture,
      itemBulk,
      itemTraits,
      qtyMap,
      money: { value: { pp: 0, gp: 24, sp: 2, cp: 4 }, pending: false },
      editable: false,
    },
  });
  const inputs = body.match(/<input/g) ?? [];
  assert.equal(inputs.length, 0, 'view-only renders no controls');
  assert.match(body, /×1/, 'quantities render as text');
});
