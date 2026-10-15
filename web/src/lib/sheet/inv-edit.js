// Dave's inventory editing (specs/015 slice 8), over the synced
// sheet_ui.inv overrides. Pure functions; the panel renders their output.
//
// Overrides per item name: equipped (bool), loc (container name or null =
// top level), container (bool: a user-marked container), tags (string[]
// of magic|gems|trade), removed (bool, restorable). Quantities stay on the
// existing inv field — these are display/organization only.

export const INV_TABS = /** @type {const} */ ([
  ['all', 'All Equipment'],
  ['equipped', 'Equipped Items'],
  ['magic', 'Magic Items'],
  ['gems', 'Gems/Jewelry/Art'],
  ['trade', 'Trade Goods'],
]);

export const TAG_LABEL = /** @type {Record<string, string>} */ ({ magic: 'Magic', gems: 'Gem/Art', trade: 'Trade' });

const MAGIC_TRAITS = new Set(['magical', 'invested', 'artifact', 'staff', 'wand']);

/**
 * Tags derived from the Foundry item record — Dave's derivedTags, fed by the
 * corpus instead of his regex: treasure gems/art → gems, `precious` trait or
 * treasure trade goods → trade, magical traits / runes → magic.
 * @param {any} rec corpus entry (kind item) or null
 * @param {{runes?: boolean}} [hint]
 */
export function derivedTags(rec, hint = {}) {
  /** @type {string[]} */
  const tags = [];
  const traits = new Set((rec?.traits ?? []).map((/** @type {string} */ t) => t.toLowerCase()));
  const cat = String(rec?.detail?.type ?? '').toLowerCase();
  if (hint.runes || [...traits].some((t) => MAGIC_TRAITS.has(t))) tags.push('magic');
  if (cat === 'gem' || cat === 'art-object' || cat === 'art') tags.push('gems');
  if (traits.has('precious') || cat === 'trade-good' || cat === 'raw-material') tags.push('trade');
  return tags;
}

/**
 * @typedef {Object} InvRow
 * @property {string} name
 * @property {number} qty
 * @property {number|null} bulkTenths
 * @property {string[]} traits
 * @property {boolean} invested
 * @property {string|null} loc  container name, null = top level
 * @property {boolean} equipped
 * @property {string[]} tags
 * @property {boolean} isContainer
 * @property {boolean} extradimensional
 * @property {boolean} backpack
 */

/**
 * Build the organized view.
 * @param {{equipment: any[], containers: any[]}} base
 * @param {Record<string, any>} inv sheet_ui.inv overrides
 * @param {(name: string) => number} qtyOf
 * @param {(name: string) => number|null} bulkOf tenths
 * @param {(name: string) => string[]} traitsOf
 * @param {(name: string) => any} recOf corpus record (may be null)
 */
export function organize(base, inv, qtyOf, bulkOf, traitsOf, recOf) {
  const exportContainers = new Map((base.containers ?? []).map((c) => [c.name, c]));
  /** @type {InvRow[]} */
  const rows = [];
  /** @type {string[]} */
  const removed = [];
  // Export containers with no equipment row of their own (a Pathbuilder
  // bag of holding often has none) still exist as containers.
  const equipment = [...(base.equipment ?? [])];
  for (const c of base.containers ?? []) {
    if (!equipment.some((e) => e.name === c.name)) equipment.push({ name: c.name, qty: 1, container: null, synthetic: true });
  }
  for (const item of equipment) {
    const o = inv?.[item.name] ?? {};
    if (o.removed) {
      removed.push(item.name);
      continue;
    }
    const qty = item.synthetic ? 1 : qtyOf(item.name);
    if (qty <= 0) continue;
    const ec = exportContainers.get(item.name);
    const isContainer = Boolean(ec) || Boolean(o.container);
    const rec = recOf(item.name);
    rows.push({
      name: item.name,
      qty,
      bulkTenths: bulkOf(item.name),
      traits: traitsOf(item.name),
      invested: Boolean(item.invested),
      loc: o.loc !== undefined ? o.loc : (item.container ?? null),
      equipped: o.equipped !== undefined ? Boolean(o.equipped) : Boolean(ec?.backpack),
      tags: Array.isArray(o.tags) ? o.tags : derivedTags(rec),
      isContainer,
      extradimensional: Boolean(ec?.extradimensional),
      backpack: Boolean(ec?.backpack),
    });
  }
  // A container nested in a container is lifted to the top level (Dave's rule).
  const containerNames = new Set(rows.filter((r) => r.isContainer).map((r) => r.name));
  for (const r of rows) {
    if (r.isContainer && r.loc !== null) r.loc = null;
    if (r.loc !== null && !containerNames.has(r.loc)) r.loc = null;
  }
  return { rows, removed: removed.sort((a, b) => a.localeCompare(b)) };
}

/**
 * Bulk total: everything carried; extradimensional contents excluded; a worn
 * backpack's first 2 Bulk (20 tenths) of contents don't count (PC p. 272).
 * @param {InvRow[]} rows
 */
export function carriedTenths(rows) {
  const byName = new Map(rows.map((r) => [r.name, r]));
  let total = 0;
  /** @type {Map<string, number>} */
  const inside = new Map();
  for (const r of rows) {
    const t = (r.bulkTenths ?? 0) * r.qty;
    if (r.loc !== null) {
      const c = byName.get(r.loc);
      if (c?.extradimensional) continue;
      inside.set(r.loc, (inside.get(r.loc) ?? 0) + t);
    }
    total += t;
  }
  for (const [name, sum] of inside) {
    const c = byName.get(name);
    if (c?.backpack && c.equipped) total -= Math.min(sum, 20);
  }
  return Math.max(0, total);
}

/** Dave's Bulk thresholds: encumbered above 5+Str, max 10+Str (in Bulk). */
export function bulkLimits(/** @type {number} */ strMod) {
  return { encumbered: 5 + strMod, max: 10 + strMod };
}

/**
 * Patch one item's overrides, returning the next whole inv map.
 * @param {Record<string, any>} inv
 * @param {string} name
 * @param {Record<string, any>} patch
 */
export function patchInv(inv, name, patch) {
  const next = { ...(inv ?? {}) };
  next[name] = { ...(next[name] ?? {}), ...patch };
  return next;
}
