import { test } from 'vitest';
import assert from 'node:assert/strict';

import { inventoryView, tenthsText } from '../../src/lib/sheet/bulk.js';
// Second real Pathbuilder export (gh#80 evidence): Taraan Skyseeker, a
// campaign-share export fetched verbatim from a public playthrough repo
// (enmaku/strength-of-thousands, heroes/taraan-skyseeker.json; only
// re-indented). Campaign-share exports omit the "success": true wrapper
// the app adds on manual export — the armor-row shape is the evidence,
// not a parse fixture. It carries TWO armor rows: a worn suit and a held
// shield, which settles where Pathbuilder files shields and what `worn`
// means on them.
import taraan from '../data/pb_export_taraan_skyseeker.json';

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
    { name: "Explorer's Clothing", prof: 'unarmored', worn: true, qty: 1 },
    { name: 'Steel Shield', prof: 'shield', worn: false, qty: 1 },
  ],
  containers: [],
};
const wornBulk = {
  Torch: 1,
  Longsword: 10,
  "Explorer's Clothing": 1,
  'Steel Shield': 10,
};
const qtyOne = () => 1;

test('the character total folds weapons, armor rows, and carried suits in', () => {
  const { totalTenths, totalText } = inventoryView(wornSheet, wornBulk, {}, qtyOne);
  assert.equal(
    totalTenths,
    22,
    'Torch 1 + Longsword 10 + worn clothing 1 + stowed shield 10 (a shield counts listed held or stowed — no carried-suit +1)',
  );
  assert.equal(totalText, '2 Bulk + 2 L');
});

test('unresolved armor names stay gaps; resolved rows count by the carried rule', () => {
  const gaps = {
    Torch: 1,
    Longsword: null,
    "Explorer's Clothing": null,
    'Steel Shield': null,
  };
  const { totalTenths } = inventoryView(wornSheet, gaps, {}, qtyOne);
  assert.equal(totalTenths, 1, 'gaps degrade to 0 — never blocking, never invented');
});

test('weapon qty multiplies in the total', () => {
  const twoSwords = { ...wornSheet, weapons: [{ name: 'Longsword', qty: 2 }] };
  const { totalTenths } = inventoryView(twoSwords, wornBulk, {}, qtyOne);
  assert.equal(totalTenths, 32, 'Torch 1 + 2 × 10 + worn clothing 1 + stowed shield 10');
});

// -- gh#80: a carried suit is listed + 1 Bulk (Player Core pg. 271,
// Armor Statistics → Bulk) --

const suitSheet = (worn) => ({
  equipment: [],
  weapons: [],
  armor: [{ name: 'Full Plate', prof: 'heavy', worn, qty: 1 }],
  containers: [],
});

const suitBulk = { 'Full Plate': 20 }; // listed 2 Bulk

test('the same suit worn vs carried makes two different totals (boundary)', () => {
  const worn = inventoryView(suitSheet(true), suitBulk, {}, qtyOne);
  assert.equal(worn.totalTenths, 20, 'worn suit: the listed value IS the worn value');
  const carried = inventoryView(suitSheet(false), suitBulk, {}, qtyOne);
  assert.equal(carried.totalTenths, 30, 'carried suit: listed + 1 Bulk');
  assert.equal(carried.totalText, '3 Bulk');
});

test('carried-suit floor: listed L carries at 1 Bulk, listed 1 Bulk carries at 2', () => {
  // Both sides of the L/1-Bulk line — one fixture on one side passes
  // against every wrong threshold on that side.
  const light = inventoryView(
    {
      equipment: [],
      weapons: [],
      armor: [{ name: "Explorer's Clothing", prof: 'unarmored', worn: false, qty: 1 }],
      containers: [],
    },
    { "Explorer's Clothing": 1 },
    {},
    qtyOne,
  );
  assert.equal(light.totalTenths, 10, 'listed L → 1 Bulk carried, not 1 Bulk + 1 L');
  const oneBulk = inventoryView(
    { equipment: [], weapons: [], armor: [{ name: 'Leather', prof: 'light', worn: false, qty: 1 }], containers: [] },
    { Leather: 10 },
    {},
    qtyOne,
  );
  assert.equal(oneBulk.totalTenths, 20, 'listed 1 Bulk → 2 Bulk carried');
});

test('carried suits multiply by qty; a carried gap stays a gap', () => {
  const sheet = {
    equipment: [],
    weapons: [],
    armor: [
      { name: 'Full Plate', prof: 'heavy', worn: false, qty: 2 },
      { name: 'Mystery Suit', prof: 'heavy', worn: false, qty: 1 },
    ],
    containers: [],
  };
  const mixedBulk = { 'Full Plate': 20, 'Mystery Suit': null };
  const { totalTenths } = inventoryView(sheet, mixedBulk, {}, qtyOne);
  assert.equal(totalTenths, 60, '2 × (20 + 10) + gap 0 — never invented');
});

test('shield rows count listed held or stowed — the carried-suit +1 is not for shields', () => {
  const shieldBulk = { 'Steel Shield': 10 };
  const sheetFor = (worn) => ({
    equipment: [],
    weapons: [],
    armor: [{ name: 'Steel Shield', prof: 'shield', worn, qty: 1 }],
    containers: [],
  });
  assert.equal(inventoryView(sheetFor(true), shieldBulk, {}, qtyOne).totalTenths, 10, 'held: listed');
  assert.equal(
    inventoryView(sheetFor(false), shieldBulk, {}, qtyOne).totalTenths,
    10,
    'stowed: listed — no +1',
  );
});

// -- the second real export drives the fold: its exported armor rows are
// the production input shape (verbatim build.armor passthrough) --

test('the real export pins the armor row shape: suit and shield on build.armor', () => {
  const armor = taraan.build.armor;
  assert.equal(armor.length, 2, 'more than one armor row — sample-of-one settled');
  const suit = armor.find((row) => row.name === 'Kilted Breastplate');
  assert.equal(suit.prof, 'light');
  assert.equal(suit.worn, true);
  const shield = armor.find((row) => row.name === "Caster's Targe");
  assert.equal(shield.prof, 'shield', 'Pathbuilder files shields under build.armor');
  assert.equal(shield.worn, true, 'worn on a shield row means held-in-hand');
});

test('the real export folds as exported; carrying the suit adds the +1', () => {
  const armor = taraan.build.armor;
  // fixture bulk map (design §5: unit tests use a fixture map, not the
  // live corpus) — these numbers are test data, not corpus/rules facts.
  const taraanBulk = { 'Kilted Breastplate': 20, "Caster's Targe": 10 };
  const sheetFor = (rows) => ({ equipment: [], weapons: [], armor: rows, containers: [] });
  const asExported = inventoryView(sheetFor(armor), taraanBulk, {}, qtyOne);
  assert.equal(asExported.totalTenths, 30, 'worn suit 20 + held shield 10');
  assert.equal(asExported.totalText, '3 Bulk');
  const suitCarried = armor.map((row) => (row.prof === 'shield' ? row : { ...row, worn: false }));
  const carried = inventoryView(sheetFor(suitCarried), taraanBulk, {}, qtyOne);
  assert.equal(carried.totalTenths, 40, 'suit carried: 30 + shield held 10');
});
