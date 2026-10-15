/* global process */

// The client side of the wire contract gate (MOR-122). The sync socket's
// two sides are built in two languages: the server decodes with the types
// in `src/sync/protocol.rs`, the client builds frames in
// `src/lib/sheet/state.js`. One checked-in fixture —
// `specs/007-party-sync/contracts/field-targets.json` — is the shared
// source of truth: the Rust suite (`src/tests/sync/contract.rs`) pins that
// file to the serde wire shape, and these helpers pin every client write
// frame to the same file. A drifted key set turns a suite red here instead
// of dying as an undecodable frame at the socket.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import assert from 'node:assert/strict';

// vitest runs with the package dir as cwd (`npm --prefix web test`), so the
// repo-root fixture resolves the same way the gate and CI read it
// (convention: tests/pwa/install.test.js).
const fixture = JSON.parse(
  readFileSync(resolve(process.cwd(), '../specs/007-party-sync/contracts/field-targets.json'), 'utf8'),
);

// Why this file cannot be imported from `src/`, and what replaced that import:
// `web/tsconfig.json`'s include is `src/**` only, and svelte-check's language
// service reads no config below `web/`, so ANY `node:fs` import that reaches the
// client check graph is unresolvable — 2580 on four lines, and the reason a
// production module here was red while the same text in a test was invisible.
// So the pin's DATA lives in `src/lib/contract/wire-contract.js` as literals and
// this file keeps the fixture-reading GATE: every literal over there is asserted
// equal to the fixture here, on a production path, so the duplication cannot
// drift quietly — the copy is checked against the source of truth rather than
// trusted.
const browserPin = await import('../../src/lib/contract/wire-contract.js');
assert.deepEqual(
  [...browserPin.stashOps].sort(),
  [...fixture.stash_ops].sort(),
  'src/lib/contract/wire-contract.js `stashOps` must equal the fixture\'s `stash_ops`',
);
assert.deepEqual(
  [...browserPin.vitalsFields].sort(),
  [...fixture.vitals_fields].sort(),
  'src/lib/contract/wire-contract.js `vitalsFields` must equal the fixture\'s `vitals_fields`',
);
assert.deepEqual(
  Object.fromEntries(
    Object.entries(browserPin.targetFields).map(([k, v]) => [k, { fields: [...v.fields].sort() }]),
  ),
  Object.fromEntries(
    Object.entries(fixture.targets).map(([k, v]) => [k, { fields: [...v.fields].sort() }]),
  ),
  'src/lib/contract/wire-contract.js `targetFields` must equal the fixture\'s `targets`',
);

// (`fixture.targets` has no local reader here any more: the target-kind gate
//  moved to src/lib/contract/wire-contract.js and is pinned to the fixture by
//  the `targetFields` equality assertion above.)
const effectOps = new Set(fixture.effect_ops);
// E12 §3: the stash row's `op` is the second enum-valued wire value. The
// fixture pins the legal set (`qty`/`meta` on main today); this set pins the
// client's literals to it, so a client that builds `{op:"transfer"}` before
// lane L3 adds it fails a suite here instead of dying as a `rejected` ack at
// the socket. The server's own bounds are src/sync/write.rs's accepted set.
const stashOps = new Set(fixture.stash_ops);
if (stashOps.size === 0) {
  throw new Error('the contract fixture pins no stash_ops — refusing to gate without it');
}
// gh#91: the vitals target's `field` is the one enum-valued wire value —
// the fixture pins the legal set, the Rust suite pins the fixture to the
// serde names, and this set pins the client's literals to the fixture.
const vitalsFields = new Set(fixture.vitals_fields);
if (vitalsFields.size === 0) {
  throw new Error('the contract fixture pins no vitals_fields — refusing to gate without it');
}

// `assertStashOp` and `assertWriteTarget`'s data are re-exported from the
// browser-safe half rather than kept here: production code under `src/` must
// reach the pin WITHOUT importing this file (see the header above), and the two
// forms must be the same assertion, not a pair that can disagree.
export { assertStashOp, assertWriteTarget } from '../../src/lib/contract/wire-contract.js';

/** Assert an effect frame's `value.op` against the accepted op set (the
 * write path's bounds: create on `effect_new`, update/end on `effect`).
 * @param {{op: string}} value the write frame's `value` */
export function assertEffectOp(value) {
  assert.ok(value && typeof value === 'object', 'an effect write value is an object');
  assert.ok(
    effectOps.has(value.op),
    `unknown effect op "${value.op}" — the pinned contract accepts: ${[...effectOps].join(', ')}`,
  );
}
