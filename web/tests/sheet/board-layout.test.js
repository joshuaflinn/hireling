import { describe, it, expect } from 'vitest';
import { layoutPass, defaultLayout, validLayout, fitRight, snapX, relayout, WIDGET_IDS, GAP, MINW } from '../../src/lib/sheet/board-layout.js';

describe("Dave's board layout math", () => {
  it('stacks overlapping boxes, leaves side-by-side boxes at y 0', () => {
    const out = layoutPass(['a', 'b', 'c'], { a: { x: 0, w: 100, h: 50 }, b: { x: 0, w: 100, h: 30 }, c: { x: 200, w: 100, h: 10 } });
    expect(out.b.y).toBe(50 + GAP);
    expect(out.c.y).toBe(0);
  });

  it('default wide layout puts Stats | Magic+Inv+Feats | Ref+Notes', () => {
    const L = defaultLayout(1600);
    expect(L.pStats.x).toBe(0);
    expect(L.pSpells.x).toBe(L.pInv.x);
    expect(L.pRef.x).toBe(L.pNotes.x);
    expect(L.pRef.x).toBeGreaterThan(L.pSpells.x);
    expect(validLayout(L)).toBe(true);
  });

  it('narrow layout is two columns', () => {
    const L = defaultLayout(900);
    expect(new Set(WIDGET_IDS.map((id) => L[id].x)).size).toBe(2);
  });

  it('fitRight clamps to the board, never below MINW', () => {
    // Too little room left: keep the width (≥ MINW) and slide left (Dave's rule).
    expect(fitRight({ x: 900, w: 400 }, 1000)).toEqual({ x: 600, w: 400 });
    expect(fitRight({ x: 900, w: 150 }, 1000)).toEqual({ x: 1000 - MINW, w: MINW });
    expect(fitRight({ x: 100, w: 400 }, 1000)).toEqual({ x: 100, w: 400 });
  });

  it('snapX prefers a neighbour edge within 15px, else the 10px grid', () => {
    expect(snapX(338, [{ x: 0, w: 330 }])).toBe(335);
    expect(snapX(503, [{ x: 0, w: 330 }])).toBe(500);
  });

  it('relayout re-packs y from heights', () => {
    const L = defaultLayout(1600);
    relayout(L, () => 100, 1600);
    expect(L.pInv.y).toBe(100 + GAP);
    expect(L.pFeats.y).toBe(2 * (100 + GAP));
  });

  it('rejects a layout missing a widget', () => {
    const L = defaultLayout(1600);
    delete L.pNotes;
    expect(validLayout(L)).toBe(false);
  });
});
