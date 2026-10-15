import { describe, it, expect } from 'vitest';
import { organize, carriedTenths, derivedTags, patchInv, bulkLimits } from '../../src/lib/sheet/inv-edit.js';

const base = {
  equipment: [
    { name: 'Backpack', qty: 1, container: null },
    { name: 'Rope', qty: 1, container: 'Backpack' },
    { name: 'Rations', qty: 2, container: 'Backpack' },
    { name: "Giant body's sack", qty: 1, container: null },
    { name: 'Cold Iron Ingot', qty: 7, container: "Giant body's sack" },
    { name: 'Torch', qty: 5, container: null },
  ],
  containers: [
    { name: 'Backpack', backpack: true, extradimensional: false },
    { name: "Giant body's sack", backpack: false, extradimensional: true },
  ],
};
const bulk = /** @type {Record<string, number>} */ ({ Backpack: 1, Rope: 1, Rations: 1, "Giant body's sack": 1, 'Cold Iron Ingot': 10, Torch: 1 });
const qty = (/** @type {string} */ n) => base.equipment.find((e) => e.name === n)?.qty ?? 0;
const view = (/** @type {any} */ inv) =>
  organize(base, inv, qty, (n) => bulk[n] ?? null, () => [], () => null);

describe('inventory organize', () => {
  it('keeps export containers and their contents; backpack equipped by default', () => {
    const { rows } = view({});
    expect(rows.find((r) => r.name === 'Rope')?.loc).toBe('Backpack');
    expect(rows.find((r) => r.name === 'Backpack')?.equipped).toBe(true);
  });

  it('applies moves, equipped toggles, removals (restorable)', () => {
    let inv = patchInv({}, 'Torch', { loc: 'Backpack' });
    inv = patchInv(inv, 'Rope', { removed: true });
    const { rows, removed } = view(inv);
    expect(rows.find((r) => r.name === 'Torch')?.loc).toBe('Backpack');
    expect(rows.some((r) => r.name === 'Rope')).toBe(false);
    expect(removed).toEqual(['Rope']);
  });

  it('excludes extradimensional contents and the backpack first 2 Bulk', () => {
    const { rows } = view({});
    // carried: Backpack 1 + Rope 1 + Rations 2 + sack 1 + Torch 5 = 10; sack contents excluded;
    // backpack contents (3) under the 20-tenth relief → −3
    expect(carriedTenths(rows)).toBe(7);
  });

  it('a user-marked container accepts items; a dangling loc falls back to top', () => {
    let inv = patchInv({}, 'Torch', { container: true });
    inv = patchInv(inv, 'Rations', { loc: 'Torch' });
    expect(view(inv).rows.find((r) => r.name === 'Rations')?.loc).toBe('Torch');
    expect(view(patchInv({}, 'Rope', { loc: 'Nowhere' })).rows.find((r) => r.name === 'Rope')?.loc).toBeNull();
  });

  it('derives tags from the corpus record', () => {
    expect(derivedTags({ traits: ['consumable', 'magical'], detail: { type: 'oil' } })).toEqual(['magic']);
    expect(derivedTags({ traits: ['precious'], detail: {} })).toEqual(['trade']);
    expect(derivedTags({ traits: [], detail: { type: 'gem' } })).toEqual(['gems']);
  });

  it('bulk limits follow Strength', () => {
    expect(bulkLimits(-1)).toEqual({ encumbered: 4, max: 9 });
  });
});

describe('container-only export rows', () => {
  it('a container with no equipment row still holds its contents (excluded if extradimensional)', () => {
    const b2 = {
      equipment: [{ name: 'Cold Iron Ingot', qty: 7, container: "Giant body's sack" }],
      containers: [{ name: "Giant body's sack", backpack: false, extradimensional: true }],
    };
    const { rows } = organize(b2, {}, () => 7, (n) => (n === 'Cold Iron Ingot' ? 10 : 0), () => [], () => null);
    expect(rows.find((r) => r.name === 'Cold Iron Ingot')?.loc).toBe("Giant body's sack");
    expect(carriedTenths(rows)).toBe(0);
  });
});
