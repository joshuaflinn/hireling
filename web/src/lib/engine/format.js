// Pure display formatting for the sheet (E6 Task 4) — the prototype's
// helpers, ported verbatim where the bytes match. No I/O, no framework.

/** The rank table (prototype line 718): letter + name per proficiency rank. */
const RANKS = {
  0: ['U', 'Untrained'],
  2: ['T', 'Trained'],
  4: ['E', 'Expert'],
  6: ['M', 'Master'],
  8: ['L', 'Legendary'],
};

/**
 * Signed modifier with the prototype's true minus (U+2212): `sg()` at line
 * 972. +3 / −1.
 * @param {number} n @returns {string}
 */
export function signed(n) {
  return (n < 0 ? '−' : '+') + Math.abs(n);
}

/**
 * Ability modifier for a 10-centered score.
 * @param {number} score @returns {number}
 */
export function abilityMod(score) {
  return Math.floor((score - 10) / 2);
}

/**
 * Proficiency bonus (prototype `pb`): rank > 0 → rank + level, else 0.
 * @param {number} rank @param {number} level @returns {number}
 */
export function profBonus(rank, level) {
  return rank > 0 ? rank + level : 0;
}

/**
 * Rank letter: U / T / E / M / L (missing ranks round down to the nearest
 * even table entry).
 * @param {number} rank @returns {string}
 */
export function rankLetter(rank) {
  const keys = Object.keys(RANKS)
    .map(Number)
    .sort((a, b) => a - b);
  let letter = RANKS[0][0];
  for (const key of keys) {
    if (rank >= key) letter = RANKS[key][0];
  }
  return letter;
}

/**
 * Rank name: Untrained / Trained / Expert / Master / Legendary.
 * @param {number} rank @returns {string}
 */
export function rankName(rank) {
  const keys = Object.keys(RANKS)
    .map(Number)
    .sort((a, b) => b - a);
  for (const key of keys) {
    if (rank >= key) return RANKS[key][1];
  }
  return RANKS[0][1];
}

/**
 * Spell rank ordinal: 1st / 2nd / 3rd / 4th … (prototype `ordn`).
 * @param {number} rank @returns {string}
 */
export function ordinal(rank) {
  return rank === 1 ? '1st' : rank === 2 ? '2nd' : rank === 3 ? '3rd' : `${rank}th`;
}

/**
 * Bulk text from tenths (prototype `bulkTxt`, line 2004): "3 Bulk + 2 L",
 * "1 Bulk", "2 L", "negligible".
 * @param {number} tenths @returns {string}
 */
export function bulkText(tenths) {
  const bulk = Math.floor(tenths / 10);
  const light = tenths % 10;
  if (bulk && light) return `${bulk} Bulk + ${light} L`;
  if (bulk) return `${bulk} Bulk`;
  if (light) return `${light} L`;
  return 'negligible';
}

/**
 * Bulk display for a map value: the resolved tenths, or "—" for a corpus
 * gap (design §5: gaps never block).
 * @param {number|null} tenths @returns {string}
 */
export function bulkTextOrDash(tenths) {
  return tenths === null || tenths === undefined ? '—' : bulkText(tenths);
}
