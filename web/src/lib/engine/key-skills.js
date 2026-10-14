// Key-skills selection (E14 design D4, spec FR-2): which three skills a
// party card shows. Pure lib — components render, libs compute and test.
// Selection, not computation (spec FR-3): the comparator reads wire fields
// and never does arithmetic over them, so every seat picks the same three
// from the same output — no locale collation, no insertion order, no
// timestamp enters it.

/**
 * Pick a character's key skills: `total` descending, then `rank`
 * descending (trained investment beats a coincidental ability bonus), then
 * canonical `name` ascending (lexicographic on the wire key, `lore:`
 * prefix included). Slice to `count`; fewer rows → all of them, never
 * padded. Untrained rows (rank 0) are filtered by nothing — rank what is.
 *
 * @param {Array<Record<string, *>> | null | undefined} skills the engine's
 *   `derived.skills` (core rows and `lore:` rows, E8 contract Q2)
 * @param {number} [count=3] how many to pick (the PRD fixed three)
 * @returns {Array<Record<string, *>>} the picked rows, shallow copies with
 *   `label` resolved for display: lore rows render as "<label or
 *   key-derived> Lore" — the convention E6's `partition.js` owns, mirrored
 *   in its two lines and guarded by the parity test (drift-prone by
 *   design, so the test is the guard); core rows capitalize the key
 */
export function keySkills(skills, count = 3) {
  const list = Array.isArray(skills) ? skills : [];
  return [...list]
    .sort(
      (/** @type {any} */ a, /** @type {any} */ b) =>
        b.total - a.total ||
        b.rank - a.rank ||
        (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
    )
    .slice(0, count)
    .map(displayRow);
}

/** The display label for one picked row (see `keySkills`).
 * @param {Record<string, *>} skill @returns {Record<string, *>} */
function displayRow(skill) {
  const name = typeof skill?.name === 'string' ? skill.name : '';
  if (name.startsWith('lore:')) {
    const display =
      typeof skill?.label === 'string' && skill.label
        ? skill.label
        : name.slice('lore:'.length);
    return { ...skill, label: `${display} Lore` };
  }
  return { ...skill, label: name.charAt(0).toUpperCase() + name.slice(1) };
}
