// Connection — socket lifecycle, jittered backoff, watchdog, reconnect
// sequence (E7, design.md FR-10 + degraded-mode contract).
//
// Budgets are the spec's, not tunables: backoff delay = rng() * min(30000,
// 1000 * 2**attempt) (full jitter), watchdog fires after 50 s of inbound
// silence (server pings every 20 s, so a live link never trips it).
//
// One path rules: browser-offline (notifyOffline), socket error, and socket
// close all land in the same disconnect handler → state `offline` → schedule
// a reconnect. `offline` covers unreachable and browser-offline identically.
//
// Reconnect sequence on every (re)connect: the snapshot frame is delivered
// as a `frame` event, then the phase markers `snapshot → merge → drain →
// live` fire in order. A `drain` subscriber may return a thenable (the queue
// replay); `live` — and the backoff reset to attempt 0 — waits for it, so a
// link that dies mid-replay earns its doubled backoff and resets only after
// a full cycle. The queue must drain only after the snapshot marker; this
// ordering is the contract index.js composes against.
//
// The server owns liveness pings (20 s); this side answers `{t:"ping"}` with
// `{t:"pong"}` and counts every inbound frame as a heartbeat.

/**
 * The slice of the browser WebSocket surface this module drives; tests hand
 * a mock with the same shape.
 *
 * @typedef {Object} SyncSocket
 * @property {(data: string) => void} send
 * @property {() => void} close
 * @property {null | (() => void)} onopen
 * @property {null | ((event: { data: string }) => void)} onmessage
 * @property {null | (() => void)} onclose
 * @property {null | ((event: object) => void)} onerror
 */

const BASE_BACKOFF_MS = 1000;
const MAX_BACKOFF_MS = 30000;
const WATCHDOG_MS = 50000;

/**
 * @param {{
 *   url: string,
 *   socketFactory: (url: string) => SyncSocket,
 *   rng: () => number,
 *   now: () => number,
 *   timers: { setTimeout: (fn: () => void, ms: number) => *, clearTimeout: (id: *) => void },
 * }} options
 */
export function createConnection(options) {
  const { url, socketFactory, rng, now, timers } = options;

  /** @type {'connecting'|'live'|'offline'} */
  let currentState = 'offline';
  /** @type {SyncSocket | null} */
  let socket = null;
  let attempt = 0;
  /** @type {* | null} */
  let reconnectTimer = null;
  /** @type {* | null} */
  let watchdogTimer = null;
  let lastInbound = now();
  /** @type {Set<(event: object) => void>} */
  const listeners = new Set();

  /** @param {Record<string, *>} event */
  function emit(event) {
    for (const cb of listeners) cb(event);
  }

  /** @param {'connecting'|'live'|'offline'} next */
  function setState(next) {
    currentState = next;
    emit({ type: 'state', state: next });
  }

  function clearWatchdog() {
    if (watchdogTimer !== null) {
      timers.clearTimeout(watchdogTimer);
      watchdogTimer = null;
    }
  }

  function armWatchdog() {
    clearWatchdog();
    watchdogTimer = timers.setTimeout(() => {
      watchdogTimer = null;
      if (socket !== null) handleDisconnect(socket, 'watchdog-silence');
    }, WATCHDOG_MS);
  }

  /**
   * The one disconnect path — socket error, close, watchdog, and
   * browser-offline all arrive here (guarded so only the current socket
   * counts and each death schedules exactly one reconnect).
   *
   * @param {SyncSocket} dying
   * @param {string} reason
   */
  function handleDisconnect(dying, reason) {
    if (socket !== dying) return;
    socket = null;
    clearWatchdog();
    try {
      dying.close(); // a dead link is closed, not left dangling (watchdog path)
    } catch {
      // already gone — the browser may have closed it under us
    }
    const delay = rng() * Math.min(MAX_BACKOFF_MS, BASE_BACKOFF_MS * 2 ** attempt);
    attempt += 1;
    setState('offline');
    reconnectTimer = timers.setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
    emit({ type: 'reconnect', reason, delay });
  }

  function connect() {
    if (socket !== null) return;
    setState('connecting');
    /** @type {SyncSocket} */
    const mySocket = socketFactory(url);
    socket = mySocket;
    lastInbound = now();
    mySocket.onopen = () => {
      if (socket === mySocket) armWatchdog();
    };
    mySocket.onmessage = (event) => handleFrame(mySocket, event.data);
    mySocket.onclose = () => handleDisconnect(mySocket, 'closed');
    mySocket.onerror = () => handleDisconnect(mySocket, 'error');
  }

  /** @param {SyncSocket} mySocket @param {string} data */
  function handleFrame(mySocket, data) {
    if (socket !== mySocket) return;
    lastInbound = now();
    armWatchdog();

    /** @type {Record<string, *> | null} */
    let frame = null;
    try {
      frame = JSON.parse(data);
    } catch {
      emit({ type: 'malformed', data });
      return;
    }
    if (frame !== null && frame.t === 'ping') {
      mySocket.send(JSON.stringify({ t: 'pong' }));
      return;
    }
    emit({ type: 'frame', frame });

    if (frame !== null && frame.t === 'snapshot') {
      emit({ type: 'phase', phase: 'snapshot' });
      emit({ type: 'phase', phase: 'merge' });
      // The drain phase: a subscriber replaying the queue may return a
      // thenable; live (and the attempt reset) waits for it.
      const results = [...listeners].map((cb) => cb({ type: 'phase', phase: 'drain' }));
      const thenables = results.filter(
        (r) => r !== null && r !== undefined && typeof (/** @type {*} */ (r).then) === 'function',
      );
      const settle = () => {
        if (socket === mySocket) goLive();
      };
      if (thenables.length === 0) {
        settle();
      } else {
        Promise.all(thenables).then(settle, settle);
      }
    }
  }

  function goLive() {
    if (socket === null || currentState === 'live') return; // died mid-drain, or already there
    attempt = 0;
    setState('live');
    emit({ type: 'phase', phase: 'live' });
  }

  return {
    connect,

    /**
     * Browser-offline enters here — the same path as a socket error.
     * No-op when already down or connecting.
     */
    notifyOffline() {
      if (socket !== null) handleDisconnect(socket, 'browser-offline');
    },

    /**
     * Sends only while live. False means "not sent" — the caller keeps the
     * op queued; the connection never queues.
     *
     * @param {object} frame
     * @returns {boolean}
     */
    send(frame) {
      if (currentState !== 'live' || socket === null) return false;
      socket.send(JSON.stringify(frame));
      return true;
    },

    state() {
      return currentState;
    },

    /** @param {(event: object) => void} cb */
    onChange(cb) {
      listeners.add(cb);
    },
  };
}
