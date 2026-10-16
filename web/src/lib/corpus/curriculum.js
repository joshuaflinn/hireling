// Wizard school curricula from the Foundry pack (specs/015 slice 6) —
// replaces Dave's hardcoded CURRICULA table. A school class-feature's
// description lists its curriculum as <li><strong>cantrips:</strong> …
// @UUID[...spells-srd.Item.<id>]{Name} …</li>, one <li> per rank.

const RANK_WORD = /** @type {Record<string, number>} */ ({
  cantrips: 0,
  cantrip: 0,
  '1st': 1,
  '2nd': 2,
  '3rd': 3,
  '4th': 4,
  '5th': 5,
  '6th': 6,
  '7th': 7,
  '8th': 8,
  '9th': 9,
});

/**
 * Parse a curriculum out of a school description.
 * @param {string} html
 * @returns {Record<number, string[]>} rank → spell names ({} when absent)
 */
export function parseCurriculum(html) {
  /** @type {Record<number, string[]>} */
  const out = {};
  const text = String(html ?? '');
  const start = text.search(/<strong>\s*Curriculum\s*<\/strong>/i);
  if (start < 0) return out;
  const items = text.slice(start).match(/<li>[\s\S]*?<\/li>/g) ?? [];
  for (const li of items) {
    const head = li.match(/<strong>\s*([^<:]+?)\s*:?\s*<\/strong>/);
    if (!head) continue;
    const rank = RANK_WORD[head[1].trim().toLowerCase()];
    if (rank === undefined) continue;
    const names = [...li.matchAll(/@UUID\[Compendium\.pf2e\.spells-srd\.Item\.[A-Za-z0-9]+\]\{([^}]+)\}/g)].map(
      (m) => m[1],
    );
    if (names.length) out[rank] = names;
  }
  return out;
}

/** The school name from a Pathbuilder specials list ("Arcane School: X"). */
export function schoolOf(/** @type {string[]} */ specials) {
  for (const s of specials ?? []) {
    const m = String(s).match(/^Arcane School:\s*(.+)$/);
    if (m) return m[1].trim();
  }
  return null;
}
