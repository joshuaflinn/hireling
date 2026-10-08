import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';

import { createPartySession, RosterUnavailable } from '../../src/lib/party/session.js';
import { targetKey } from '../../src/lib/sync/store.js';

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const SUB = 'dev-sub-josh';

const ROSTER = {
  party_id: 1,
  you: { sub: SUB, role: 'player' },
  characters: [{ character: { id: 7, name: 'Flinn', owner: SUB }, vitals: { hp: 20 } }],
};

const hpTarget = (character_id) => ({ kind: 'vitals', character_id, field: 'hp' });

function fakeStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, v),
  };
}

/** A fetchImpl with a mutable world. */
function fakeFetch({ ok = true, body = ROSTER, fail = false } = {}) {
  const calls = [];
  return {
    calls,
    impl: async (url) => {
      calls.push(url);
      if (fail) throw new TypeError('network is gone');
      return {
        ok,
        status: ok ? 200 : 409,
        json: async () => body,
      };
    },
  };
}

function fakeSockets() {
  const sockets = [];
  const factory = (url) => {
    const s = {
      url,
      closed: false,
      onopen: null,
      onmessage: null,
      onclose: null,
      onerror: null,
      send() {},
      close() {
        s.closed = true;
        s.onclose?.();
      },
      open() {
        s.onopen?.();
      },
      message(data) {
        s.onmessage?.({ data });
      },
    };
    sockets.push(s);
    return s;
  };
  return { sockets, factory };
}

const noopPersist = async () => false;

test('online: roster fetched, sync aims at the roster party, cache written', async () => {
  vi.useFakeTimers();
  const fetcher = fakeFetch();
  const sockets = fakeSockets();
  const storage = fakeStorage();
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    socketFactory: sockets.factory,
    requestPersist: noopPersist,
  });
  assert.deepEqual(fetcher.calls, ['/api/party/roster']);
  assert.equal(session.offlineColdBoot, false);
  assert.equal(session.roster.party_id, 1);

  session.sync.connect();
  assert.equal(
    sockets.sockets[0]?.url,
    'ws://localhost:3000/api/ws/party/1',
    'the session sync addresses the roster party, not a constant',
  );

  const cached = JSON.parse(storage.getItem(`hireling:boot:${SUB}`));
  assert.equal(cached.roster.party_id, 1, 'the roster lands in the boot cache');
  assert.deepEqual(cached.snapshot, { fields: [] });
});

test('offline with a cache: cold boot from last-known state, read-only intent', async () => {
  vi.useFakeTimers();
  const storage = fakeStorage();
  storage.setItem(
    `hireling:boot:${SUB}`,
    JSON.stringify({
      roster: ROSTER,
      snapshot: { fields: [{ field: hpTarget(7), value: 18, version: 3 }] },
      saved_at: 1,
    }),
  );
  const fetcher = fakeFetch({ fail: true });
  const sockets = fakeSockets();
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    socketFactory: sockets.factory,
    requestPersist: noopPersist,
  });
  assert.equal(session.offlineColdBoot, true, 'the caller styles this read-only');
  assert.deepEqual(session.roster, ROSTER, 'the cached roster renders');
  const field = session.sync.state()[targetKey(hpTarget(7))];
  assert.equal(field?.value, 18, 'the sync is seeded from the cached snapshot');
  assert.equal(field?.version, 3);
});

test('offline without a cache: RosterUnavailable, no cachedRoster to offer', async () => {
  const fetcher = fakeFetch({ fail: true });
  await assert.rejects(
    () =>
      createPartySession({
        fetchImpl: fetcher.impl,
        storage: fakeStorage(),
        account: { sub: SUB },
        requestPersist: noopPersist,
      }),
    (error) => error instanceof RosterUnavailable && error.cachedRoster === null,
  );
});

test('a failed roster answer (not ok) with a cache also cold-boots', async () => {
  const storage = fakeStorage();
  storage.setItem(
    `hireling:boot:${SUB}`,
    JSON.stringify({ roster: ROSTER, snapshot: { fields: [] }, saved_at: 1 }),
  );
  const fetcher = fakeFetch({ ok: false });
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    requestPersist: noopPersist,
  });
  assert.equal(session.offlineColdBoot, true);
});

test('sync events throttle-write the snapshot into the boot cache', async () => {
  vi.useFakeTimers();
  const fetcher = fakeFetch();
  const storage = fakeStorage();
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    requestPersist: noopPersist,
  });

  session.sync.write(hpTarget(7), 12); // an optimistic write emits queue+fields events
  assert.equal(
    JSON.parse(storage.getItem(`hireling:boot:${SUB}`)).snapshot.fields.length,
    0,
    'nothing written inside the throttle window',
  );
  await vi.advanceTimersByTimeAsync(2100);
  const snapshot = JSON.parse(storage.getItem(`hireling:boot:${SUB}`)).snapshot;
  const field = snapshot.fields.find((f) => f.value === 12);
  assert.ok(field, 'the merged store state reached the boot cache');
});

test('a refused boot-cache write never takes the sync down (subscriber loop)', async () => {
  vi.useFakeTimers();
  const fetcher = fakeFetch();
  const storage = fakeStorage();
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  let refuseBootWrites = false;
  const realSetItem = storage.setItem;
  storage.setItem = (k, v) => {
    // Refuse ONLY the boot-cache key: the write queue's persistence is
    // E7's degradation story, not this test's subject. Quota pressure on
    // the boot pair is what rides the subscriber loop.
    if (refuseBootWrites && String(k).startsWith('hireling:boot:')) {
      throw new DOMException('quota exceeded', 'QuotaExceededError');
    }
    realSetItem(k, v);
  };
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    requestPersist: noopPersist,
  });

  // Quota pressure / private browsing: every cache write now throws. The
  // writer rides the sync's event loop — the throw must die in the
  // writer, never in the frame path.
  refuseBootWrites = true;
  assert.doesNotThrow(() => session.sync.write(hpTarget(7), 12));
  await vi.advanceTimersByTimeAsync(2100); // the throttled write fires, swallowed
  assert.doesNotThrow(() => session.sync.write(hpTarget(7), 15), 'the sync still lives');
  assert.equal(warn.mock.calls.length, 1, 'warned once, not per failed write');
});

test('persist() is asked exactly once per account across two sessions', async () => {
  const fetcher = fakeFetch();
  const storage = fakeStorage();
  const persist = vi.fn(async () => true);
  const options = () => ({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    requestPersist: persist,
  });
  await createPartySession(options());
  await createPartySession(options());
  assert.equal(persist.mock.calls.length, 1, 'the ask is once per sub, ever');
});

test('close() hangs the link up: a signed-out tab never reopens the socket', async () => {
  // Real time, no fake timers — the reconnect scheduler holds the sync
  // module's defaultTimers binding, so an injected clock cannot prove
  // absence here. rng is pinned to 0: the pre-fix bug reconnected at once
  // (0 ms delay), so a 50 ms real wait exposes it.
  const rngSpy = vi.spyOn(Math, 'random').mockReturnValue(0);
  const fetcher = fakeFetch();
  const sockets = fakeSockets();
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage: fakeStorage(),
    account: { sub: SUB },
    socketFactory: sockets.factory,
    requestPersist: noopPersist,
  });
  session.sync.connect();
  assert.equal(sockets.sockets.length, 1);

  session.close();
  assert.equal(sockets.sockets[0].closed, true, 'the socket dies with the session');
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert.equal(sockets.sockets.length, 1, 'no reconnect ever rides out of close()');
  rngSpy.mockRestore();
});

test('flush() lands the trailing write and leaves the link alone', async () => {
  vi.useFakeTimers();
  const fetcher = fakeFetch();
  const sockets = fakeSockets();
  const storage = fakeStorage();
  const session = await createPartySession({
    fetchImpl: fetcher.impl,
    storage,
    account: { sub: SUB },
    socketFactory: sockets.factory,
    requestPersist: noopPersist,
  });
  session.sync.connect();
  sockets.sockets[0].open();
  sockets.sockets[0].message(
    JSON.stringify({ t: 'snapshot', fields: [{ field: hpTarget(7), value: 16, version: 2 }] }),
  );
  await vi.advanceTimersByTimeAsync(1); // the merge schedules a throttled cache write

  session.flush();
  const cached = JSON.parse(storage.getItem(`hireling:boot:${SUB}`));
  assert.equal(
    cached.snapshot.fields.find((/** @type {any} */ f) => f.value === 16) !== undefined,
    true,
    'page hide must not lose the ≤2 s trailing write',
  );
  assert.equal(sockets.sockets[0].closed, false, 'flush is not a hang-up — the page owns the socket now');
});
