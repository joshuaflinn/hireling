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
/** @typedef {Record<string, *>} EngineOutput — the E8 engine-output contract
 *   (specs/008/contracts/engine-output.md §3), verbatim from the wire. */

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
 * The composed sync instance — the public API this module returns (E7's
 * client surface; E6/E10 consume exactly these members).
 *
 * @typedef {Object} Sync
 * @property {() => void} connect
 * @property {() => void} disconnect browser-offline — the reconnect loop keeps running
 * @property {() => void} hangUp session over (logout) — the socket dies, no reconnect, ever
 * @property {(target: Record<string, *>, value: *) => void} write
 * @property {() => Record<string, {target: Record<string, *>, value: *, version: number}>} state
 * @property {(characterId: number) => EngineOutput | null} derived
 * @property {() => boolean} isSyncing
 * @property {() => string} snapshotForBoot
 * @property {() => Array<{op_id: string, target: Record<string, *>, base_version: number, value: *, created_at: string}>} queue
 * @property {() => 'connecting' | 'live' | 'offline'} connectionState
 * @property {(cb: (event: Record<string, *>) => void) => () => boolean} subscribe
 */

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
 *   bootSnapshot?: { fields: Array<{ field: Record<string, *>, value: *, version: number }> },
 * }} options
 * @returns {Sync}
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

  /**
   * One wire-shaped field set through the store's version merge — the
   * `snapshot` frame's body and E10's `bootSnapshot` seed share this one
   * path (no new merge semantics anywhere; the seed is just an old
   * snapshot that lost any race it deserved to lose).
   * @param {Array<{ field: Record<string, *>, value: *, version: number }>} fields
   */
  function applySnapshotFields(fields) {
    for (const f of fields) {
      store.applyServerField(f.field, f.value, /** @type {number} */ (f.version));
    }
  }

  // The cold-boot seed: last-known wire state from the boot cache, merged
  // by version exactly like a live snapshot (FR-9). Applied before any
  // socket exists, so the first paint already carries last-known numbers.
  applySnapshotFields(options.bootSnapshot?.fields ?? []);

  // E8's derived surface (design D4–D6): engine output is never versioned
  // and never stored durably — it is a pure function of already-versioned
  // state, refreshed by the snapshot's `derived` array and by `derived`
  // frames. A client that reloads cold loses nothing; an output that has
  // not arrived yet reads as null and the sheet shows its loading state.
  /** @type {Map<number, EngineOutput>} */
  const derivedOutputs = new Map();

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
      const fields = /** @type {Array<{ field: Record<string, *>, value: *, version: number }>} */ (
        frame.fields ?? []
      );
      applySnapshotFields(fields);
      // The catch-up snapshot carries every roster character's engine
      // output (design D6) — applied before the fields event so the
      // sheet's first paint after connect already has its numbers.
      for (const output of /** @type {EngineOutput[]} */ (frame.derived ?? [])) {
        derivedOutputs.set(output.character_id, output);
      }
      emit({ type: 'fields' });
    } else if (frame.t === 'diff') {
      store.applyServerField(frame.field, frame.value, /** @type {number} */ (frame.version));
      emit({ type: 'fields' });
    } else if (frame.t === 'derived') {
      // One character's recomputed engine output, fanned out after the
      // diff that caused it (D6's TCP-order rule).
      derivedOutputs.set(frame.character_id, frame.output);
      emit({ type: 'derived', character_id: frame.character_id });
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
    if (outcome === 'applied' || outcome === 'already_applied') {
      // E6's inline-error affordance: a win on a field clears any error
      // shown at that control.
      emit({ type: 'applied', key: op ? targetKey(op.target) : null });
    } else if (outcome === 'rejected' || outcome === 'forbidden') {
      // The store exposed the op (degraded-mode §4); E6 renders the
      // ack's reason inline at the control.
      emit({
        type: 'op_exposed',
        op_id: opId,
        outcome,
        op,
        reason: ack.reason ?? null,
      });
    }
  }

  return {
    connect() {
      connection.connect();
    },

    /** Browser-offline — the same reconnect loop as a socket error; the
     *  link is wanted back when the browser returns. */
    disconnect() {
      connection.notifyOffline();
    },

    /** The session is over (logout): the socket dies and no reconnect is
     *  ever scheduled again. NOT the same path as disconnect — that one
     *  keeps the loop alive on purpose. */
    hangUp() {
      connection.hangUp();
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

    /**
     * The engine output for one character, verbatim from the wire — null
     * until the snapshot or a `derived` frame delivers it. No client
     * computes (contract law); this is the sync surface's only derived
     * source.
     *
     * @param {number} characterId
     * @returns {EngineOutput | null}
     */
    derived(characterId) {
      return derivedOutputs.get(characterId) ?? null;
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
