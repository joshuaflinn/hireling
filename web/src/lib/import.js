// Import-page logic (E5 FR-18): the testable core behind the minimal page.
// Paste and upload converge on one POST body; the result renderer maps the
// server's outcome and every post-import diff kind to human lines.

/**
 * @typedef {Object} ImportSummary
 * @property {string} [name]
 * @property {number} [level]
 * @property {boolean} [first_import]
 */
/**
 * @typedef {Object} ImportOutcome
 * @property {ImportSummary} [character]
 * @property {{first_import?: boolean}} [diff]
 * @property {{skipped_fields: number}} [advisory]
 */
/**
 * @typedef {Object} SubmitResult
 * @property {boolean} ok
 * @property {number} status
 * @property {any} payload — whatever the server answered with; parsed
 *   defensively, so `any` is the honest type at this boundary.
 */

/**
 * Build the POST body from the paste box. File upload lands in
 * `readFileToText` and takes the identical path — same body either way.
 * @param {string} text
 * @returns {string}
 */
export function buildBody(text) {
  return text;
}

/**
 * Read an uploaded file to its text content (the same body a paste gives).
 * @param {File} file
 * @returns {Promise<string>}
 */
export async function readFileToText(file) {
  return file.text();
}

/**
 * Submit an export body to the import endpoint.
 * @param {string} body
 * @returns {Promise<SubmitResult>}
 */
export async function submit(body) {
  let response;
  try {
    response = await fetch('/api/characters/import', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: buildBody(body),
    });
  } catch {
    return { ok: false, status: 0, payload: null };
  }
  /** @type {any} */
  let payload = null;
  try {
    payload = await response.json();
  } catch {
    payload = null; // a 500 body, say — the code fallback speaks instead
  }
  return { ok: response.ok, status: response.status, payload };
}

/** @type {Record<string, string>} */
const CODE_FALLBACKS = {
  'payload-too-large': 'That file is too large to be a character export.',
  'payload-too-deep': 'That JSON is nested too deeply to be a character export.',
  'invalid-json': 'That isn\u2019t valid JSON.',
  'not-pathbuilder': 'That doesn\u2019t look like a Pathbuilder export.',
};

/**
 * The error line to show: the server's exact message, or a fallback when
 * the response carried none (0 = unreachable server).
 * @param {SubmitResult} result
 * @returns {string}
 */
export function errorFor(result) {
  const message = result?.payload?.error?.message;
  if (typeof message === 'string' && message.length > 0) return message;
  const code = result?.payload?.error?.code;
  if (typeof code === 'string' && code in CODE_FALLBACKS) return CODE_FALLBACKS[code];
  if (result?.status === 0) return 'The server is unreachable right now.';
  return 'The import failed unexpectedly. Try again.';
}

/**
 * Every diff kind (data-model §5) as human-readable lines, plus the
 * skipped-field count line.
 * @param {{ character?: ImportSummary, diff?: any, advisory?: {skipped_fields: number} }} outcome
 * @returns {string[]}
 */
export function describeOutcome(outcome) {
  const lines = [];
  const character = outcome?.character ?? {};
  lines.push(
    character.first_import
      ? `Imported ${character.name} (level ${character.level}) — welcome to the party.`
      : `Re-imported ${character.name} (level ${character.level}).`,
  );
  const diff = outcome?.diff ?? {};
  for (const kept of diff.kept_unmatched ?? []) {
    lines.push(
      kept.kind === 'slot'
        ? `Kept slot ${kept.caster_key} rank ${kept.rank} #${Number(kept.slot_index) + 1}${kept.used ? ' (used)' : ''} — it left the new export.`
        : `Kept item ${kept.name} (tracked change ${Number(kept.qty_delta) > 0 ? '+' : ''}${kept.qty_delta}) — it left the new export.`,
    );
  }
  for (const seeded of diff.seeded ?? []) {
    lines.push(`Seeded new slot ${seeded.caster_key} rank ${seeded.rank} #${Number(seeded.slot_index) + 1}.`);
  }
  for (const divergence of diff.prep_divergence ?? []) {
    lines.push(
      `Prep kept at ${divergence.caster_key} rank ${divergence.rank} #${Number(divergence.slot_index) + 1}: table says "${divergence.live}", export said "${divergence.export}".`,
    );
  }
  for (const notice of diff.notices ?? []) {
    if (notice.kind === 'negative_quantity') {
      lines.push(
        `${notice.name} would show ${Number(notice.base_qty) + Number(notice.delta)} (tracked ${Number(notice.delta) > 0 ? '+' : ''}${notice.delta}) — check it at the table.`,
      );
    } else if (notice.kind === 'max_hp_changed') {
      lines.push(`Max HP changed ${notice.old} → ${notice.new}.`);
    } else if (notice.kind === 'section_skipped') {
      lines.push(`Section "${notice.section}" didn't import (unrecognized shape) — re-export if you need it.`);
    }
  }
  const skipped = outcome?.advisory?.skipped_fields ?? 0;
  if (skipped > 0) {
    lines.push(`Skipped ${skipped} unrecognized field${skipped === 1 ? '' : 's'} — nothing broke.`);
  }
  if (lines.length === 1 && !character.first_import) {
    lines.push('Everything matches the live table — no changes.');
  }
  return lines;
}
