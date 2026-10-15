// Regression: production 2026-10-15 — the party screen hung on skeletons and the
// live socket never came up. connection.js calls `timers.setTimeout(...)`, so a
// bare `{ setTimeout }` default runs with `this` = the timers object, which
// browsers reject ("TypeError: Illegal invocation"). Node and jsdom accept it,
// so the suite was blind. These tests pin the browser's receiver rule.
import { describe, it, expect, vi, afterEach } from 'vitest';
import { defaultTimers } from '../../src/lib/sync/index.js';
import { createThrottledWriter } from '../../src/lib/party/boot.js';

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
  it('sync defaultTimers can be called as timers.setTimeout / timers.clearTimeout', () => {
    strictTimers();
    expect(() => {
      const id = defaultTimers.setTimeout(() => {}, 10);
      defaultTimers.clearTimeout(id);
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
