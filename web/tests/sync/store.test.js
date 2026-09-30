import test from 'node:test';
import assert from 'node:assert/strict';

import { createStore, targetKey } from '../../src/lib/sync/store.js';

const VITALS = (cid = 7) => ({ kind: 'vitals', character_id: cid, field: 'hp' });
const MONEY = (cid = 7) => ({ kind: 'vitals', character_id: cid, field: 'money' });

function op(id, target, baseVersion, value) {
  return {
    op_id: id,
    target,
    base_version: baseVersion,
    value,
    created_at: '2026-09-30T00:00:00Z',
  };
}

test('newer server fields apply; state reflects value and version', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 25, version: 5 },
  );
});

test('an older version is ignored — state unchanged', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.applyServerField(VITALS(), 10, 4);
  const field = s.state()[targetKey(VITALS())];
  assert.equal(field.value, 25);
  assert.equal(field.version, 5);
  // The same version twice is not "newer" either (own-echo guard).
  s.applyServerField(VITALS(), 99, 5);
  assert.equal(s.state()[targetKey(VITALS())].value, 25);
});

test('enqueueView shows the optimistic echo and flips isSyncing', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  assert.equal(s.state()[targetKey(VITALS())].value, 30);
  assert.equal(s.isSyncing(), true);
});

test('applied ack confirms the echo and settles the field', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  s.ack('op-1', 'applied', 6, 30);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 30, version: 6 },
  );
  assert.equal(s.isSyncing(), false);
});

test('own-write echo after the applied ack is a no-op', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  s.ack('op-1', 'applied', 6, 30);
  const events = [];
  s.subscribe((e) => events.push(e.type));
  // The writer's own diff comes back at the committed version.
  s.applyServerField(VITALS(), 30, 6);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 30, version: 6 },
  );
  // And it is not news: no changed event for a version we already have.
  assert.deepEqual(events, []);
});

test('superseded ack reverts to the server value and never exposes an error', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  const events = [];
  s.subscribe((e) => events.push(e));
  s.ack('op-1', 'superseded', 9, 41);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 41, version: 9 },
  );
  assert.equal(s.isSyncing(), false);
  // Silence rule (FR-8): no error, no exposure — a lost race is not a fault.
  assert.deepEqual(
    events.filter((e) => e.type === 'error' || e.type === 'op_exposed'),
    [],
  );
});

test('superseded ack without a newer value falls back to the pre-echo server state', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  // Ack beat the superseding diff across the wire: no winner known yet.
  s.ack('op-1', 'superseded', null, null);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 25, version: 5 },
  );
});

test('a newer server diff while pending does not break the echo, and the ack settles cleanly', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  // Someone else moved the field while our write was in flight.
  s.applyServerField(VITALS(), 28, 6);
  assert.equal(s.state()[targetKey(VITALS())].value, 30);
  // We lost the race: revert lands on the newest server truth.
  s.ack('op-1', 'superseded', null, null);
  assert.equal(s.state()[targetKey(VITALS())].value, 28);
});

test('rejected ack reverts and exposes the op record for E6', () => {
  const s = createStore();
  s.applyServerField(MONEY(), { gp: 10 }, 3);
  s.enqueueView(op('op-9', MONEY(), 3, { gp: -999 }));
  const events = [];
  s.subscribe((e) => events.push(e));
  s.ack('op-9', 'rejected', 3, { gp: 10 });
  const field = s.state()[targetKey(MONEY())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: { gp: 10 }, version: 3 },
  );
  assert.equal(s.isSyncing(), false);
  const exposed = events.filter((e) => e.type === 'op_exposed');
  assert.equal(exposed.length, 1);
  assert.equal(exposed[0].op_id, 'op-9');
  assert.equal(exposed[0].outcome, 'rejected');
  assert.deepEqual(exposed[0].op.value, { gp: -999 });
});

test('forbidden ack behaves like rejected — revert plus exposure', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-2', VITALS(), 5, 30));
  const events = [];
  s.subscribe((e) => events.push(e));
  s.ack('op-2', 'forbidden', 5, 25);
  assert.equal(s.state()[targetKey(VITALS())].value, 25);
  const exposed = events.filter((e) => e.type === 'op_exposed');
  assert.equal(exposed.length, 1);
  assert.equal(exposed[0].outcome, 'forbidden');
});

test('applied ack with a stale version still clears pending without regressing the field', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  s.enqueueView(op('op-1', VITALS(), 5, 30));
  // A teammate's newer write landed before our ack was processed.
  s.applyServerField(VITALS(), 40, 7);
  s.ack('op-1', 'applied', 6, 30);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 40, version: 7 },
  );
  assert.equal(s.isSyncing(), false);
});

test('two devices interleavings converge regardless of arrival order', () => {
  const history = [
    [VITALS(), 10, 1],
    [VITALS(), 20, 2],
    [VITALS(), 30, 3],
    [MONEY(), { gp: 1 }, 1],
    [MONEY(), { gp: 2 }, 2],
  ];
  const build = (order) => {
    const s = createStore();
    for (const [target, value, version] of order) {
      s.applyServerField(target, value, version);
    }
    return s.state();
  };
  const forward = build(history);
  const backward = build(history.slice().reverse());
  const shuffled = build([history[3], history[0], history[4], history[2], history[1]]);
  assert.deepEqual(backward, forward);
  assert.deepEqual(shuffled, forward);
});

test('targets with the same data in different key orders are the same field', () => {
  const s = createStore();
  s.applyServerField(VITALS(), 25, 5);
  const reordered = { field: 'hp', character_id: 7, kind: 'vitals' };
  s.applyServerField(reordered, 30, 6);
  const field = s.state()[targetKey(VITALS())];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 30, version: 6 },
  );
  assert.equal(Object.keys(s.state()).length, 1);
});
