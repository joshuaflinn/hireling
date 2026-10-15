// E12 Task 6 — the client half of the wire-contract gate.
//
// `specs/007-party-sync/contracts/field-targets.json` is the shared truth:
// `src/tests/sync/contract.rs` pins that file to the Rust serde types, and
// `web/tests/helpers/wire-contract.js` pins every CLIENT write frame to it.
// This suite is where the stash store's frames go through that pin, so a
// drifted key set or an unwired `op` literal turns a suite red here instead of
// dying as an undecodable frame at the socket.
//
// The rule that makes this suite worth its runtime: assert the frame the
// PRODUCTION store builds, not a frame hand-written to look right. Every case
// below calls a `stash.js` write action and pins whatever came out of it.

import { test } from 'vitest';
import assert from 'node:assert/strict';

import { createStashState } from '../../src/lib/party/stash.js';
import { assertWriteTarget, assertStashOp } from '../helpers/wire-contract.js';

/** Collect everything the store pushes at the seam. */
function spySync() {
  const writes = [];
  return {
    writes,
    connect() {},
    disconnect() {},
    hangUp() {},
    write(target, value, baseVersion) {
      writes.push({ target, value, base_version: baseVersion ?? 0 });
    },
    state: () => ({}),
    derived: () => null,
    isSyncing: () => false,
    snapshotForBoot: () => '{"fields":[]}',
    queue: () => [],
    connectionState: () => 'live',
    subscribe: () => () => false,
  };
}

/** Every frame the store built, with the target pin run over it. Returns the
 * values so a test can pin the op without re-walking the array. */
function pinFrames(writes) {
  return writes.map((op) => {
    assertWriteTarget(op.target);
    return op.value;
  });
}

test('every stash frame the store builds passes the pinned contract', () => {
  const sync = spySync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.addRow({ item_name: 'Rope', quantity: 2, bulk: 1, notes: '50 ft' });
  stash.adjustQty('Torch', -2);
  stash.editMeta('Lantern', { bulk: 0.1, notes: 'bullseye' });
  stash.setQuartermaster(8);
  stash.adjustBank({ gp: 1, sp: 2, cp: 3 });

  assert.equal(sync.writes.length, 5, 'one frame per action — no hidden extra writes');
  const values = pinFrames(sync.writes);

  // Only the two `stash`-kinded frames carry an `op`, and both must be in the
  // pinned set: the mint (`stash_new`) and the bank/party frames do not.
  const opBearing = sync.writes
    .filter((op) => op.target.kind === 'stash')
    .map((op) => op.value);
  assert.equal(opBearing.length, 2, 'qty and meta are the two op-bearing stash frames');
  for (const value of opBearing) assertStashOp(value);

  assert.deepEqual(values[0], { item_name: 'Rope', quantity: 2, bulk: 1, notes: '50 ft' });
  assert.deepEqual(values[2], { op: 'meta', bulk: 0.1, notes: 'bullseye' });
  assert.deepEqual(values[3], { character_id: 8 });
  assert.deepEqual(values[4], { gp_delta: 1, sp_delta: 2, cp_delta: 3 });
});

test('the pin actually bites: an unwired stash op fails the gate, not the socket', () => {
  // The negative control. Without this, a helper that silently accepted
  // anything would pass the suite above just as green — a pin that cannot
  // fail is not a pin. `transfer` is the op wire-protocol §3 says lane L3
  // ADDS, so it is outside today's set and must be caught here.
  assert.throws(
    () => assertStashOp({ op: 'transfer', to_character_id: 7, qty_delta: -2 }),
    /unknown stash op "transfer"/,
  );
  // ...and the op the contract DOES accept must not throw, or the negative
  // control above would pass against a helper that rejects everything.
  assertStashOp({ op: 'qty', qty_delta: -2 });
});

test('a target with a drifted key set fails the pin', () => {
  // Two more negative controls, one per failure mode the fixture guards:
  // a missing sibling and an extra one. Both are silent at the socket until
  // serde refuses the frame.
  assert.throws(
    () => assertWriteTarget({ kind: 'stash', party_id: 1 }), // item_name missing
    /field set must match the pinned contract/,
  );
  assert.throws(
    () => assertWriteTarget({ kind: 'bank', party_id: 1, item_name: 'Torch' }), // extra sibling
    /field set must match the pinned contract/,
  );
  // The shape the store really builds passes.
  assertWriteTarget({ kind: 'bank', party_id: 1 });
});
