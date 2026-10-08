// The party session (E10 design D2/D6): the shell's ONE roster fetch and
// ONE sync per logged-in tab. PartyView and every SheetView mount consume
// this instance — the roster and the sheets see literally the same store,
// so chip truth holds by construction, not by coincidence.
//
// Boot order: read the boot cache → seed the sync from it (always, when
// present) → fetch the roster → cache it. No branch asks WHY the link is
// down (E7's one-path rule): a fetch failure with a cache is an offline
// cold boot from last-known state; without a cache it is an honest throw.

/* global fetch, localStorage, window, JSON */

import { createSync, partySocketUrl } from '../sync/index.js';
import {
  readBootCache,
  writeBootCache,
  createThrottledWriter,
  requestPersistentStorage,
} from './boot.js';

/** The roster read failed and there is nothing cached to fall back on.
 *  `.cachedRoster` carries the fallback when one exists (the caller
 *  renders it read-only); it is null when there is nothing. */
export class RosterUnavailable extends Error {
  /**
   * @param {string} message
   * @param {*} cachedRoster
   */
  constructor(message, cachedRoster = null) {
    super(message);
    this.name = 'RosterUnavailable';
    this.cachedRoster = cachedRoster;
  }
}

/**
 * @typedef {Object} PartySession
 * @property {*} roster the roster payload (fresh, or the cached one cold-booting)
 * @property {ReturnType<typeof createSync>} sync the session sync — one per tab
 * @property {() => Promise<{roster: *, offlineColdBoot: boolean}>} refresh
 * @property {boolean} offlineColdBoot true when the roster came from the cache
 * @property {() => void} close flush pending boot-cache writes and drop the
 *   socket — logout and page hide
 */

/**
 * Open the party session for one account.
 *
 * @param {{
 *   fetchImpl?: typeof fetch,
 *   storage?: { getItem: (k: string) => string | null, setItem: (k: string, v: string) => void },
 *   account: { sub: string },
 *   socketFactory?: (url: string) => *,
 *   requestPersist?: (navigatorRef?: *) => Promise<boolean>,
 * }} options
 * @returns {Promise<PartySession>}
 */
export async function createPartySession(options) {
  const {
    fetchImpl = fetch,
    storage = localStorage,
    account,
    socketFactory,
    requestPersist = requestPersistentStorage,
  } = options;
  const sub = account.sub;

  const cached = readBootCache(storage, sub);

  /** The one sync (D2). With a cache the party id is known before any
   *  fetch answers — an offline cold boot gets its socket ambitions and
   *  its seeded store immediately; without one it waits for the roster. */
  /** @type {ReturnType<typeof createSync> | null} */
  let sync = null;
  /** @param {number} partyId */
  const makeSync = (partyId) => {
    const syncOptions = {
      url: partySocketUrl(partyId),
      storage,
      accountSub: sub,
      bootSnapshot: cached?.snapshot ?? undefined,
      ...(socketFactory ? { socketFactory } : {}),
    };
    return createSync(syncOptions);
  };
  if (cached?.roster) sync = makeSync(cached.roster.party_id);

  /** @type {any} */
  let roster;
  let offlineColdBoot = false;
  try {
    roster = await fetchRoster(fetchImpl);
  } catch (error) {
    if (!cached?.roster) {
      throw new RosterUnavailable(`The party roster is unreachable (${error}).`, null);
    }
    roster = cached.roster;
    offlineColdBoot = true;
  }
  if (!sync) sync = makeSync(roster.party_id);
  const sessionSync = sync; // non-null from here; the closure reads this

  // The persistent-storage grant (FR-9): asked once per account, result
  // logged, never blocking. The flag lives in the same storage the queue
  // and boot cache use — one per-sub namespace.
  const persistFlag = `hireling:persist-asked:${sub}`;
  if (!storage.getItem(persistFlag)) {
    storage.setItem(persistFlag, 'asked');
    requestPersist().catch(() => {}); // requestPersistentStorage never rejects; belt
  }

  // Cache the fresh roster now; throttle-follow every store merge so the
  // next cold boot is never staler than two seconds of merges.
  writeBootCache(storage, sub, roster, sessionSync.snapshotForBoot());
  const snapshotWriter = createThrottledWriter(
    /** @param {*} currentRoster @param {string} snapshotJson */
    (currentRoster, snapshotJson) => writeBootCache(storage, sub, currentRoster, snapshotJson),
    2000,
  );
  sessionSync.subscribe((event) => {
    if (event.type === 'fields' || event.type === 'queue' || event.type === 'derived') {
      snapshotWriter.call(roster, sessionSync.snapshotForBoot());
    }
  });

  /** Re-fetch the roster and rewrite the cache (import success calls
   *  this — the new card renders without a reload). Throws
   *  RosterUnavailable with the current roster when the wire fails. */
  async function refresh() {
    try {
      const fresh = await fetchRoster(fetchImpl);
      roster = fresh;
      writeBootCache(storage, sub, roster, sessionSync.snapshotForBoot());
      return { roster: fresh, offlineColdBoot: false };
    } catch (error) {
      throw new RosterUnavailable(`The roster refresh failed (${error}).`, roster);
    }
  }

  // The session owns its socket and its cache writes (D2/D6): `close()`
  // flushes the trailing throttle — the last ≤2 s of merges must not die
  // with the tab — then drops the link. Logout and page hide call it.
  return {
    roster,
    sync: sessionSync,
    refresh,
    offlineColdBoot,
    close() {
      snapshotWriter.flush();
      sessionSync.disconnect();
    },
  };
}

/** One roster read: GET /api/party/roster, the shape contracts/roster-rest.md
 *  pins. Non-2xx answers fail the same as a dead wire — the fallback path
 *  is one path, not per-status theatre.
 *
 * @param {typeof fetch} fetchImpl
 * @returns {Promise<*>}
 */
async function fetchRoster(fetchImpl) {
  const response = await fetchImpl('/api/party/roster');
  if (!response.ok) throw new Error(`the server answered ${response.status}`);
  return response.json();
}
