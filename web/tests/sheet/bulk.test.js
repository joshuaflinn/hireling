import { test } from 'vitest';
import assert from 'node:assert/strict';

import { inventoryView, tenthsText } from '../../src/lib/sheet/bulk.js';

const baseSheet = {
  equipment: [
    { name: 'Rope', qty: 1, container: 'Backpack', invested: false },
    { name: 'Chalk', qty: 10, container: 'Backpack', invested: false },
    { name: 'Backpack', qty: 1, container: null, invested: true },
    { name: 'Silver Chunk', qty: 2, container: "Giant body's sack", invested: false },
    { name: 'Staff', qty: 1, container: null, invested: false },
  ],
  containers: [
    { name: 'Backpack', extradimensional: false, backpack: true },
    { name: "Giant body's sack", extradimensional: true, backpack: false },
  ],
};

const bulk = { Rope: 10, Chalk: null, Backpack: 1, 'Silver Chunk': 1, Staff: 10 };
const traits = { Rope: ['trade'], Staff: ['magical', 'staff'] };
const qtyOf = (name) => ({ Rope: 1, Chalk: 10, Backpack: 1, 'Silver Chunk': 2, Staff: 1 })[name] ?? 0;

// Top level: Backpack item 1×1 + Staff 10×1 = 11. Backpack group:
// Rope 10×1 + Chalk gap 0×10 = 10. Sack excluded. Total 21.

test('rollup: member bulk × qty in tenths, L and numbers and gaps handled', () => {
  const { groups } = inventoryView(baseSheet, bulk, traits, qtyOf);
  const backpack = groups.find((group) => group.name === 'Backpack');
  assert.equal(backpack.bulkTenths, 10, 'Rope 10 + Chalk gap 0×10');
  const top = groups.find((group) => group.name === null);
  assert.equal(top.bulkTenths, 11, 'the container item itself counts where it sits');
});

test('extradimensional containers are excluded from their rollup and the total', () => {
  const { groups, totalTenths, totalText } = inventoryView(baseSheet, bulk, traits, qtyOf);
  const sack = groups.find((group) => group.name === "Giant body's sack");
  assert.equal(sack.extradimensional, true);
  assert.equal(sack.bulkTenths, null, 'contents never counted');
  assert.equal(sack.bulkText, 'excluded');
  // Top level: Backpack item 0.1×1 + Staff 1×10 = 11; Backpack group 10 → total 21.
  assert.equal(totalTenths, 21, 'sack contents excluded from the character total');
  assert.equal(totalText, '2 Bulk + 1 L');
});

test('tenths text: L, numbers, gaps, and negligible', () => {
  assert.equal(tenthsText(32), '3 Bulk + 2 L');
  assert.equal(tenthsText(10), '1 Bulk');
  assert.equal(tenthsText(2), '2 L');
  assert.equal(tenthsText(0), 'negligible');
  assert.equal(tenthsText(null), '—');
});

// -- carried and worn: the weapon in hand and the armor on their back
// count toward the character total (gh#46) --

const wornSheet = {
  equipment: [{ name: 'Torch', qty: 1, container: null, invested: false }],
  weapons: [{ name: 'Longsword', qty: 1 }],
  armor: [
    { name: "Explorer's Clothing", worn: true, qty: 1 },
    { name: 'Shield (stowed)', worn: false, qty: 1 },
  ],
  containers: [],
};
const wornBulk = {
  Torch: 1,
  Longsword: 10,
  "Explorer's Clothing": 1,
  'Shield (stowed)': 5,
};
const qtyOne = () => 1;

test('the character total folds wielded weapons and worn armor in', () => {
  const { totalTenths, totalText } = inventoryView(wornSheet, wornBulk, {}, qtyOne);
  assert.equal(
    totalTenths,
    12,
    'Torch 1 + Longsword 10 + worn clothing 1; the stowed shield row is outside the fold',
  );
  assert.equal(totalText, '1 Bulk + 2 L');
});

test('non-worn armor is outside the gh#46 fold; corpus gaps contribute zero', () => {
  const gaps = {
    Torch: 1,
    Longsword: null,
    "Explorer's Clothing": null,
    'Shield (stowed)': 5,
  };
  const { totalTenths } = inventoryView(wornSheet, gaps, {}, qtyOne);
  assert.equal(totalTenths, 1, 'gaps degrade to 0 — never blocking, never invented');
});

test('weapon qty multiplies in the total', () => {
  const twoSwords = { ...wornSheet, weapons: [{ name: 'Longsword', qty: 2 }] };
  const { totalTenths } = inventoryView(twoSwords, wornBulk, {}, qtyOne);
  assert.equal(totalTenths, 22, 'Torch 1 + 2 × 10 + worn clothing 1');
});
