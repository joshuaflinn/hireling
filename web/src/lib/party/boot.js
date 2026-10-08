// The boot cache (E10 design D6): the per-account persisted pair — roster
// payload + wire snapshot — that makes an offline cold boot honest.
// Keyed like the write queue (one key per account sub, `hireling:boot:`).
// `saved_at` is diagnostics only and is NEVER read for staleness: client
// clocks are untrusted, versions merge (data-model §3).

/* global setTimeout, clearTimeout, Date, JSON, console */

/**
 * The per-account cache key — the queue's isolation pattern.
 * @param {string} sub
 */
export function bootCacheKey(sub) {
  return `hireling:boot:${sub}`;
}

/**
 * Read one account's boot cache. Absent or corrupt reads as null — a cold
 * boot with no history is a normal state, never an error.
 *
 * @param {{ getItem: (k: string) => string | null }} storage
 * @param {string} sub
 * @returns {{ roster: *, snapshot: *, saved_at: number } | null}
 */
export function readBootCache(storage, sub) {
  try {
    const raw = storage.getItem(bootCacheKey(sub));
    if (!raw) return null;
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

/** Warn-once flag: a storage that refuses every write would otherwise
 *  repeat the same console line on every throttled write, forever.
 *  Module state — in production that is one line per tab, which is the
 *  point. Test constraint: exactly one warn-count assertion per file
 *  (vitest isolates module registries per file, so the flag starts fresh
 *  in each); a second case in the same file asserting the count races
 *  this flag and sees zero. */
let warnedWriteFailure = false;

/**
 * Persist one account's boot pair. `snapshotJson` is `sync.snapshotForBoot()`
 * output — parsed here so the stored shape is one JSON document.
 *
 * @param {{ setItem: (k: string, v: string) => void }} storage
 * @param {string} sub
 * @param {*} roster the roster response, verbatim
 * @param {string} snapshotJson
 */
export function writeBootCache(storage, sub, roster, snapshotJson) {
  try {
    storage.setItem(
      bootCacheKey(sub),
      JSON.stringify({ roster, snapshot: JSON.parse(snapshotJson), saved_at: Date.now() }),
    );
  } catch (error) {
    // A cache that cannot be written (quota pressure, private browsing)
    // degrades exactly as documented (FR-9): last-known state is
    // best-effort. It must never ride a throw out of a store subscriber —
    // the writer rides the sync's event loop, and live sync outranks the
    // cache. Warn once; every failed write after the first repeats a fact
    // already on the console.
    if (!warnedWriteFailure) {
      warnedWriteFailure = true;
      console.warn(`boot cache write failed — last-known state stays best-effort (${error})`);
    }
  }
}

/**
 * A trailing throttle: N calls inside the window collapse into the last
 * one firing once, after the window. Timers are injectable (the sync
 * module's pattern); production passes nothing and gets the globals.
 *
 * @template A
 * @param {(...args: A[]) => void} fn
 * @param {number} ms
 * @param {{ setTimeout: (fn: () => void, ms: number) => *, clearTimeout: (id: *) => void }} [timers]
 * @returns {{ call: (...args: A[]) => void, flush: () => void }}
 */
export function createThrottledWriter(fn, ms, timers = { setTimeout, clearTimeout }) {
  /** @type {*} */
  let pending = null;
  /** @type {A[] | null} */
  let queuedArgs = null;
  return {
    call(...args) {
      queuedArgs = args;
      if (pending !== null) return; // a write is already scheduled; last call wins
      pending = timers.setTimeout(() => {
        pending = null;
        if (queuedArgs) fn(...queuedArgs);
        queuedArgs = null;
      }, ms);
    },
    /** Write anything pending right now (page hide, logout). */
    flush() {
      if (pending === null) return;
      timers.clearTimeout(pending);
      pending = null;
      if (queuedArgs) fn(...queuedArgs);
      queuedArgs = null;
    },
  };
}

/**
 * Ask the browser to spare last-known state from eviction (FR-9; E7
 * contract §3's sanctioned durability upgrade). Logged once, never thrown,
 * never blocking: a denial behaves exactly as today's documented limit.
 *
 * @param {{ storage?: { persist?: () => Promise<boolean> } }} [navigatorRef]
 * @returns {Promise<boolean>}
 */
export async function requestPersistentStorage(navigatorRef = globalThis.navigator) {
  try {
    const persist = navigatorRef?.storage?.persist?.bind(navigatorRef.storage);
    if (!persist) return false;
    const granted = await persist();
    console.info(`persistent storage ${granted ? 'granted' : 'denied'} — last-known state stays best-effort on denial`);
    return granted;
  } catch (error) {
    console.info(`persistent storage request failed: ${error}`);
    return false;
  }
}
