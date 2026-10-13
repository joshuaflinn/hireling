// The curated-prose lookup (E9 design D1): the seed is a build-time static
// module — no runtime fetch — so tooltips render offline at the table
// (FR-10; the SW caches the shell it rides in). Join rule: case-insensitive
// exact name against the seed's lowercase keys; a miss gets the fallback
// shape, never invented prose (design D9.2). Curation is by PR to the seed
// file (E4's condition-tiers.json pattern); in-app editing is E15.
import seed from './condition-prose.json';

/**
 * One lookup result. `found: false` is the "no paraphrase yet" shape — the
 * tip renders name + badges + fallback note and nothing else.
 *
 * @typedef {object} ProseEntry
 * @property {boolean} found
 * @property {string} text the paraphrase ('' on a miss)
 * @property {number | null} page Player Core page cite
 * @property {number | null} aonId Archives of Nethys condition id
 * @property {boolean} link false ⇒ excluded from linkification
 *   (objects-only / meta conditions — the prototype's `nl` flag)
 */

/** Collapse casing and whitespace runs to the seed's key form. @param {unknown} name */
function keyOf(name) {
  return String(name ?? '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Look up one condition's curated prose by name (ci-exact).
 *
 * @param {string} name
 * @returns {ProseEntry}
 */
export function lookup(name) {
  const entry = /** @type {any} */ (seed)[keyOf(name)];
  if (!entry || typeof entry.text !== 'string') {
    return { found: false, text: '', page: null, aonId: null, link: false };
  }
  return {
    found: true,
    text: entry.text,
    page: typeof entry.page === 'number' ? entry.page : null,
    aonId: typeof entry.aonId === 'number' ? entry.aonId : null,
    link: entry.link !== false,
  };
}
