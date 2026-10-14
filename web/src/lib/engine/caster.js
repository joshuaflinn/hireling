// The spell-DC block selection, shared by the sheet's StatsPane and the
// party-card stat block (E14 design D5). Extracted verbatim from
// StatsPane's `$derived` so both surfaces pick the same block — one rules
// choice, one home (spec FR-3 statement 4: sheet equality). Pure lib —
// components render, libs compute and test.

/**
 * Pick the caster block whose spell numbers a stat display shows.
 *
 * Rules-flavored: innate caster blocks rank below tradition blocks (the
 * tradition block is the character's real spellcasting; the innate block is
 * a secondary tradition), so the first non-innate block wins. An
 * innate-only caster falls back to the first block; no casters at all is
 * null — no spell-DC entry on either surface.
 *
 * @param {Array<Record<string, *>> | null | undefined} casters the engine
 *   output's `derived.casters` (E8 contract §3)
 * @returns {Record<string, *> | null} the selected caster block, or null
 */
export function primaryCaster(casters) {
  const list = Array.isArray(casters) ? casters : [];
  return list.find((/** @type {any} */ caster) => !caster.innate) ?? list[0] ?? null;
}
