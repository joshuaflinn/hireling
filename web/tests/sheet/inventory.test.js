import { render, cleanup } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';

import InventoryPanel from '../../src/lib/sheet/components/InventoryPanel.svelte';

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

afterEach(cleanup);

test('the inventory renders groups, rollups, chips, coins, and the total', () => {
  const { container } = render(InventoryPanel, {
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
  assert.match(container.innerHTML, /Backpack/, 'container group header');
  assert.match(container.innerHTML, /extradimensional/, "the sack's exclusion is labeled");
  assert.match(container.innerHTML, /Carried/, 'the top-level group');
  assert.match(container.innerHTML, /Total carried: <b>/, 'the rollup total');
  assert.match(container.innerHTML, /trade/, 'trait chips render from the corpus map');
  assert.match(container.innerHTML, /GP/, 'coin bar');
  assert.match(container.innerHTML, /Rope/);
});

test('view-only inventory: no inputs, quantities as text', () => {
  const { container } = render(InventoryPanel, {
    props: {
      baseSheet: fixture,
      itemBulk,
      itemTraits,
      qtyMap,
      money: { value: { pp: 0, gp: 24, sp: 2, cp: 4 }, pending: false },
      editable: false,
    },
  });
  assert.equal(container.querySelectorAll('input').length, 0, 'view-only renders no controls');
  assert.match(container.innerHTML, /×1/, 'quantities render as text');
});

// ---- MOR-48 review fixes: the production path owns the behaviour ----------

test('a rejected quantity write surfaces inline at that item; a rejected coin write at the coins (finding 5)', () => {
  const { container } = render(InventoryPanel, {
    props: {
      baseSheet: fixture,
      itemBulk,
      itemTraits,
      qtyMap,
      money: { value: { pp: 0, gp: 24, sp: 2, cp: 4 }, pending: false },
      opErrors: [
        {
          key: 'inv:7:Chalk',
          target: { kind: 'inv', character_id: 7, item_name: 'Chalk' },
          op_id: 'op-4',
          outcome: 'rejected',
          reason: 'The party refused that quantity.',
        },
        {
          key: 'vitals:7:money',
          target: { kind: 'vitals', character_id: 7, field: 'money' },
          op_id: 'op-5',
          outcome: 'rejected',
          reason: 'Coins write refused: negative amounts.',
        },
      ],
      editable: true,
      offline: false,
      onqty: () => {},
      onmoney: () => {},
    },
  });
  assert.match(container.innerHTML, /The party refused that quantity\./);
  assert.match(container.innerHTML, /Coins write refused: negative amounts\./);
});
