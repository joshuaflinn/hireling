// The stash state layer (E12 design D6) — the party-side sibling of
// `sheet/state.js` and `party/state.js`. One sync, one fields map: the stash
// reads the SAME merged state the roster cards and the sheets read, so a loot
// number can never drift from the diff that moved it.
//
// Read path: every field the screen shows is already merged by the sync
// store's strictly-newer rule. This module does not re-merge — it projects the
// merged map into the shapes the screen renders (rows, bank, designation,
// claim log). That is the whole reason it is a projection and not a store of
// its own: a second merge here would be a second truth.
//
// Write path: the same rule as the sheet's — client-side bounds validation
// first (invalid input never leaves this layer; the server re-validates
// anyway), then `sync.write`. Queueing, retry, and replay are inherited whole
// from the sync module. There is deliberately NO second queue (design D6).
//
// Refused ops: `rejected`/`forbidden` are user-input facts, not sync events
// (degraded-mode §4), so they surface as self-clearing op records for inline
// rendering — never a modal, never a banner that blocks the screen.
//
// The claim log is the one read-only surface: it renders server-committed rows
// only (design D6 rejects optimistic local claim rows), merged by row id. The
// wire puts the row id in the version slot precisely so the store's ordinary
// strictly-newer rule IS the merge-by-id rule (contract §3), so this module
// adds no id special case — it sorts.

import { writable, get } from 'svelte/store';

import { targetKey } from '../sync/store.js';
// The shared contract pin. It lives under tests/ because it is a gate, not
// runtime code — but the production path runs through it too now, which is
// the point: a frame must satisfy the fixture before it satisfies the socket.
// `vite build` tree-shakes it out of the client bundle.
import { assertWriteTarget, assertStashOp } from '../../../tests/helpers/wire-contract.js';

/** @param {number} partyId @param {string} itemName */
const stashTarget = (partyId, itemName) => ({ kind: 'stash', party_id: partyId, item_name: itemName });
/** @param {number} partyId */
const stashNewTarget = (partyId) => ({ kind: 'stash_new', party_id: partyId });
/** @param {number} partyId */
const bankTarget = (partyId) => ({ kind: 'bank', party_id: partyId });
/** @param {number} partyId */
const partyTarget = (partyId) => ({ kind: 'party', party_id: partyId, field: 'quartermaster' });

/**
 * Ops that `wire-protocol.md` §3 names but lane L3 has not wired yet. The
 * contract says "lane L3 adds `transfer`/`sell` to the accepted set", and
 * `field-targets.json`'s `stash_ops` is the machine-readable half of that
 * sentence — so this list is DERIVED from the pin rather than typed in by
 * hand. When L3 merges, the fixture gains the op and the dialog unblocks
 * itself; nothing here needs editing, and nothing here can drift from the
 * contract it is describing.
 */
const UNWIRED = ['transfer', 'sell'];

/**
 * @typedef {object} StashRow
 * @property {string} item_name
 * @property {number} quantity
 * @property {number|null} bulk
 * @property {string|null} notes
 * @property {number} version
 *
 * @typedef {object} StashState
 * @property {() => StashRow[]} rows loot rows, name-sorted
 * @property {() => {gp: number, sp: number, cp: number} | null} bank the party
 *   balance, or null when no movement has ever touched it (design D1: the row
 *   is not zero-minted, so "unknown" is the honest pre-state)
 * @property {() => number | null} quartermasterId
 * @property {() => Array<object>} claims claim rows, newest-first by row id
 * @property {() => Array<{op: object, outcome: string, reason: string|null}>} opErrors
 *   self-clearing refused-op records (degraded-mode §4)
 * @property {() => boolean} canWrite any write affordance at all for this seat
 * @property {() => boolean} canDesignate the quartermaster flip (FR-5: any owner)
 * @property {() => boolean} canSell QM's owner only (FR-6)
 * @property {() => boolean} canTransfer open to every roster owner (FR-3)
 * @property {(item: {item_name: string, quantity: number, bulk?: number|null,
 *   notes?: string|null}) => void} addRow
 * @property {(itemName: string, qtyDelta: number) => void} adjustQty
 * @property {(itemName: string, meta: {bulk?: number|null, notes?: string|null}) => void} editMeta
 * @property {(characterId: number|null) => void} setQuartermaster
 * @property {(coins: {gp: number, sp: number, cp: number}) => void} adjustBank
 * @property {(itemName: string, qty: number, characterId: number) => void} transfer
 * @property {(itemName: string, qty: number, coins: {gp: number, sp: number, cp: number}) => void} sell
 * @property {() => string[]} unwiredActions ops the pin says are not on the wire
 * @property {() => import('svelte/store').Readable<boolean>} changed fires the
 *   projection moved — what the screen re-reads itself off
 * @property {() => void} refresh re-project the merged map (event-driven; the
 *   screen never calls this, the sync subscription and tests do)
 * @property {() => void} destroy
 */

/**
 * @param {{
 *   sync: import('../sync/index.js').Sync,
 *   partyId: number,
 *   you?: {sub: string, role: string} | null,
 *   roster?: Array<{character: {id: number, name: string, owner: string}}>,
 * }} setup
 * @returns {StashState & {sync: import('../sync/index.js').Sync}}
 */
export function createStashState({ sync, partyId, you = null, roster = [] }) {
  const isGm = you?.role === 'gm';
  const mySub = you?.sub ?? null;

  /** Roster owner lookup: character id → owner sub. FR-6's sell gate and
   *  FR-5's designation both key off "do I own the designated character",
   *  which is a roster fact, not a wire field. */
  /** @type {Map<number, string>} */
  const owners = new Map();
  for (const entry of roster) {
    owners.set(entry.character.id, entry.character.owner);
  }

  /**
   * The last value this module could UNDERSTAND for each field, remembered.
   *
   * Needed because the transport's record for an additive field is not always a
   * value it can render. On an `applied` ack the ack handler passes the op's own
   * frame as the committed value (sync/index.js:191) — which is true for a CAS
   * field and meaningless for an additive one, where the resulting balance rides
   * the next diff. Without this, a field that stated a balance at version N would
   * read as no balance at all between that ack and its diff, and the header would
   * blank on a race the design explicitly allows (wire-protocol §4: the diff may
   * land "before or after" the ack). This cache is what makes the screen hold the
   * last real number across that window — and it is dropped in `reset()`, so a
   * re-read of the world never shows a remembered value from before a
   * `full_resync`.
   *
   * @type {Map<string, {value: *, version: number}>}
   */
  const lastCommitted = new Map();

  // Declared above the initial projection: `project()` runs on this line and
  // reads the cache, so a later `const` would be in its temporal dead zone.
  const projection = writable(project());

  /** The one place the merged map is read. Pure: no store writes, so a test
   *  can call it as often as it likes. */
  /**
   * The transport's fields, split into two views: what the SERVER last stated,
   * and what the CLIENT has outstanding.
   *
   * This split is the whole reason the projection cannot read `sync.state()`
   * straight. `state()` renders one value per field — its `effectiveValue`
   * returns the pending value when a write is in flight (sync/store.js), which
   * is right for a CAS field, where the pending value is the value the server
   * will hold. On the ADDITIVE class it is a DELTA (`{gp_delta:…}`,
   * `{op:'qty', qty_delta}`), so the field reads as the user's intent and not as
   * a balance or a quantity at all — and the committed base is nowhere in that
   * map, because the pending value REPLACED it rather than sitting beside it.
   * Folding a local echo then has nothing to fold onto, which is exactly how the
   * blank-balance bug got in.
   *
   * So both halves are rebuilt here from the transport's own outputs: `state()`
   * for whatever is showing, `queue()` (the durable queue's FIFO view, the ops
   * not yet acked) for what is mine. The sheet lane reaches the same place by a
   * different road — `sheet/state.js` keeps its own `fields` mirror fed by
   * snapshot/diff frames so server values are never clobbered by an echo, and
   * pairs it with a `pendingKeys` set. This module takes the same shape off the
   * public surface instead of a second mirror, because the stash's server state
   * arrives on the party socket the session already reads.
   */
  /** Order-insensitive deep compare — the two targets' key ORDER is irrelevant. */
  function deepEqual(a, b) {
    if (a === b) return true;
    if (typeof a !== typeof b || a === null || b === null) return false;
    if (typeof a !== 'object') return false;
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k) => deepEqual(a[k], b[k]));
  }


  function readFields() {
    /** @type {Map<string, {target: object, value: *, version: number}>} */
    const server = new Map();
    /** @type {Map<string, object>} */
    const mine = new Map();

    // The join below is by STRING, and getting that string from one side only is
    // what took two rounds to find. `sync.state()` returns a plain object whose
    // keys are `targetKey(...)` encodings, and `targetKey` SORTS an object's keys
    // (sync/store.js). A target literal written in another property order —
    // `stashTarget()`'s `{kind, party_id, item_name}` against the `stashRow`
    // frame's `{item_name, kind, party_id}` — therefore encodes DIFFERENTLY, and
    // both strings are legitimate keys for the same field. So a queue op keyed by
    // `targetKey(op.target)` can miss the very field it is pending on, and the
    // miss is silent: the local echo is filed as "not mine" and dropped.
    //
    // The fix is to let the transport's own keys be the only source of identity.
    // Each queued op is matched to a state key by comparing the DECODED targets
    // (order-insensitive), not their encodings, so a pending op is found whether
    // or not its literal happens to be written in sorted order.
    const stateEntries = Object.entries(sync.state());
    /** @param {object} a @param {object} b */
    const sameTarget = (a, b) => {
      const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
      for (const k of keys) if (!deepEqual(a[k], b[k])) return false;
      return true;
    };
    for (const op of sync.queue?.() ?? []) {
      const [, entry] = stateEntries.find(([, e]) => sameTarget(e.target, op.target));
      if (entry) mine.set(targetKey(entry.target), op);
      // An op whose field the transport has never stated (a mint against a row
      // that does not exist yet) has no state key to join to. It is still mine,
      // so it is filed under its own encoding; nothing above can collide with it.
      else mine.set(targetKey(op.target), op);
    }

    for (const [, entry] of stateEntries) {
      const key = targetKey(entry.target);
      const op = mine.get(key);
      // The committed value comes from `entry.serverValue` — the transport's own
      // record of what the server last stated, which survives the echo rather
      // than being overwritten by it (sync/store.js). Falling back to `entry.value`
      // when the field is not echoing is redundant today and left out on purpose:
      // `serverValue` IS the server's statement in both cases, and reading one
      // field instead of two is one fewer way to read the wrong one.
      //
      // `version` stays the field's server version either way: it is the
      // strictly-newer gate's own number and says nothing about who is showing.
      const shown = entry.value;
      const value = entry.serverValue;
      // A field the server has never stated carries version 0 and a null value;
      // both are "nothing yet" and the callers test for that on `version`.
      if (op && isDelta(op.value)) {
        // Pending additive echo: the committed base and my intent, kept apart.
        server.set(key, { target: entry.target, value, version: entry.version });
        mine.set(key, op);
      } else if (isDelta(value) || isDelta(shown)) {
        // The field's last statement is a DELTA, so it is not a balance and not
        // a row, and the caller must not render it as one. Two ways here, both
        // real, both ending the same way:
        //
        //  - No op is pending and a delta sits on the field. An `applied` ack
        //    settled the version while the value frame stayed — protocol.rs's
        //    ack frame carries `{t, op_id, outcome, version}` and no value.
        //  - The delta is in `serverValue` because the ack handler passed the
        //    OP's value as the committed one (`sync/index.js:191`,
        //    `op ? op.value : null`). Right for the CAS class, where the ack
        //    means "exactly this committed"; for the ADDITIVE class the ack only
        //    says "your delta was applied, it is version N" — the resulting
        //    balance is not on that frame, and the `stash`/`bank` diff that
        //    follows carries it.
        //
        // So the field is reported as still stating its LAST understandable
        // value: the transport has moved on without saying anything renderable,
        // and a header that blanks for one frame is the bug, not the race.
        const last = lastCommitted.get(key);
        server.set(key, {
          target: entry.target,
          value: last?.value ?? null,
          version: last?.version ?? 0,
        });
      } else {
        // A committed statement, in the shape this field kind renders.
        lastCommitted.set(key, { value: shown, version: entry.version });
        server.set(key, { target: entry.target, value: shown, version: entry.version });
      }
    }
    return { server, mine };
  }

  /**
   * Fold one additive delta onto a committed base, floored where the server
   * floors. `sheet/state.js`'s `Math.max(0, base + delta)` is the precedent for a
   * row; the bank floors per denomination because §4 refuses an overdraw rather
   * than posting a debt, and the screen must not show -7 gp the server will never
   * accept.
   */
  function foldBalance(base, delta) {
    const from = base ?? { gp: 0, sp: 0, cp: 0 };
    return {
      gp: Math.max(0, from.gp + (delta.gp_delta ?? 0)),
      sp: Math.max(0, from.sp + (delta.sp_delta ?? 0)),
      cp: Math.max(0, from.cp + (delta.cp_delta ?? 0)),
    };
  }

  /** Is this value an additive op frame rather than a balance or a row? */
  function isDelta(value) {
    return (
      value !== null && typeof value === 'object' && ('gp_delta' in value || 'qty_delta' in value)
    );
  }

  /** A rendered stash row, from either source. */
  function rowOf(value, version) {
    return {
      item_name: value.item_name,
      quantity: value.quantity,
      bulk: value.bulk ?? null,
      notes: value.notes ?? null,
      version,
    };
  }

  function project() {
    const { server, mine } = readFields();
    /** @type {Array<object>} */
    const rows = [];
    /** @type {Array<object>} */
    const claims = [];
    let committedBank = null;
    let bank = null;
    let quartermasterId = null;
    let quartermasterVersion = 0;

    for (const entry of server.values()) {
      const { target } = entry;
      if (target.kind === 'stash') {
        // Committed row first: a real row at a real version, shaped like a row.
        // A removal tombstone (contract §4: `value: null` at a newer version)
        // arrives as a null value, and FR-1's delete-at-zero means the row is
        // GONE — never a row at quantity 0, and never a row minted by a
        // tombstone for a name we never had.
        if (entry.version !== 0 && !isDelta(entry.value) && entry.value?.item_name != null) {
          rows.push(rowOf(entry.value, entry.version));
        }
      } else if (target.kind === 'bank') {
        if (entry.version !== 0 && !isDelta(entry.value) && entry.value?.gp != null) {
          committedBank = { gp: entry.value.gp, sp: entry.value.sp, cp: entry.value.cp };
        }
      } else if (target.kind === 'party' && target.field === 'quartermaster') {
        if (entry.version === 0) continue;
        quartermasterId = entry.value?.quartermaster_character_id ?? null;
        quartermasterVersion = entry.version;
      } else if (target.kind === 'claim') {
        // Merge-by-id is the store's strictly-newer rule already (the id
        // rides the version slot — contract §3). Skip a row the store never
        // accepted (version 0) and one whose body is absent; append-only
        // means there is nothing else to do but order it.
        if (entry.value === null || entry.version === 0) continue;
        claims.push(entry.value);
      }
    }

    // Then the local echo: what this seat has asked for and the server has not
    // yet answered (degraded-mode §4 — "a write applies to view state
    // immediately, tagged `pending`"). Folded onto the committed value above,
    // never rendered as a raw delta frame.
    for (const op of false ? [] : mine.values()) {
      const { target } = op;
      if (!isDelta(op.value)) continue; // a CAS op's pending value is already shown
      if (target.kind === 'bank') {
        bank = foldBalance(committedBank, op.value);
      } else if (target.kind === 'stash') {
        // FR-1 deletes a row at exactly zero, so a draw that lands on zero is
        // shown as the row disappearing rather than as a 0-quantity row.
        const shown = rows.find((row) => row.item_name === target.item_name);
        const next = Math.max(0, (shown?.quantity ?? 0) + (op.value.qty_delta ?? 0));
        if (shown) {
          if (next === 0) rows.splice(rows.indexOf(shown), 1);
          else shown.quantity = next;
        } else if (next > 0) {
          // A draw against a row the server has never stated. It renders, tagged
          // pending, because §4 says so — and it is the row the user is looking
          // at while the link is down.
          rows.push(rowOf({ item_name: target.item_name, quantity: next }, 0));
        }
      }
    }

    // A mint is not a delta — it creates a row the server has never stated, so
    // it is in neither view until the fan-out. Shown as pending for the same
    // §4 reason; it carries no bulk or notes because only the server's diff does.
    for (const op of mine.values()) {
      if (op.target.kind !== 'stash_new') continue;
      if (rows.some((row) => row.item_name === op.value.item_name)) continue;
      rows.push(rowOf(op.value, 0));
    }

    // With no draw in flight, the screen shows the server's balance and nothing
    // else. Assigned here rather than in the branch above so the committed read
    // and the folded read meet at one place — a balance the projection computed
    // and then forgot to show is how this field read `null` for a whole round of
    // tests while the fold itself was correct.
    if (bank === null) bank = committedBank;

    rows.sort((a, b) => a.item_name.localeCompare(b.item_name));
    claims.sort((a, b) => b.id - a.id); // newest-first, D7

    const designatedOwner = quartermasterId === null ? null : owners.get(quartermasterId) ?? null;

    return {
      rows,
      bank,
      claims,
      quartermasterId,
      quartermasterVersion,
      // FR-11/E14 SC-5: the GM renders zero write affordances — the socket
      // would refuse the frame anyway (`gm is read-only`), so a control that
      // only produced a rejected ack is a lie, not a disabled button.
      canWrite: !isGm,
      // FR-5's flip rule, read straight: party settings are writable by any
      // character owner, the designated owner included, designated or not.
      canDesignate: !isGm,
      // FR-6: only the designated character's owner sells. Unnamed
      // designation → nobody. GM → never (FR-5: "the GM never holds it").
      canSell: !isGm && mySub !== null && designatedOwner === mySub,
      // FR-3: transfers and claims are open to all roster owners.
      canTransfer: !isGm,
    };
  }

  function refresh() {
    projection.set(project());
  }

  /** The current projection, read synchronously. Plain function accessors are
   *  the shape the screen calls (`{#if stash.canSell()}`) rather than derived
   *  stores: svelte 5's `derived` recomputes only while subscribed, so an
   *  unsubscribed derived store here would read stale after a fan-out — and
   *  FR-5 explicitly requires the sell affordance to appear/disappear on the
   *  fan-out. The one store this module exports is `changed`, which exists so
   *  a component can re-read when the wire moves.
   *
   *  It re-projects on every read, and that is load-bearing. `project()` is a
   *  pure fold over the sync store's map with no I/O, and a cache updated only by
   *  the `fields`/`applied` events goes wrong whenever a value is delivered
   *  without one: the caller that hands `deliver()` straight to the store, a
   *  future event kind, a frame the transport accepts while this module's
   *  listener is not subscribed yet. The cached version did exactly that and read
   *  an EMPTY bank for a party the server had already stated a balance to — no
   *  error, no red test, just a blank number in front of a user on a live link,
   *  which is the worst shape a bug in a treasure tracker can take. The screen
   *  reads a handful of fields per render, so this costs a fold over the party's
   *  own fields and buys a read that cannot be stale by construction. */
  function current() {
    const next = project();
    projection.set(next);
    return next;
  }

  // --- refused op records -------------------------------------------------
  // degraded-mode §4: self-clearing. "Clears on the next applied field" is
  // the sheet's own rule (`opErrors` auto-clear on the field's next applied
  // ack, sheet/state.js) carried over verbatim rather than re-invented.
  /** @type {Array<{op: object, outcome: string, reason: string|null}>} */
  let refused = [];

  function expose(op, outcome, reason) {
    refused = [...refused, { op, outcome, reason: reason ?? null }];
  }

  function clearRefused() {
    refused = [];
  }

  const unsubscribe = sync.subscribe((event) => {
    if (event.type === 'fields' || event.type === 'derived') {
      refresh();
      // A field moved: whatever the refused op was arguing about has settled
      // server-side, so the inline record has done its job.
      clearRefused();
    } else if (event.type === 'op_exposed') {
      expose(event.op ?? {}, event.outcome, event.reason ?? null);
    } else if (event.type === 'applied') {
      // Re-project on the ack too, not only on the field. §4 lets the own-write
      // diff land before or after the ack, and on the additive class the ack is
      // the frame that settles the field's version, so a projection that only
      // listened for `fields` would be one event behind whenever the diff
      // arrived first — the order the additive server actually produces, since it
      // commits the delta inside the row lock and fans the diff from the
      // transaction. Cheap either way: `current()` is a pure fold, no I/O.
      refresh();
      clearRefused();
    }
  });

  // --- write actions ------------------------------------------------------

  /** @param {Record<string, *>} target @param {*} value @param {number} [baseVersion] */
  function post(target, value, baseVersion) {
    // The same pin the contract suite runs its own frames through, now on the
    // production path: a frame this module builds must satisfy the fixture
    // before it satisfies the socket.
    assertWriteTarget(target);
    if (target.kind === 'stash') assertStashOp(/** @type {{op: string}} */ (value));
    sync.write(target, value, baseVersion);
  }

  /** Which §3 ops the pin still has unshipped. The screen renders these as a
   *  reason on the dialog, never as a button that silently does nothing. */
  function unwiredActions() {
    return UNWIRED.filter((name) => !isWired(name));
  }

  function isWired(name) {
    // Derived from the fixture via the helper's own gate: if the pin accepts
    // the op literal, the op is on the wire.
    try {
      assertStashOp({ op: name });
      return true;
    } catch {
      return false;
    }
  }

  return {
    sync,

    refresh,
    destroy: unsubscribe,

    /** The projection moved — the screen re-reads its own accessors off this. */
    changed: () => projection,

    rows: () => current().rows,
    bank: () => current().bank,
    claims: () => current().claims,
    quartermasterId: () => current().quartermasterId,
    opErrors: () => refused,
    canWrite: () => current().canWrite,
    canDesignate: () => current().canDesignate,
    canSell: () => current().canSell,
    canTransfer: () => current().canTransfer,

    /** `stash_new` is the mint: no CAS, identity is the unique name (FR-1's
     *  duplicate rejection is the constraint, not a check we duplicate). */
    addRow(item) {
      const name = typeof item.item_name === 'string' ? item.item_name.trim() : '';
      const quantity = Number(item.quantity);
      if (!name || !Number.isInteger(quantity) || quantity < 1) return; // never reaches the wire
      post(
        stashNewTarget(partyId),
        {
          item_name: name,
          quantity,
          bulk: item.bulk ?? null,
          notes: item.notes?.trim() ? item.notes.trim() : null,
        },
      );
    },

    /** Additive (§4): no version predicate, no CAS, and `superseded` is not
     *  in this op's outcome vocabulary — a lost race is a refused draw, not a
     *  silently dropped delta. */
    adjustQty(itemName, qtyDelta) {
      if (!itemName || !Number.isInteger(qtyDelta) || qtyDelta === 0) return;
      post(stashTarget(partyId, itemName), { op: 'qty', qty_delta: qtyDelta });
    },

    /** Absolute, CAS on the row version the seat actually read (D2). */
    editMeta(itemName, meta) {
      if (!itemName) return;
      const version = sync.state()[targetKey(stashTarget(partyId, itemName))]?.version ?? 0;
      post(
        stashTarget(partyId, itemName),
        { op: 'meta', bulk: meta.bulk ?? null, notes: meta.notes ?? null },
        version,
      );
    },

    setQuartermaster(characterId) {
      const id = characterId === null ? null : Number(characterId);
      if (id !== null && !Number.isInteger(id)) return;
      // CAS on the version this seat actually read — the same rule as meta.
      post(partyTarget(partyId), { character_id: id }, current().quartermasterVersion);
    },

    /** Signed and additive (FR-2.5): the client sends the DELTA, never the
     *  balance it wants. A read-modify-write here is exactly the bug D2 was
     *  written to refuse. */
    adjustBank(coins) {
      const gp = Number(coins.gp) | 0;
      const sp = Number(coins.sp) | 0;
      const cp = Number(coins.cp) | 0;
      if (gp === 0 && sp === 0 && cp === 0) return;
      post(bankTarget(partyId), { gp_delta: gp, sp_delta: sp, cp_delta: cp });
    },

    unwiredActions,

    // --- seat gating: predicate accessors over the derived view -----------
    canWrite: () => current().canWrite,
    canDesignate: () => current().canDesignate,
    canSell: () => current().canSell,
    canTransfer: () => current().canTransfer,
    bank: () => current().bank,
    quartermasterId: () => current().quartermasterId,
    claims: () => current().claims,
    opErrors: () => refused,

    /** FR-3 / FR-6: compound ops. `wire-protocol.md` §3 puts both in lane
     *  L3's hands, so today they refuse locally rather than queue a frame the
     *  server answers with `rejected`. `unwiredActions()` is what the dialog
     *  shows; when L3 merges the op into the fixture, these post for real. */
    transfer(itemName, qty, characterId) {
      if (!isWired('transfer')) return;
      post(stashTarget(partyId, itemName), { op: 'transfer', qty_delta: -qty, to_character_id: characterId });
    },
    sell(itemName, qty, coins) {
      if (!isWired('sell')) return;
      post(stashTarget(partyId, itemName), {
        op: 'sell',
        qty_delta: -qty,
        coins_gp: coins.gp,
        coins_sp: coins.sp,
        coins_cp: coins.cp,
      });
    },
  };
}
