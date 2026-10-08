import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';

import {
  bootCacheKey,
  readBootCache,
  writeBootCache,
  createThrottledWriter,
  requestPersistentStorage,
} from '../../src/lib/party/boot.js';

afterEach(() => {
  vi.useRealTimers();
});

/** Map-backed storage matching the repo's fakeStorage shape. */
function fakeStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, v),
    size: () => map.size,
  };
}

test('a refused write degrades as documented — never throws, warns once', () => {
  const throwing = {
    getItem: () => null,
    setItem: () => {
      throw new DOMException('quota exceeded', 'QuotaExceededError');
    },
  };
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  assert.doesNotThrow(
    () => writeBootCache(throwing, 's', { party_id: 1 }, '{"fields":[]}'),
    'FR-9: a cache that cannot be written is degradation, not an error',
  );
  assert.doesNotThrow(() => writeBootCache(throwing, 's', { party_id: 1 }, '{"fields":[]}'));
  assert.equal(warn.mock.calls.length, 1, 'warned once, not per failed write');
});

test('boot cache keys isolate per account', () => {
  assert.equal(bootCacheKey('a'), 'hireling:boot:a');
  assert.notEqual(bootCacheKey('a'), bootCacheKey('b'));
});

test('write then read round-trips roster and snapshot verbatim', () => {
  const storage = fakeStorage();
  const roster = { party_id: 1, you: { sub: 's', role: 'player' }, characters: [{ id: 3 }] };
  const snapshot = JSON.stringify({ fields: [{ field: { kind: 'vitals' }, value: 9, version: 4 }] });
  writeBootCache(storage, 's', roster, snapshot);
  const read = readBootCache(storage, 's');
  assert.deepEqual(read.roster, roster);
  assert.deepEqual(read.snapshot, JSON.parse(snapshot));
  assert.equal(typeof read.saved_at, 'number', 'saved_at is diagnostics only');
  assert.equal(readBootCache(storage, 'other'), null, 'another sub reads nothing');
});

test('a corrupt cache reads as absent — never a crash on boot', () => {
  const storage = fakeStorage();
  storage.setItem(bootCacheKey('s'), '{not json');
  assert.equal(readBootCache(storage, 's'), null);
  assert.equal(readBootCache(storage, 'never-written'), null);
});

test('the throttled writer coalesces bursts into one durable write', () => {
  vi.useFakeTimers();
  const storage = fakeStorage();
  let writes = 0;
  const writer = createThrottledWriter(
    (roster) => {
      writes += 1;
      writeBootCache(storage, 's', roster, '{"fields":[]}');
    },
    2000,
  );
  writer.call({ n: 1 });
  writer.call({ n: 2 });
  writer.call({ n: 3 });
  assert.equal(writes, 0, 'nothing written inside the window');
  vi.advanceTimersByTime(2100);
  assert.equal(writes, 1, 'the burst lands as one trailing write');
  assert.equal(readBootCache(storage, 's').roster.n, 3, 'last call wins');
  assert.equal(storage.size(), 1, 'one key, one write');

  writer.call({ n: 4 });
  writer.flush();
  assert.equal(writes, 2, 'flush writes the pending call immediately');
  assert.equal(readBootCache(storage, 's').roster.n, 4);
  writer.flush();
  assert.equal(writes, 2, 'flush with nothing pending writes nothing');
});

test('requestPersistentStorage: true on grant, false without the API, never rejects', async () => {
  assert.equal(await requestPersistentStorage({ storage: { persist: async () => true } }), true);
  assert.equal(await requestPersistentStorage({}), false, 'no storage object — honest false');
  assert.equal(
    await requestPersistentStorage({ storage: { persist: async () => Promise.reject(new Error('no')) } }),
    false,
    'a denial is a logged condition, not a crash',
  );
  assert.equal(await requestPersistentStorage(undefined), false, 'no navigator at all');
});
