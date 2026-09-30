// Durable per-account write queue — E7 (degraded-mode contract §3).
//
// The queue is dumb on purpose: no validation, no drop paths, no client-side
// skip of "doomed" ops — the server owns every verdict. Ops ride a JSON array
// under `hireling:queue:{accountSub}` so a reload (or a service-worker
// replay) picks up exactly what was enqueued, FIFO, unbounded. Storage is the
// three-function interface from design.md (`getItem`/`setItem`) so
// `node --test` runs without a DOM; the browser hands us `localStorage`.
//
// Per-account keying is the boundary: one user's queue can never replay
// another's (logout/re-login with a non-empty queue is an E10 first-run
// migration, not this module's problem).

/**
 * @typedef {Object} QueueOp
 * @property {string} op_id
 * @property {object} target
 * @property {number} base_version
 * @property {*} value
 * @property {string} created_at
 */

/**
 * @param {{ storage: { getItem: (k: string) => string | null, setItem: (k: string, v: string) => void }, accountSub: string }} options
 */
export function createQueue(options) {
  const { storage, accountSub } = options;
  const key = `hireling:queue:${accountSub}`;
  /** @type {Array<QueueOp>} */
  let ops = load();
  const listeners = new Set();

  function load() {
    const raw = storage.getItem(key);
    if (raw === null) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  }

  function persist() {
    storage.setItem(key, JSON.stringify(ops));
  }

  function emit() {
    for (const cb of listeners) cb();
  }

  return {
    /** Appends to the tail. Never refuses, never drops. */
    enqueue(/** @type {QueueOp} */ newOp) {
      ops.push(newOp);
      persist();
      emit();
    },

    /** FIFO snapshot (oldest first) — a copy; callers can't mutate the queue. */
    peekAll() {
      return ops.slice();
    },

    /** Removes and returns the named op, or null if it isn't queued. */
    dequeue(/** @type {string} */ opId) {
      const index = ops.findIndex((candidate) => candidate.op_id === opId);
      if (index === -1) return null;
      const [removed] = ops.splice(index, 1);
      persist();
      emit();
      return removed;
    },

    get length() {
      return ops.length;
    },

    /** @param {() => void} cb */
    onChange(cb) {
      listeners.add(cb);
    },
  };
}
