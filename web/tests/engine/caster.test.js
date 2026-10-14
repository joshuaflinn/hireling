import { test } from 'vitest';
import assert from 'node:assert/strict';

import { primaryCaster } from '../../src/lib/engine/caster.js';

// primaryCaster (E14 design D5): the sheet's spell-DC block selection,
// extracted so sheet and card share it. The guards below are ordered so the
// first case is the one a degraded extraction gets wrong: on the reference
// fixture's ordering ([non-innate, innate]) `find(!innate)` and `casters[0]`
// agree, so a test pinned to that ordering passes a broken rule silently.

/** Two caster blocks with distinct, assertable spell numbers. */
const INNATE = {
  caster_key: 'Wellspring Gnome',
  innate: true,
  spell_attack: { base: 8, total: 8, applied: [], suppressed: [] },
  spell_dc: { base: 18, total: 18, applied: [], suppressed: [] },
};
const TRADITION = {
  caster_key: 'Wizard',
  innate: false,
  spell_attack: { base: 9, total: 9, applied: [], suppressed: [] },
  spell_dc: { base: 19, total: 19, applied: [], suppressed: [] },
};

test('innate block ordered first: the non-innate block wins the pick', () => {
  // The discriminating ordering — the one case `casters[0]` gets wrong and
  // the reference fixture cannot express (round-1 finding F6).
  assert.equal(primaryCaster([INNATE, TRADITION]), TRADITION);
});

test('non-innate block first: the rule and first-block agree on this ordering', () => {
  // The reference fixture's own ordering — kept so the shared case stays
  // pinned too (the rule must not over-correct into "last non-innate").
  assert.equal(primaryCaster([TRADITION, INNATE]), TRADITION);
});

test('innate-only caster: fallback to the first block', () => {
  assert.equal(primaryCaster([INNATE]), INNATE);
});

test('empty caster list: null — no spell-DC entry on either surface', () => {
  assert.equal(primaryCaster([]), null);
});

test('null or undefined input: null, no throw', () => {
  assert.equal(primaryCaster(null), null);
  assert.equal(primaryCaster(undefined), null);
});
