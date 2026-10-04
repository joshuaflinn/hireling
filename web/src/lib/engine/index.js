// The engine seam (E6 design §4): ONE function the sheet consumes for every
// derived number. E6 renders; it never computes. The output shape is the E8
// engine-output contract (specs/008/contracts/engine-output.md §3) —
// referenced, never duplicated here.
//
// The adapter swap is done: the seam FORWARDS engine output and nothing
// else. The server computes (contract Q3 — "No consumer computes; everyone
// renders"); the output rides E7's wire (the catch-up snapshot's `derived`
// array and per-character `derived` frames, design D4–D6) and reaches this
// module through the sync surface. `base.js` — the base-only interim math —
// is deleted, not kept in parallel (design §4).
//
// Null is an honest answer: until the wire delivers the character's output
// (a beat after connect on a live socket), there is nothing to render and
// the sheet shows its loading state — no placeholder numbers, no local
// derivation standing in.

/**
 * Forward the sheet's numbers for one character, verbatim from the wire.
 *
 * @param {import('../sync/index.js').Sync} sync the sync surface (the only
 *   derived source the app has)
 * @param {number} characterId the character whose output to forward
 * @returns {Record<string, *> | null} the EngineOutput contract shape, or
 *   null before the wire has delivered it
 */
export function derive(sync, characterId) {
  return sync.derived(characterId);
}
