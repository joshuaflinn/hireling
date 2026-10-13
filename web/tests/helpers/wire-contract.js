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

const kinds = fixture.targets;
const effectOps = new Set(fixture.effect_ops);

/** Assert one write frame's `target` against the pinned contract: the
 * `kind` must be one the server decodes, and the key set must match the
 * contract exactly — a missing or an extra sibling field both fail.
 * @param {{kind: string}} target the frame's `target` value */
export function assertWriteTarget(target) {
  assert.ok(target && typeof target === 'object', 'a write target is an object');
  const entry = kinds[target.kind];
  assert.ok(
    entry,
    `unknown write target kind "${target.kind}" — the pinned contract knows: ${Object.keys(kinds).join(', ')}`,
  );
  assert.deepEqual(
    Object.keys(target).filter((key) => key !== 'kind').sort(), // the tag is checked above, not a sibling
    [...entry.fields].sort(),
    `the "${target.kind}" target's field set must match the pinned contract exactly`,
  );
}

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
