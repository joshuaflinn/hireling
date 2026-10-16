// Dave's widget board (specs/015 slice 9) — the pure layout math, ported
// from docs/reference/lorum_ipsum_dashboard.html (layoutPass, defLayout,
// fitRight, snapX, orderForDrag). Widgets are absolutely positioned boxes
// {x, y, w, h}; h null = auto height. y is always recomputed by a column
// packing pass, so a layout is really "x, w, (h), order".

export const WIDGET_IDS = ['pStats', 'pSpells', 'pInv', 'pFeats', 'pRef', 'pNotes'];
export const GAP = 5;
export const MINW = 220;
export const MINH = 120;
export const NARROW_BP = 780;
export const WIDE_BP = 1100;
const SNAPG = 10;
const EDGE_SNAP = 15;

/** @typedef {{x: number, y: number, w: number, h: number | null}} Box */

const overlap = (/** @type {number} */ a0, /** @type {number} */ a1, /** @type {number} */ b0, /** @type {number} */ b1) =>
  a0 < b1 && b0 < a1;

/**
 * Stack boxes in `order`: each lands just below every earlier box it
 * overlaps horizontally.
 * @param {string[]} order
 * @param {Record<string, {x: number, w: number, h: number}>} dims
 * @returns {Record<string, {x: number, y: number, w: number, h: number}>}
 */
export function layoutPass(order, dims) {
  /** @type {Array<{x: number, y: number, w: number, h: number}>} */
  const placed = [];
  /** @type {Record<string, {x: number, y: number, w: number, h: number}>} */
  const out = {};
  for (const id of order) {
    const d = dims[id];
    let y = 0;
    for (const p of placed) if (overlap(d.x, d.x + d.w, p.x, p.x + p.w)) y = Math.max(y, p.y + p.h + GAP);
    out[id] = { x: d.x, y, w: d.w, h: d.h };
    placed.push(out[id]);
  }
  return out;
}

/**
 * Dave's default three-column (wide) / two-column board.
 * @param {number} boardWidth
 * @returns {Record<string, Box>}
 */
export function defaultLayout(boardWidth) {
  const bw = Math.max(NARROW_BP, boardWidth);
  /** @type {Record<string, Box>} */
  const items = {};
  if (bw >= WIDE_BP) {
    const col1w = 330;
    const rem = Math.max(0, bw - col1w - GAP * 2);
    const col2w = Math.round((rem * 1.25) / 2.25);
    const col3w = rem - col2w;
    const x2 = col1w + GAP;
    const x3 = x2 + col2w + GAP;
    /** @type {Array<[string, number, number]>} */
    const spec = [
      ['pStats', 0, col1w],
      ['pSpells', x2, col2w],
      ['pInv', x2, col2w],
      ['pFeats', x2, col2w],
      ['pRef', x3, col3w],
      ['pNotes', x3, col3w],
    ];
    for (const [id, x, w] of spec) items[id] = { x, y: 0, w: Math.max(MINW, w), h: null };
  } else {
    const colw = Math.max(MINW, Math.floor((bw - GAP) / 2));
    /** @type {Array<[string, number]>} */
    const spec = [
      ['pStats', 0],
      ['pRef', 0],
      ['pNotes', 0],
      ['pSpells', colw + GAP],
      ['pInv', colw + GAP],
      ['pFeats', colw + GAP],
    ];
    for (const [id, x] of spec) items[id] = { x, y: 0, w: colw, h: null };
  }
  return items;
}

/** A saved layout is usable if every widget has numeric x/w. */
export function validLayout(/** @type {any} */ items) {
  return (
    !!items &&
    WIDGET_IDS.every((id) => {
      const it = items[id];
      return it && typeof it.x === 'number' && typeof it.w === 'number' && (it.h === null || typeof it.h === 'number');
    })
  );
}

/** Clamp a box so it never passes the board's right edge. */
export function fitRight(/** @type {{x: number, w: number}} */ it, /** @type {number} */ limit) {
  if (it.x + it.w <= limit) return it;
  let w = limit - it.x;
  if (w < MINW) {
    w = Math.max(MINW, Math.min(it.w, limit));
    it.x = Math.max(0, limit - w);
  }
  it.w = w;
  return it;
}

/** Snap x to the grid, or to a neighbour's edge when within EDGE_SNAP. */
export function snapX(/** @type {number} */ x, /** @type {Array<{x: number, w: number}>} */ others) {
  const grid = Math.max(0, Math.round(x / SNAPG) * SNAPG);
  let best = null;
  let bestDist = EDGE_SNAP;
  for (const o of others) {
    for (const cand of [o.x, o.x + o.w + GAP]) {
      const d = Math.abs(x - cand);
      if (d < bestDist) {
        bestDist = d;
        best = cand;
      }
    }
  }
  return best !== null ? Math.max(0, best) : grid;
}

/**
 * Where a dragged widget slots into the vertical order.
 * @param {string} id @param {number} dx @param {number} dy @param {number} dw
 * @param {Record<string, Box>} items @param {(id: string) => number} heightOf
 */
export function orderForDrag(id, dx, dy, dw, items, heightOf) {
  const others = WIDGET_IDS.filter((k) => k !== id).sort(
    (a, b) => items[a].y - items[b].y || items[a].x - items[b].x,
  );
  let at = others.length;
  for (let i = 0; i < others.length; i += 1) {
    const o = items[others[i]];
    if (overlap(dx, dx + dw, o.x, o.x + o.w) && o.y + heightOf(others[i]) / 2 > dy) {
      at = i;
      break;
    }
  }
  others.splice(at, 0, id);
  return others;
}

/** Re-pack every widget's y from the current x/w/height, in reading order. */
export function relayout(/** @type {Record<string, Box>} */ items, /** @type {(id: string) => number} */ heightOf, /** @type {number} */ limit) {
  for (const id of WIDGET_IDS) fitRight(items[id], limit);
  const order = [...WIDGET_IDS].sort((a, b) => items[a].y - items[b].y || items[a].x - items[b].x);
  /** @type {Record<string, {x: number, w: number, h: number}>} */
  const dims = {};
  for (const id of order) dims[id] = { x: items[id].x, w: items[id].w, h: heightOf(id) };
  const out = layoutPass(order, dims);
  for (const id of order) items[id].y = out[id].y;
  return items;
}
