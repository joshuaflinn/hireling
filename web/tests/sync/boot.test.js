import { test } from 'vitest';
import assert from 'node:assert/strict';

import { createSync } from '../../src/lib/sync/index.js';
import { targetKey } from '../../src/lib/sync/store.js';
import { fakeClock, mockSockets, fakeStorage } from './fakes.js';

const hpField = (character_id, value, version) => ({
  field: { kind: 'vitals', character_id, field: 'hp' },
  value,
  version,
});

/** A sync wired to a mock socket; `deliver` plays a server frame. */
function setup(options = {}) {
  const clock = fakeClock();
  const mocks = mockSockets();
  const sync = createSync({
    url: 'ws://test/party/1',
    storage: fakeStorage(),
    accountSub: 'sub-a',
    socketFactory: mocks.factory,
    rng: () => 1,
    now: clock.now,
    timers: clock.timers,
    idFactory: () => 'op-1',
    ...options,
  });
  const deliver = (frame) => mocks.sockets.at(-1)?.message(JSON.stringify(frame));
  return { sync, mocks, deliver };
}

test('bootSnapshot seeds the store; versions still arbitrate afterwards', () => {
  const seed = hpField(1, 10, 5);
  const { sync, deliver } = setup({ bootSnapshot: { fields: [seed] } });

  // Seeded before any socket exists — the cold boot's first paint has data.
  const key = targetKey(seed.field);
  assert.equal(sync.state()[key]?.version, 5);
  assert.equal(sync.state()[key]?.value, 10);

  // Connect: a live snapshot with an OLDER hp version changes nothing
  // (strictly-newer merge — the seed is not a blind overwrite).
  sync.connect();
  deliver({ t: 'snapshot', fields: [hpField(1, 99, 4)] });
  assert.equal(sync.state()[key]?.value, 10);

  // A NEWER one wins — the seed is not sticky.
  deliver({ t: 'snapshot', fields: [hpField(1, 12, 6)] });
  assert.equal(sync.state()[key]?.value, 12);
  assert.equal(sync.state()[key]?.version, 6);
});

test('no bootSnapshot behaves exactly as before', () => {
  const { sync, deliver } = setup();
  assert.deepEqual(sync.state(), {}, 'empty store, no throw');
  sync.connect();
  deliver({ t: 'snapshot', fields: [hpField(1, 7, 2)] });
  const [only] = Object.values(sync.state());
  assert.equal(only?.value, 7, 'the live snapshot merges as always');
});
