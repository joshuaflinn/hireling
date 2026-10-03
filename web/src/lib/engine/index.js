// The engine seam (E6 design §4): ONE function the sheet consumes for every
// derived number. E6 renders; it never computes. The output shape is the E8
// engine-output contract (specs/008-buff-effect-engine/contracts/
// engine-output.md §3) — referenced, never duplicated here.
//
// Mode switch: base-only mode runs `base.js` until E8 lands — bounded,
// named debt with a deletion date (the E8 adapter swap deletes `base.js`,
// not this interface). When the engine ships, this body forwards engine
// output and nothing else in the app changes.

import { deriveBase } from './base.js';

/** @typedef {Record<string, *>} BaseSheetView */


/**
 * Derive the sheet's numbers for one character.
 *
 * @param {{ id: number, base_sheet: * }} character the bootstrap
 *   character payload (id + base_sheet)
 * @param {{ level_adjust?: number, effects?: Array<object> }} liveState the
 *   character's live state, extracted from the sync store by sheet/state.js
 * @returns {Record<string, *>} DerivedSheet per the engine-output contract
 *   (typed loosely here; the members the sheet reads are pinned in
 *   base.js's derivation and its tests).
 */
export function derive(character, liveState) {
  return deriveBase(character, liveState);
}
