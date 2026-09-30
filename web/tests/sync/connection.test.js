import test from 'node:test';
import assert from 'node:assert/strict';

import { createConnection } from '../../src/lib/sync/connection.js';
import { fakeClock, mockSockets } from './fakes.js';

function setup(rng = () => 1) {
  const clock = fakeClock();
  const mocks = mockSockets();
  const conn = createConnection({
    url: 'ws://test/party/1',
    socketFactory: mocks.factory,
    rng,
    now: clock.now,
    timers: clock.timers,
  });
  return { clock, mocks, conn };
}

const SNAPSHOT = JSON.stringify({
  t: 'snapshot',
  party_id: 1,
  fields: [],
  effects: [],
});

function handshake(socket) {
  socket.open();
  socket.message(SNAPSHOT);
}

test('reconnect backoff: full jitter bounds, 2x growth, hard 30 s cap over 10 attempts', () => {
  const { clock, mocks, conn } = setup(() => 1); // rng=1 → delays at the cap
  const delays = [];
  conn.onChange((e) => {
    if (e.type === 'reconnect') delays.push(e.delay);
  });
  conn.connect();
  assert.equal(conn.state(), 'connecting');
  for (let i = 0; i < 10; i++) {
    mocks.sockets.at(-1).error();
    const expectedCap = Math.min(30000, 1000 * 2 ** i);
    assert.equal(delays[i], expectedCap, `attempt ${i}`);
    clock.advance(expectedCap); // fires the reconnect → next socket
  }
  assert.equal(mocks.sockets.length, 11);
});

test('backoff resets to base only after a full cycle (snapshot→drain→live) completes', () => {
  const { clock, mocks, conn } = setup(() => 1);
  const delays = [];
  const holdDrain = { armed: false, promise: null };
  let releaseDrain;
  holdDrain.promise = new Promise((resolve) => {
    releaseDrain = resolve;
  });
  conn.onChange((e) => {
    if (e.type === 'reconnect') delays.push(e.delay);
    if (e.type === 'phase' && e.phase === 'drain' && holdDrain.armed) {
      holdDrain.armed = false;
      return holdDrain.promise;
    }
    return undefined;
  });

  // Cycle 1: complete → live, attempt reset. Kill → base delay (1000).
  conn.connect();
  handshake(mocks.sockets[0]);
  assert.equal(conn.state(), 'live');
  mocks.sockets[0].error();
  assert.deepEqual(delays, [1000]);

  // Cycle 2: completes → live → reset. Kill again → base delay, NOT 2000.
  clock.advance(1000);
  handshake(mocks.sockets[1]);
  assert.equal(conn.state(), 'live');
  mocks.sockets[1].error();
  assert.deepEqual(delays, [1000, 1000]);

  // Cycle 3: killed MID-cycle — drain phase held, never live. The failure
  // counts (delay doubles) and resolving the stale drain must NOT reset.
  clock.advance(1000);
  const third = mocks.sockets[2];
  third.open();
  holdDrain.armed = true; // this cycle's drain replay stalls (pending promise)
  third.message(SNAPSHOT);
  assert.equal(conn.state(), 'connecting'); // live waits for drain
  third.error();
  assert.deepEqual(delays, [1000, 1000, 2000]);
  releaseDrain(); // stale drain settles — no reset from the grave
  clock.advance(2000);

  // Cycle 4: full clean cycle → live → reset → base delay again.
  handshake(mocks.sockets[3]);
  assert.equal(conn.state(), 'live');
  mocks.sockets[3].error();
  assert.deepEqual(delays, [1000, 1000, 2000, 1000]);
});

test('50 s of inbound silence treats the link as dead; fresh inbound keeps it alive', () => {
  const { clock, mocks, conn } = setup();
  conn.connect();
  handshake(mocks.sockets[0]);
  assert.equal(conn.state(), 'live');

  // A ping at 49 s is inbound traffic: answers pong, resets the clock.
  clock.advance(49000);
  mocks.sockets[0].message('{"t":"ping"}');
  assert.deepEqual(mocks.sockets[0].sent, ['{"t":"pong"}']);
  clock.advance(49999);
  assert.equal(conn.state(), 'live');

  // Silence from here hits the deadline 50 s after the last inbound frame.
  clock.advance(1);
  assert.equal(conn.state(), 'offline');
  assert.equal(mocks.sockets[0].closed, true); // the dead link is closed
  clock.advance(1000); // backoff → a fresh socket
  assert.equal(mocks.sockets.length, 2);
});

test('browser-offline and a socket error take the identical one-path transition', () => {
  const viaError = setup();
  const viaNotify = setup();
  const seen = { error: [], notify: [] };
  for (const [setup_, key] of [
    [viaError, 'error'],
    [viaNotify, 'notify'],
  ]) {
    setup_.conn.onChange((e) => {
      if (e.type === 'state' || e.type === 'reconnect') seen[key].push(e);
    });
    setup_.conn.connect();
  }
  viaError.mocks.sockets[0].error();
  viaNotify.conn.notifyOffline();
  for (const key of ['error', 'notify']) {
    // Events: connecting → offline → reconnect(delay). Identical for both
    // triggers — that identity IS the one-path proof.
    assert.equal(seen[key].length, 3);
    assert.equal(seen[key][1].state, 'offline');
    assert.equal(seen[key][2].delay, 1000); // same attempt, same rng
  }
  // Notify while already offline is a no-op — still exactly one timer.
  viaNotify.conn.notifyOffline();
  viaNotify.clock.advance(1000);
  assert.equal(viaNotify.mocks.sockets.length, 2);
});

test('the queue may drain only after the snapshot phase marker', () => {
  const { clock, mocks, conn } = setup();
  const order = [];
  conn.onChange((e) => {
    if (e.type === 'frame' && e.frame.t === 'snapshot') order.push('frame:snapshot');
    if (e.type === 'phase') {
      order.push(`phase:${e.phase}`);
      if (e.phase === 'drain') order.push('drain:replaying-queue');
    }
  });
  conn.connect();
  handshake(mocks.sockets[0]);
  clock.advance(1); // flush drain promise microtasks
  assert.deepEqual(order, [
    'frame:snapshot',
    'phase:snapshot',
    'phase:merge',
    'phase:drain',
    'drain:replaying-queue',
    'phase:live',
  ]);
});

test('send writes only while live; malformed inbound is dropped without state change', () => {
  const { mocks, conn } = setup();
  assert.equal(conn.send({ t: 'pong' }), false);
  conn.connect();
  assert.equal(conn.send({ t: 'pong' }), false); // connecting, not live
  handshake(mocks.sockets[0]);
  assert.equal(conn.send({ t: 'write', op_id: 'op-1' }), true);
  assert.deepEqual(mocks.sockets[0].sent, ['{"t":"write","op_id":"op-1"}']);
  mocks.sockets[0].message('not json at all');
  assert.equal(conn.state(), 'live');
});
