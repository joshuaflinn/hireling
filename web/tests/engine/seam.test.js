import test from 'node:test';
import assert from 'node:assert/strict';

import { derive } from '../../src/lib/engine/index.js';

// The seam post-swap: one function, forwarding engine output verbatim from
// the sync surface. It computes nothing — hand it a fake sync and watch the
// object identity ride through. The deletion of base.js left no second
// computation path behind (design §4).

test('derive() forwards the sync surface answer for the character, verbatim', () => {
  const output = { schema: 'hireling.engine.output.v1', character_id: 7 };
  const sync = {
    derived(id) {
      assert.equal(id, 7, 'the seam asks for the right character');
      return output;
    },
  };
  assert.equal(derive(sync, 7), output, 'the same object — a forward, not a copy');
});

test('derive() forwards null — the honest pre-wire answer', () => {
  const sync = {
    derived: () => null,
  };
  assert.equal(derive(sync, 7), null);
});
