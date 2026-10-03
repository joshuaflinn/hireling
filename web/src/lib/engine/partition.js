// Lore partition (E6 per MOR-50): E8 folds lores into `derived.skills` as
// `lore:<name>` entries; the sheet wants them back as a separate list with a
// display label (spec §2.2's lore rows). Pure transform in a lib module —
// web/ has no component test harness, so logic in a component ships
// untested.
//
// E6 owns the partition AND the label: the display label is derived here
// from the key (`lore:Underworld` → `Underworld Lore`), never trusted from
// the wire, so the render does not depend on the server shipping labels.

/**
 * Split the engine's skills array into core skills and lores.
 *
 * Entries whose `name` carries the `lore:` prefix are lores; everything
 * else is core. Both lists keep the input order; core rows pass through by
 * reference and lore rows are shallow copies with a derived `label` — the
 * input is never mutated and provenance (`applied`/`suppressed`) rides
 * along untouched.
 *
 * @param {Array<Record<string, *>> | null | undefined} skills the engine's
 *   `derived.skills` (post-swap the lores are already folded in; until the
 *   adapter swap deletes base.js, callers concatenate its separate `lores`
 *   array onto `derived.skills`)
 * @returns {{ core: Array<Record<string, *>>, lores: Array<Record<string, *>> }}
 *   core skills in order; lores in order, each with a derived `label`
 */
export function partitionSkills(skills) {
  const list = Array.isArray(skills) ? skills : [];
  /** @type {Array<Record<string, *>>} */
  const core = [];
  /** @type {Array<Record<string, *>>} */
  const lores = [];
  for (const skill of list) {
    const name = typeof skill?.name === 'string' ? skill.name : '';
    if (name.startsWith('lore:')) {
      lores.push({ ...skill, label: loreLabel(name) });
    } else {
      core.push(skill);
    }
  }
  return { core, lores };
}

/** `lore:Underworld` → `Underworld Lore` (the prototype's display form).
 * @param {string} name */
function loreLabel(name) {
  return `${name.slice('lore:'.length)} Lore`;
}
