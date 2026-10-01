// Public sync API — composes queue + store + connection into the one module
// E6, the service worker, and E10 consume (E7 design.md "Client module").
//
// The rules that survive composition:
// - `write()` never refuses and never throws — it enqueues (degraded-mode
//   contract §2) and paints the optimistic echo immediately. Live sockets
//   get the frame at once; offline it rides the queue to the next drain.
// - The indicator (`isSyncing`) reads the QUEUE length only — never the
//   socket state (empty queue + dead link = nothing to do = false).
// - The queue drains only after the connection's snapshot phase marker (the
//   connection owns that ordering); replay stops at the first failed send
//   and the remainder rides the next cycle.
// - Acks settle store and queue together: `applied`/`already_applied` carry
//   the committed version plus the op's own value (CAS committed exactly
//   that); every other outcome passes no server value — the store falls
//   back to the newest server truth it has, silently for `superseded`.

/* global WebSocket, window, setTimeout, clearTimeout, crypto */

import { createQueue } from './queue.js';
import { createStore, targetKey } from './store.js';
import { createConnection } from './connection.js';

/** @typedef {import('./connection.js').SyncSocket} SyncSocket */

const WS_PATH = '/api/ws/party';

/** @param {string} url @returns {SyncSocket} */
function defaultSocketFactory(url) {
  if (typeof WebSocket === 'undefined') {
    throw new Error('no WebSocket in this environment; inject socketFactory');
  }
  return /** @type {SyncSocket} */ (new WebSocket(url));
}

const defaultTimers = { setTimeout, clearTimeout };

/**
 * Absolute ws(s) URL for a party socket from the current page location.
 * @param {number} partyId
 */
export function partySocketUrl(partyId) {
  const { protocol, host } = window.location;
  const scheme = protocol === 'https:' ? 'wss:' : 'ws:';
  return `${scheme}//${host}${WS_PATH}/${partyId}`;
}

/**
 * @param {{
 *   url: string,
 *   storage: { getItem: (k: string) => string | null, setItem: (k: string, v: string) => void },
 *   accountSub: string,
 *   socketFactory?: (url: string) => SyncSocket,
 *   rng?: () => number,
 *   now?: () => number,
 *   timers?: { setTimeout: (fn: () => void, ms: number) => *, clearTimeout: (id: *) => void },
 *   idFactory?: () => string,
 * }} options
 */
export function createSync(options) {
  const {
    url,
    storage,
    accountSub,
    socketFactory = defaultSocketFactory,
    rng = Math.random,
    now = Date.now,
    timers = defaultTimers,
    idFactory = () => crypto.randomUUID(),
  } = options;

  const queue = createQueue({ storage, accountSub });
  const store = createStore();
  const listeners = new Set();

  /** @param {Record<string, *>} event */
  function emit(event) {
    for (const cb of listeners) cb(event);
  }

  const connection = createConnection({ url, socketFactory, rng, timers });

  connection.onChange((event) => {
    if (event.type === 'state') {
      emit({ type: 'connection', state: event.state });
    } else if (event.type === 'phase' && event.phase === 'drain') {
      replayQueue(); // drain phase: the queue may finally ride the wire
    } else if (event.type === 'frame' && event.frame) {
      handleFrame(event.frame);
    }
  });

  /** The drain-phase callback: replay the durable queue in FIFO order. */
  function replayQueue() {
    for (const op of queue.peekAll()) {
      const sent = connection.send({
        t: 'write',
        op_id: op.op_id,
        target: op.target,
        base_version: op.base_version,
        value: op.value,
      });
      if (!sent) return; // link died mid-replay; the rest rides the next cycle
    }
  }

  /** @param {Record<string, *>} frame */
  function handleFrame(frame) {
    if (frame.t === 'snapshot') {
      const fields = /** @type {Array<Record<string, *>>} */ (frame.fields ?? []);
      for (const f of fields) {
        store.applyServerField(f.field, f.value, /** @type {number} */ (f.version));
      }
    } else if (frame.t === 'diff') {
      store.applyServerField(frame.field, frame.value, /** @type {number} */ (frame.version));
    } else if (frame.t === 'ack') {
      settleAck(/** @type {Record<string, *>} */ (frame));
    }
  }

  /** @param {Record<string, *>} ack */
  function settleAck(ack) {
    const opId = /** @type {string} */ (ack.op_id);
    const outcome = /** @type {'applied'|'superseded'|'already_applied'|'rejected'|'forbidden'} */ (
      ack.outcome
    );
    const op = queue.peekAll().find((candidate) => candidate.op_id === opId) ?? null;
    if (outcome === 'applied' || outcome === 'already_applied') {
      // CAS committed exactly the op's value; the ack carries its version.
      store.ack(opId, outcome, ack.version ?? null, op ? op.value : null);
    } else {
      // No server value rides these acks — the store keeps its newest
      // server truth; the winner's diff (superseded) supplies the rest.
      store.ack(opId, outcome, null, null);
    }
    queue.dequeue(opId);
    emit({ type: 'queue', length: queue.length });
  }

  return {
    connect() {
      connection.connect();
    },

    /** Browser-offline and "hang up" share the connection's one path. */
    disconnect() {
      connection.notifyOffline();
    },

    /**
     * Queue a write for a versioned field. Never throws, never refuses.
     *
     * @param {Record<string, *>} target
     * @param {*} value
     */
    write(target, value) {
      const current = store.state()[targetKey(target)];
      const op = {
        op_id: idFactory(),
        target,
        base_version: current ? current.version : 0,
        value,
        created_at: new Date(now()).toISOString(),
      };
      queue.enqueue(op);
      store.enqueueView(op);
      connection.send({
        t: 'write',
        op_id: op.op_id,
        target,
        base_version: op.base_version,
        value,
      });
      emit({ type: 'queue', length: queue.length });
    },

    state() {
      return store.state();
    },

    /** The indicator's truth: the queue, nothing else. */
    isSyncing() {
      return queue.length > 0;
    },

    /** E10's cold-boot input: the merged state as a wire-shaped snapshot. */
    snapshotForBoot() {
      const fields = Object.values(store.state()).map((f) => ({
        field: f.target,
        value: f.value,
        version: f.version,
      }));
      return JSON.stringify({ fields });
    },

    /** Debug/page views: the durable queue, oldest first. */
    queue() {
      return queue.peekAll();
    },

    connectionState() {
      return connection.state();
    },

    /** @param {(event: Record<string, *>) => void} cb */
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
