import { test } from 'vitest';
import assert from 'node:assert/strict';

import { createSync } from '../../src/lib/sync/index.js';
import { targetKey } from '../../src/lib/sync/store.js';
import { fakeClock, mockSockets, fakeStorage } from './fakes.js';

const VITALS = { kind: 'vitals', character_id: 7, field: 'hp' };
const SNAPSHOT = JSON.stringify({
  t: 'snapshot',
  fields: [{ field: VITALS, value: 25, version: 5 }],
  snapshot_bytes: 42,
});

function setup(storage = fakeStorage()) {
  const clock = fakeClock();
  const mocks = mockSockets();
  let opSeq = 0;
  const sync = createSync({
    url: 'ws://test/party/1',
    storage,
    accountSub: 'sub-a',
    socketFactory: mocks.factory,
    rng: () => 1,
    now: clock.now,
    timers: clock.timers,
    idFactory: () => `op-${++opSeq}`,
  });
  return { clock, mocks, sync, storage };
}

function handshake(socket) {
  socket.open();
  socket.message(SNAPSHOT);
}

test('write() enqueues, echoes optimistically, and survives a fake reload', () => {
  const { sync, storage } = setup();
  sync.write(VITALS, 30);
  assert.equal(sync.isSyncing(), true);
  assert.equal(sync.state()[targetKey(VITALS)].value, 30);

  // A fresh instance over the same storage sees the same durable queue —
  // the reload story: nothing enqueued is ever lost to a page turn.
  const reloaded = createSync({
    url: 'ws://test/party/1',
    storage,
    accountSub: 'sub-a',
    socketFactory: mockSockets().factory,
    rng: () => 1,
    now: () => 0,
    timers: fakeClock().timers,
    idFactory: () => 'op-x',
  });
  assert.deepEqual(
    reloaded.queue().map((o) => o.op_id),
    ['op-1'],
  );
});

test('empty queue + dead socket: the indicator reads false', () => {
  const { sync } = setup();
  assert.equal(sync.connectionState(), 'offline');
  assert.equal(sync.isSyncing(), false);
});

test('a live write rides the socket; the applied ack settles field and queue', () => {
  const { mocks, sync } = setup();
  sync.connect();
  handshake(mocks.sockets[0]);
  assert.equal(sync.connectionState(), 'live');

  sync.write(VITALS, 30);
  assert.equal(sync.isSyncing(), true);
  const sent = JSON.parse(mocks.sockets[0].sent.at(-1));
  assert.deepEqual(
    { t: sent.t, op_id: sent.op_id, base_version: sent.base_version, value: sent.value },
    { t: 'write', op_id: 'op-1', base_version: 5, value: 30 },
  );

  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: 'op-1', outcome: 'applied', version: 6 }),
  );
  assert.equal(sync.isSyncing(), false);
  const field = sync.state()[targetKey(VITALS)];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 30, version: 6 },
  );
});

test('an offline write replays over the socket after the reconnect cycle', () => {
  const { mocks, sync } = setup();
  sync.write(VITALS, 30); // never connected — straight to the queue
  assert.deepEqual(mocks.sockets, []);
  sync.connect();
  handshake(mocks.sockets[0]);
  // The drain phase replayed the queue: the write went out on the wire.
  const sent = mocks.sockets[0].sent.map((raw) => JSON.parse(raw));
  assert.ok(sent.some((f) => f.t === 'write' && f.op_id === 'op-1' && f.value === 30));
});

test('superseded ack: op leaves the queue, state reverts to server truth, silently', () => {
  const { mocks, sync } = setup();
  sync.connect();
  handshake(mocks.sockets[0]);
  const events = [];
  sync.subscribe((e) => events.push(e));
  sync.write(VITALS, 30);
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: 'op-1', outcome: 'superseded', winning_version: 9 }),
  );
  assert.equal(sync.isSyncing(), false);
  const field = sync.state()[targetKey(VITALS)];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 25, version: 5 }, // pre-echo server truth until the winner's diff lands
  );
  assert.deepEqual(
    events.filter((e) => e.type === 'op_exposed' || e.type === 'error'),
    [],
  );
});

test('server diffs merge strictly-newer into the live state', () => {
  const { mocks, sync } = setup();
  sync.connect();
  handshake(mocks.sockets[0]);
  mocks.sockets[0].message(
    JSON.stringify({
      t: 'diff',
      field: VITALS,
      value: 27,
      version: 8,
      actor_sub: 'someone-else',
      op_id: null,
    }),
  );
  const field = sync.state()[targetKey(VITALS)];
  assert.deepEqual(
    { value: field.value, version: field.version },
    { value: 27, version: 8 },
  );
});

test('snapshotForBoot() serializes the merged state as a wire-shaped snapshot', () => {
  const { mocks, sync } = setup();
  sync.connect();
  handshake(mocks.sockets[0]);
  sync.write(VITALS, 30);
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: 'op-1', outcome: 'applied', version: 6 }),
  );
  const boot = JSON.parse(sync.snapshotForBoot());
  assert.deepEqual(boot, {
    fields: [{ field: VITALS, value: 30, version: 6 }],
  });
});

test('accounts are isolated: a different sub reads a different queue', () => {
  const { sync, storage } = setup();
  sync.write(VITALS, 30);
  const other = createSync({
    url: 'ws://test/party/1',
    storage,
    accountSub: 'sub-b',
    socketFactory: mockSockets().factory,
    rng: () => 1,
    now: () => 0,
    timers: fakeClock().timers,
    idFactory: () => 'op-x',
  });
  assert.deepEqual(other.queue(), []);
});
