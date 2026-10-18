// Regression: production 2026-10-15 — the party screen hung on skeletons and the
// live socket never came up. connection.js calls `timers.setTimeout(...)`, so a
// bare `{ setTimeout }` default runs with `this` = the timers object, which
// browsers reject ("TypeError: Illegal invocation"). Node and jsdom accept it,
// so the suite was blind. These tests pin the browser's receiver rule.
import { describe, it, expect, vi, afterEach } from 'vitest';
import { createThrottledWriter } from '../../src/lib/party/boot.js';
import { mockSockets, fakeStorage } from './fakes.js';

/** Install browser-strict timer globals: throw unless called with the global receiver. */
function strictTimers() {
  const realSet = globalThis.setTimeout;
  const realClear = globalThis.clearTimeout;
  const strictSet = function (fn, ms) {
    if (this !== undefined && this !== globalThis) throw new TypeError('Illegal invocation');
    return realSet(fn, ms);
  };
  const strictClear = function (id) {
    if (this !== undefined && this !== globalThis) throw new TypeError('Illegal invocation');
    return realClear(id);
  };
  vi.stubGlobal('setTimeout', strictSet);
  vi.stubGlobal('clearTimeout', strictClear);
}

afterEach(() => vi.unstubAllGlobals());

describe('default timers survive method-call receivers (browser rule)', () => {
  // Drives the production path, not the default object: SheetView.svelte:52
  // calls createSync() with no `timers`, and the watchdog arms from both
  // `onopen` and every inbound frame (connection.js:154, :167) — the two
  // sites the outage threw from. The strict globals go in *before*
  // sync/index.js evaluates (resetModules + dynamic import), because a
  // module-level `{ setTimeout, clearTimeout }` captures jsdom's originals at
  // load time and never consults a later stub — which is how the first cut of
  // this test passed against the bug it names.
  it('createSync default timers survive onopen + onmessage watchdog arming', async () => {
    strictTimers();
    vi.resetModules();
    const { createSync } = await import('../../src/lib/sync/index.js');
    const mocks = mockSockets();
    const sync = createSync({
      url: 'ws://test/party/1',
      storage: fakeStorage(),
      accountSub: 'sub-a',
      socketFactory: mocks.factory,
    });
    expect(() => {
      sync.connect();
      mocks.sockets[0].open();
      mocks.sockets[0].message(JSON.stringify({ t: 'snapshot', fields: [] }));
    }).not.toThrow();
  });

  it('createThrottledWriter default timers do not throw on schedule + flush', () => {
    strictTimers();
    const fn = vi.fn();
    const w = createThrottledWriter(fn, 10);
    expect(() => {
      w.call('a');
      w.flush();
    }).not.toThrow();
    expect(fn).toHaveBeenCalled();
  });
});
