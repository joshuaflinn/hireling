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
 * reference and lore rows are shallow copies with their display label —
 * the wire's `label` (contract §3, verbatim from the export) when it
 * shipped one, otherwise derived from the key. The input is never mutated
 * and provenance (`applied`/`suppressed`) rides along untouched.
 *
 * @param {Array<Record<string, *>> | null | undefined} skills the engine's
 *   `derived.skills` (post-swap the lores ride inside it as `lore:<name>`
 *   entries)
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
      // The wire's lore rows carry the lore's display name (contract §3
      // `label`, verbatim from the export — the canonical key lowercases
      // and underscores, so the display case lives only there). The
      // sheet's row form is "<name> Lore" (the prototype's); derive from
      // the key only when the wire shipped no name.
      const display =
        typeof skill?.label === 'string' && skill.label
          ? skill.label
          : name.slice('lore:'.length);
      lores.push({ ...skill, label: `${display} Lore` });
    } else {
      core.push(skill);
    }
  }
  return { core, lores };
}
