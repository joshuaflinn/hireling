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

import { writable } from 'svelte/store';

import { encodeValue, targetKey } from '../sync/store.js';
// The shared contract pin, on the production path: a frame this module builds
// must satisfy the contract before it satisfies the socket. It lives in `src/`
// rather than under `tests/` because `web/tsconfig.json` includes only `src/**`
// and svelte-check's language service reads no config below `web/`, so a test
// helper that imports `node:fs` cannot be reached from here at all (its header
// carries the full reason). The fixture stays the source of truth: the equality
// between the literals here and `field-targets.json` is asserted in
// `tests/helpers/wire-contract.js`, on a production path.
import { assertWriteTarget, assertStashOp } from '../contract/wire-contract.js';

/**
 * One refused-op record: degraded-mode §4's "expose on the op record" — the
 * shape the screen renders inline, self-clearing.
 *
 * @typedef {{op: Record<string, unknown>, op_id: string, outcome: string, reason: string|null}} OpRecord
 *
 * The targets the party surface routes on — wire-protocol §3's field table,
 * restricted to what a stash screen folds. `field` is set on the party-settings
 * target only. Declared above every use: a JSDoc typedef is visible to the text
 * that FOLLOWS it, and `Projection` names this one, so declaring it lower in the
 * file leaves `Projection` unresolved and the screen's `$state(null)` slot
 * `never` — which reports as nine "property does not exist on type 'never'"
 * errors on lines that are correct.
 *
 * @typedef {{kind: string, party_id?: number, item_name?: string, field?: string}} StashTarget
 */

/** @param {number} partyId @param {string} itemName */
const stashTarget = (partyId, itemName) => ({ kind: 'stash', party_id: partyId, item_name: itemName });
/** @param {number} partyId */
const stashNewTarget = (partyId) => ({ kind: 'stash_new', party_id: partyId });
/** @param {number} partyId */
const bankTarget = (partyId) => ({ kind: 'bank', party_id: partyId });
/** @param {number} partyId */
const partyTarget = (partyId) => ({ kind: 'party', party_id: partyId, field: 'quartermaster' });


/**
 * @typedef {object} StashRow
 * @property {string} item_name
 * @property {number} quantity
 * @property {number|null} bulk
 * @property {string|null} notes
 * @property {number} version
 *
 * @typedef {object} BankBalance
 * @property {number} gp
 * @property {number} sp
 * @property {number} cp
 *
 * A sell price as the §3 frame carries it. `pp` is optional and is the only
 * denomination the bank does not hold: `stash_sell_tx` converts it at credit
 * time (1 pp = 10 gp, Player Core pg. 267 — the one conversion site, cited
 * there). A seat may offer it; the read route's book value never does (D5).
 *
 * @typedef {{pp?: number, gp?: number, sp?: number, cp?: number}} SellPrice
 *
 * One committed claim row, in the wire shape `read_claim_row` emits
 * (`src/sync/write.rs`). `id` rides the target, the rest rides the value;
 * coins are null on item rows and set only on the bank rows (FR-4).
 *
 * @typedef {object} ClaimRow
 * @property {number} id
 * @property {string} item_name
 * @property {number} quantity
 * @property {string} origin
 * @property {string} destination
 * @property {string} actor_sub
 * @property {string} created_at
 * @property {number|null} coins_gp
 * @property {number|null} coins_sp
 * @property {number|null} coins_cp
 *
 * The whole folded view: every accessor's return, in one place. Exported so the
 * screen can name the shape it mirrors into local state without restating any
 * of these fields — a restated shape drifts from the store the first time an
 * accessor changes, and `svelte-check` would only see the drift as `never`.
 *
 * @typedef {{
 *   rows: StashRow[],
 *   bank: BankBalance | null,
 *   claims: ClaimRow[],
 *   quartermasterId: number | null,
 *   quartermasterVersion: number,
 *   canWrite: boolean,
 *   canDesignate: boolean,
 *   canSell: boolean,
 *   canTransfer: boolean,
 *   notices: OpRecord[],
 * }} Projection
 *
 * @typedef {object} StashState
 * @property {() => StashRow[]} rows loot rows, name-sorted
 * @property {() => BankBalance | null} bank the party
 *   balance, or null when no movement has ever touched it (design D1: the row
 *   is not zero-minted, so "unknown" is the honest pre-state)
 * @property {() => number | null} quartermasterId
 * @property {() => number} quartermasterVersion the version this seat read, which
 *   the designation write CASes on — same rule as `editMeta`'s row version
 * @property {() => ClaimRow[]} claims claim rows, newest-first by row id
 * @property {() => OpRecord[]} opErrors
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
 * @property {(coins: BankBalance) => void} adjustBank
 * @property {(itemName: string, qty: number, direction: 'in' | 'out', characterId: number) => void} transfer
 * @property {(itemName: string, qty: number, price: SellPrice) => void} sell
 * @property {() => import('svelte/store').Readable<Projection>} changed fires
 *   when the projection moved — what the screen re-reads itself off
 * @property {() => Projection} snapshot the whole folded view in the flat shape
 *   the screen mirrors into local state. Lives HERE rather than in the component
 *   so the field set has one owner: a screen that restated it would drift the
 *   first time an accessor is added, and the drift would surface as `never`,
 *   not as an error on the missing field.
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

  // The refused-op records: degraded-mode §4, self-clearing (the sheet's own
  // `opErrors` auto-clear rule, carried over verbatim rather than re-invented).
  // Declared HERE, above the initial `project()` below, for the same reason the
  // cache is: `project()` now returns these as part of the projection, so a
  // declaration further down would be in that call's temporal dead zone — which
  // is not a lint, it is a ReferenceError at module evaluation and every test in
  // the party suite.
  /** @type {OpRecord[]} */
  let refused = [];
  // bumped by the refusal paths and NOT by a clear, so the fold cache cannot
  // serve a pre-refusal projection to a read that happens after one. Length
  // alone would go stale: see the cache comment in `current()`.
  let refusalSeq = 0;

  // Declared above the initial projection: `project()` runs on this line and
  // reads the cache and `refused`, so a later `const` or `let` would be in its
  // temporal dead zone.
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
  /** @param {*} a @param {*} b @returns {boolean} */
  function deepEqual(a, b) {
    if (a === b) return true;
    if (typeof a !== typeof b || a === null || b === null) return false;
    if (typeof a !== 'object') return false;
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k) => deepEqual(a[k], b[k]));
  }


  /** One state entry as `sync.state()` reports it, with the target narrowed to
   *  the kinds this party surface routes on (wire-protocol §3's table). */
  /** One state entry, as `sync.state()` reports it, with the target narrowed to
   *  the kinds this surface routes on (wire-protocol §3's field table). Declared
   *  literally: `FieldState` is a JSDoc typedef inside `createStore()` and is not
   *  reachable by `import()`, and `state()`'s own return type declares `target`
   *  as `Record<string, *>`, so the narrowing belongs at the join. */
  /** @typedef {{target: Record<string, *> & StashTarget, value: *, version: number, serverValue: *}} Entry */

  function readFields() {
    /** @type {Map<string, Entry>} */
    const server = new Map();

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
    /** The transport's state entry, narrowed to this surface's target kinds. */
    /** @param {{target: *, value: *, version: number, serverValue?: *}} e */
    const entryOf = (e) => /** @type {Entry} */ (e);
    /** @param {Record<string, *>} a @param {Record<string, *>} b */
    const sameTarget = (a, b) => {
      const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
      for (const k of keys) if (!deepEqual(a[k], b[k])) return false;
      return true;
    };
    // `mine` is a MULTI-map, and that is the whole defect this block used to
    // carry. It was keyed by target and filled with `set`, so a second pending op
    // on the same field overwrote the first: two offline draws on one row echoed
    // one delta, and the queue's FIFO order was reduced to "the last tap". §4
    // makes that shape the norm, not an edge — the queue is unbounded with no
    // coalescing — so the fold has to see every pending op, in order.
    /** @type {Map<string, Entry[]>} */
    const mine = new Map();
    /** @param {string} key @param {Entry} op */
    const pushMine = (key, op) => {
      const existing = mine.get(key);
      if (existing) existing.push(op);
      else mine.set(key, [op]);
    };
    /** Which state key (if any) this op's decoded target joins to. */
    /** @param {StashTarget} opTarget */
    const joinKey = (opTarget) => {
      const found = stateEntries.find(([, e]) => sameTarget(entryOf(e).target, opTarget));
      return found ? targetKey(entryOf(found[1]).target) : null;
    };
    // `targetKey` sorts keys (sync/store.js), so two ops on the same field share
    // a join key whether or not their literals were written in the same order —
    // which is what makes a list keyed by join key the same list §4 describes.
    /** @type {Map<string, Entry[]>} */
    const byJoin = new Map();
    for (const op of sync.queue?.() ?? []) {
      const opTarget = /** @type {StashTarget} */ (op.target);
      const entry = /** @type {Entry} */ (/** @type {unknown} */ (op));
      const key = joinKey(opTarget);
      // An op whose field the transport has never stated (a mint against a row
      // that does not exist yet) has no state key to join to. It is still mine,
      // so it is filed under its own encoding; nothing above can collide with it.
      const fileKey = key ?? targetKey(op.target);
      pushMine(fileKey, entry);
      if (key) {
        const bucket = byJoin.get(key);
        if (bucket) bucket.push(entry);
        else byJoin.set(key, [entry]);
      }
    }

    for (const [, raw] of stateEntries) {
      const entry = /** @type {Entry} */ (raw);
      const key = targetKey(entry.target);
      const pending = byJoin.get(key) ?? [];
      const op = pending[pending.length - 1] ?? null;
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
        server.set(key, { target: entry.target, value, version: entry.version, serverValue: value });
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
          serverValue: last?.value ?? null,
        });
      } else {
        // A committed statement, in the shape this field kind renders.
        lastCommitted.set(key, { value: shown, version: entry.version });
        server.set(key, { target: entry.target, value: shown, version: entry.version, serverValue: entry.serverValue });
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
  /** @param {BankBalance|null} base @param {{gp_delta?: number, sp_delta?: number, cp_delta?: number}} delta */
  function foldBalance(base, delta) {
    const from = base ?? { gp: 0, sp: 0, cp: 0 };
    return {
      gp: Math.max(0, from.gp + (delta.gp_delta ?? 0)),
      sp: Math.max(0, from.sp + (delta.sp_delta ?? 0)),
      cp: Math.max(0, from.cp + (delta.cp_delta ?? 0)),
    };
  }

  /** Is this value an additive op frame rather than a balance or a row? */
  /** @param {*} value */
  function isDelta(value) {
    return (
      value !== null && typeof value === 'object' && ('gp_delta' in value || 'qty_delta' in value)
    );
  }

  /** A rendered stash row, from either source. */
  /** @param {{item_name: string, quantity: number, bulk?: number|null, notes?: string|null}} value
   *  @param {number} version
   *  @returns {StashRow} */
  function rowOf(value, version) {
    return {
      item_name: value.item_name,
      quantity: value.quantity,
      bulk: value.bulk ?? null,
      notes: value.notes ?? null,
      version,
    };
  }

  /** @returns {Projection} */
  function project() {
    const { server, mine } = readFields();
    /** @type {StashRow[]} */
    const rows = [];
    /** @type {ClaimRow[]} */
    const claims = [];
    /** @type {BankBalance | null} */
    let committedBank = null;
    /** @type {BankBalance | null} */
    let bank = null;
    /** @type {number | null} */
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
        //
        // `== null` is not available here even though it is the idiomatic test
        // for "null or undefined": the gate image's eslint config enforces
        // `eqeqeq: ["error", "always"]`, which admits no `==` at all. The repo
        // replica (`web/gate-eslint.config.mjs`) does not carry that rule, so
        // `just ci-local` stays green over exactly this class — which is why the
        // two spellings below name both nullish cases explicitly instead.
        if (
          entry.version !== 0 &&
          !isDelta(entry.value) &&
          entry.value?.item_name !== null &&
          entry.value?.item_name !== undefined
        ) {
          rows.push(rowOf(/** @type {Omit<StashRow, 'version'>} */ (entry.value), entry.version));
        }
      } else if (target.kind === 'bank') {
        // Same `eqeqeq` constraint as the stash branch above — no `== null`.
        if (
          entry.version !== 0 &&
          !isDelta(entry.value) &&
          entry.value?.gp !== null &&
          entry.value?.gp !== undefined
        ) {
          /** @type {BankBalance & {gp?: number, sp?: number, cp?: number}} */
          const coins = /** @type {BankBalance} */ (entry.value);
          committedBank = { gp: coins.gp, sp: coins.sp, cp: coins.cp };
        }
      } else if (target.kind === 'party' && target.field === 'quartermaster') {
        if (entry.version === 0) continue;
        quartermasterId =
          /** @type {{quartermaster_character_id?: number} | null} */ (entry.value)?.quartermaster_character_id ??
          null;
        quartermasterVersion = entry.version;
      } else if (target.kind === 'claim') {
        // Merge-by-id is the store's strictly-newer rule already (the id
        // rides the version slot — contract §3). Skip a row the store never
        // accepted (version 0) and one whose body is absent; append-only
        // means there is nothing else to do but order it.
        if (entry.value === null || entry.version === 0) continue;
        claims.push(/** @type {ClaimRow} */ (entry.value));
      }
    }

    // Then the local echo: what this seat has asked for and the server has not
    // yet answered (degraded-mode §4 — "a write applies to view state
    // immediately, tagged `pending`"). Folded onto the committed value above,
    // never rendered as a raw delta frame.
    for (const group of mine.values()) {
      // One group = every pending op on one field, in queue order. The group's
      // first member stands in for target selection (they all share a target by
      // construction); the fold below then walks the whole group, not one entry.
      const op = group[0];
      if (!isDelta(op.value)) continue; // a CAS op's pending value is already shown
      const { target } = op;
      const pending = group;
      if (target.kind === 'bank') {
        // The delta is only an intent if there is a balance to put it on. The
        // wire never mints a bank row: wire-protocol §3 says the field is absent
        // until the first movement, and the server creates it inside that first
        // tx. So an offline withdrawal for a party with no stated balance has no
        // base to fold onto, and rendering `0 + delta` would show the treasury as
        // whatever the user last typed. The queue holds the intent, the server
        // applies it to whatever the balance turns out to be, and the screen
        // shows no balance -- which is what the server will show too, since its
        // own ceiling guard refuses the overdraw.
        // Fold in FIFO order rather than assigning, so a second pending
        // withdrawal lands on the first instead of replacing it (finding 2), and
        // stop at the first overdraw rather than flooring it (finding 3).
        //
        // Finding 3 is why the fold BREAKS rather than skipping: write.rs:1878
        // refuses an adjustment that would drive a denomination below zero and
        // refuses it WHOLE (FR-7), so the overdraw never lands and neither can
        // anything queued behind it. The server applies the queue in send order,
        // so once a delta is refused the rest of that field's queue has no base
        // to fold onto either — skipping ahead would show a balance the server
        // will never hold, which is the same fault the floor had, one delta
        // further down the list. `foldBalance`'s floor stays as the last line of
        // defence for a base already at or below zero.
        /** @type {BankBalance} */
        let foldBase = /** @type {BankBalance} */ (bank ?? committedBank);
        if (foldBase === null) continue;
        for (const pendingOp of pending) {
          if (!isDelta(pendingOp.value)) continue;
          const incoming = /** @type {{gp_delta?: number, sp_delta?: number, cp_delta?: number}} */ (pendingOp.value);
          if (
            foldBase.gp + (incoming.gp_delta ?? 0) < 0 ||
            foldBase.sp + (incoming.sp_delta ?? 0) < 0 ||
            foldBase.cp + (incoming.cp_delta ?? 0) < 0
          )
            break;
          foldBase = foldBalance(foldBase, incoming);
        }
        bank = foldBase;
      } else if (target.kind === 'stash') {
        // FR-1 deletes a row at exactly zero, so a draw that lands on zero is
        // shown as the row disappearing rather than as a 0-quantity row.
        const name = /** @type {string} */ (target.item_name);
        const shown = rows.find((row) => row.item_name === name);
        if (shown) {
          // Fold every pending draw on this row in queue order (finding 2), and
          // drop a draw the server would refuse rather than flooring at zero
          // (finding 3, same reasoning as the bank above).
          // Stop at the first overdraw — see the bank fold above for why a
          // refusal ends the field's queue rather than stepping over it.
          let quantity = shown.quantity;
          for (const pendingOp of pending) {
            const delta = /** @type {{qty_delta?: number}} */ (pendingOp.value).qty_delta ?? 0;
            if (quantity + delta < 0) break;
            quantity += delta;
          }
          if (quantity === 0) rows.splice(rows.indexOf(shown), 1);
          else shown.quantity = quantity;
          continue;
        }
        const next = pending.reduce(
          (/** @type {number} */ total, /** @type {Entry} */ entry) =>
            total + (/** @type {{qty_delta?: number}} */ (entry.value).qty_delta ?? 0),
          0,
        );
        if (next > 0) {
          // A draw against a row the server has never stated. It renders, tagged
          // pending, because §4 says so — and it is the row the user is looking
          // at while the link is down.
          rows.push(rowOf({ item_name: name, quantity: next }, 0));
        }
      }
    }

    // A mint is not a delta — it creates a row the server has never stated, so
    // it is in neither view until the fan-out. Shown as pending for the same
    // §4 reason; it carries no bulk or notes because only the server's diff does.
    for (const group of mine.values()) {
      const op = group[0];
      if (op.target.kind !== 'stash_new') continue;
      if (rows.some((row) => row.item_name === op.value.item_name)) continue;
      rows.push(rowOf(/** @type {Omit<StashRow, 'version' | 'bulk' | 'notes'> & Partial<Pick<StashRow, 'bulk' | 'notes'>>} */ (op.value), 0));
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
      // Not folded from the merged map, but part of the same view: what this
      // seat was refused (degraded-mode §4) and what the pin still has not
      // shipped. Both belong to the projection rather than to `snapshot()` so
      // the field set has ONE owner — a screen that restated it would drift the
      // first time a field is added, and `svelte-check` would report the drift
      // as `never` on lines that are correct.
      notices: refused,
    };
  }

  // One projection value per moved input, not one per read. `projection` is the
  // writable a component subscribes to, and svelte 5's `writable` calls its
  // listeners synchronously from `set` (store/shared/index.js). So a `set` that
  // happens *inside* a listener re-enters the notification already running.
  // `current()` used to `set` on every read, and the screen's listener reads
  // (StashScreen.svelte) -- read, set, re-notify, read, forever. That is the
  // busy spin: an unbounded re-entrancy that never returns to the event loop, so
  // jsdom's queue is never reached and no test timeout can ever fire.
  //
  // What breaks the cycle: a read that finds the inputs unmoved returns the
  // cached fold and writes NOTHING, so a listener may call any accessor from
  // inside its own notification. A read that finds them moved still recomputes,
  // so the never-stale guarantee on `current()` below is kept in full -- see
  // `ensureCurrent()` for how "moved" is proved. `refused` is part of the
  // projection too, but it moves only from this module's own event handler or a
  // user action, never during a notification, so a read of it cannot re-enter.
  /** @type {string | null} */
  let cacheWitness = null;
  /** @type {Projection} */
  let cached = project();

  /** The wire moved: recompute and notify. The only place this writable is
   *  written, which is what makes a listener's read safe. */
  function refresh() {
    // Fold directly, never through `current()`: the cache is keyed partly on the
    // field set, and a refusal moves no field at all, so a cached fold could be
    // served here with the notice missing.
    cached = project();
    cacheWitness = `${inputWitness(sync.state())}#${refusalSeq}`;
    projection.set(cached);
  }

  /** The current projection, read synchronously. Plain function accessors are
   *  the shape the screen calls (`{#if stash.canSell()}`) rather than derived
   *  stores: svelte 5's `derived` recomputes only while subscribed, so an
   *  unsubscribed derived store here would read stale after a fan-out — and
   *  FR-5 explicitly requires the sell affordance to appear/disappear on the
   *  fan-out. The one store this module exports is `changed`, which exists so
   *  a component can re-read when the wire moves.
   *
   *  It always reflects the latest state, and that is load-bearing. `project()` is a
   *  pure fold over the sync store's map with no I/O, and a cache updated only by
   *  the `fields`/`applied` events goes wrong whenever a value is delivered
   *  without one: the caller that hands `deliver()` straight to the store, a
   *  future event kind, a frame the transport accepts while this module's
   *  listener is not subscribed yet. A cache that only the wire events update did
   *  exactly that and read
   *  an EMPTY bank for a party the server had already stated a balance to — no
   *  error, no red test, just a blank number in front of a user on a live link,
   *  which is the worst shape a bug in a treasure tracker can take. The screen
   *  reads a handful of fields per render, so this costs a fold over the party's
   *  own fields and buys a read that cannot be stale by construction. */
  function current() {
    // The cached fold, recomputed only when the inputs moved -- so a read never
    // writes. That "never writes" is the whole reason the screen may read an
    // accessor from inside its `changed()` subscription: a `projection.set()`
    // here would re-enter the notification already running, and the
    // read-set-notify chain never returns to the event loop. A cached fold could
    // only ever be as fresh as the last event this module listens for, which
    // `current()`'s own comment records as the blank-balance defect, so the
    // cache is allowed only because it is keyed on the INPUT, not on an event:
    // every branch below either proves nothing moved or re-folds.
    // `refusalSeq` rides the witness because the refused-op records are part of
    // the projection: a record is appended by this module's own event handler
    // without any field moving, and a cache keyed on the wire alone would serve a
    // projection with the notice missing for every read after that refusal. It is
    // a monotone counter rather than `refused.length` because a clear does not
    // bump it — see `clearRefused()` for why that asymmetry is the invariant.
    const witness = `${inputWitness(sync.state())}#${refusalSeq}`;
    if (witness !== cacheWitness) {
      cacheWitness = witness;
      cached = project();
    }
    return cached;
  }

  /** @returns {string} a content fingerprint of the whole field set.
   *
   *  The transport's own `encodeValue` does the work (sync/store.js) -- the same
   *  canonical, key-sorted encoder whose output the `targetKey` join already
   *  depends on, so a value the projection reads cannot change without the
   *  fingerprint moving. Keying on the whole field set rather than on the
   *  projection's own output is deliberate: the projection is NOT a function of
   *  every field (an out-of-party field must not appear in these rows), so a key
   *  built from the folded result would go stale exactly where the fold is
   *  skipped. A version-only key fails the same way on the frame the wire
   *  actually produces -- a peer's diff at a lower version, which
   *  `applyServerField` accepts as the owner's own committed version and which
   *  can carry a different value.
   *
   *  A key built over the WHOLE field set rather than field-by-field, and that
   *  is what makes a `full_resync` visible: a resync can drop rows while every
   *  surviving field keeps its version, so a per-field key would read "nothing
   *  moved" while the set shrinks. The whole-set string shrinks with it and reads
   *  as moved, which is the conservative direction. */
  /** @param {Record<string, {target: *, value: *, version: number, serverValue?: *}>} state
   *  @returns {string} */
  function inputWitness(state) {
    const fields = state ?? {};
    let out = '';
    for (const key of Object.keys(fields).sort()) {
      const entry = fields[key];
      out += `${key}|${encodeValue(entry.value)}|${entry.version}|${encodeValue(entry.serverValue)}|`;
    }
    return out;
  }

  // --- refused op records -------------------------------------------------
  // degraded-mode §4: self-clearing. "Clears on the next applied field" is
  // the sheet's own rule (`opErrors` auto-clear on the field's next applied
  // ack, sheet/state.js) carried over verbatim rather than re-invented. The
  // declaration itself lives above the initial `project()` — see the note there.


  /** @param {Record<string, unknown>} op @param {string} outcome
   *  @param {string|null} [reason] @param {string} [opId] */
  function expose(op, outcome, reason, opId) {
    // `op_id` rides the record separately because `sync/index.js` passes `op: null`
    // once the op has left the queue (an ack can arrive after the dequeue), and the
    // `?? {}` above then files the frame as `{}`. A screen that keyed its each-block
    // off `op.op_id` would key every such record on `undefined` — a duplicate key,
    // and Svelte silently drops one of the two rows.
    refused = [...refused, { op, op_id: opId ?? '', outcome, reason: reason ?? null }];
    refusalSeq += 1;
  }

  function clearRefused() {
    refused = [];
    // Deliberately does NOT bump `refusalSeq`. A clear only ever runs on an
    // event whose branch calls `refresh()` immediately, and `refresh()` folds
    // directly rather than through the cache — so the clear is published by that
    // fold, and bumping here would leave the cache permanently one refusal behind
    // (the append bumps, the clear does not, and the sum goes back down). The
    // asymmetry is the invariant: `refusalSeq` is monotone over refusals, and
    // every clear has a `refresh()` beside it.
  }

  const unsubscribe = sync.subscribe((event) => {
    if (event.type === 'fields' || event.type === 'derived') {
      refresh();
      // A field moved: whatever the refused op was arguing about has settled
      // server-side, so the inline record has done its job.
      clearRefused();
    } else if (event.type === 'op_exposed') {
      expose(event.op ?? {}, event.outcome, event.reason ?? null, event.op_id);
      // refresh() is the only place `projection` is ever written, and a refusal
      // has no second signal behind it: a `rejected`/`forbidden` ack emits `queue`
      // and `op_exposed` and never `fields` or `applied` (sync/index.js). Without
      // this call the record lands in the store and the screen never hears about
      // it — FR-11's "refused operations surface through the op record" reads as
      // shipped when it is not, and the reverted echo stays wrong on screen. The
      // `superseded` path gets away without it only because a field frame always
      // follows a supersede; a refusal is terminal.
      refresh();
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

  /**
   * One bank denomination to the integer the server requires, or `null` when the
   * value cannot be one. `parse_bank_adjustment` (write.rs:1856-1873) wants a
   * JSON NUMBER whose `as_i64` survives `i32::try_from`, so the two ways a
   * coercion can lie — a fraction, and a magnitude outside i32 — are refusals
   * rather than something to post anyway. A blank is the integer 0, not an
   * omitted key: that same validator refuses an absent denomination outright.
   * @param {*} value
   * @returns {number|null}
   */
  function encodeCoin(value) {
    if (value === '' || value === null || value === undefined) return 0;
    const amount = Number(value);
    if (!Number.isInteger(amount)) return null;
    if (amount < -2147483648 || amount > 2147483647) return null;
    return amount;
  }

  // --- write actions ------------------------------------------------------

  /** @param {Record<string, *>} target @param {*} value @param {number} [baseVersion] */
  function post(target, value, baseVersion) {
    // The same pin the contract suite runs its own frames through, now on the
    // production path: a frame this module builds must satisfy the fixture
    // before it satisfies the socket.
    assertWriteTarget(/** @type {{kind: string, field?: string}} */ (target));
    if (target.kind === 'stash') assertStashOp(/** @type {{op: string}} */ (value));
    sync.write(target, value, baseVersion);
  }

  return {
    sync,

    refresh,
    destroy: unsubscribe,

    /** The projection moved — the screen re-reads its own accessors off this. */
    changed: () => projection,

    /** The flat view the screen renders. Every field is a plain accessor call,
     *  so this is the accessors — not a second copy of the fold. */
    snapshot() {
      return current();
    },

    rows: () => current().rows,
    bank: () => current().bank,
    claims: () => current().claims,
    quartermasterVersion: () => current().quartermasterVersion,
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
          notes: item.notes?.trim() || null,
        },
      );
    },

    /** Signed and additive (FR-1/FR-7): a delta, never a read-modify-write. */
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
      // The blank-vs-number rule is the server's, and §4 only fixes what a blank
      // MEANS (no adjustment) — it says nothing about what the input may be.
      // write.rs's `parse_bank_adjustment` is the boundary this frame is built to
      // aim at, and it is stricter than a coercion: each of the three must be
      // PRESENT, a JSON NUMBER, and an integer >= -2147483648 (`as_i64` +
      // `i32::try_from`, write.rs:1856-1873), and an absent key is refused
      // outright. So the client encodes a blank as the integer 0 rather than
      // sending nothing, and refuses anything it cannot encode as an integer
      // rather than coercing it into one — `Number(x) | 0` did exactly that, so
      // 2.7 posted 2 (a rounded coin count) and 1e10 posted 1410065408 (a signed
      // 32-bit wrap on a party's whole bank). `''`/`null`/`undefined` are the
      // blanks and become 0; the screen's number inputs can hand back a string
      // integer, which is a real number and stays.
      /** @type {(number | string | null | undefined)[]} */
      /** @type {Record<'gp' | 'sp' | 'cp', *>} */
      const raw = { gp: coins?.gp, sp: coins?.sp, cp: coins?.cp };
      /** @type {Record<'gp' | 'sp' | 'cp', number>} */
      const encoded = /** @type {Record<'gp' | 'sp' | 'cp', number>} */ ({});
      for (const coin of /** @type {const} */ (['gp', 'sp', 'cp'])) {
        const amount = encodeCoin(raw[coin]);
        if (amount === null) return;
        encoded[coin] = amount;
      }
      const { gp, sp, cp } = encoded;
      // A no-op adjustment is not sent. FR-7 requires an adjustment that would
      // drive a denomination below zero to be refused WHOLE (write.rs:1878) and
      // is silent on the all-zero case, so the server would take a zero-frame as
      // a zero-delta APPLIED and fan a version bump for a field that never
      // moved. `addRow` refuses its no-op inputs the same way.
      if (gp === 0 && sp === 0 && cp === 0) return;
      post(bankTarget(partyId), { gp_delta: gp, sp_delta: sp, cp_delta: cp });
    },

    /** FR-3 / FR-6: the compound ops. Both frames are built to the shapes the
     *  write path's bounds validator accepts (`stash_transfer_tx` /
     *  `stash_sell_tx`, and the pinned payloads in
     *  `src/tests/sync/contract.rs`), and both are gated on the pin so a frame
     *  the fixture does not list never reaches the socket. */
    transfer(itemName, qty, direction, characterId) {
      const quantity = Number(qty);
      const character = Number(characterId);
      if (!Number.isInteger(quantity) || quantity < 1) return;
      if (!Number.isInteger(character)) return;
      if (direction !== 'in' && direction !== 'out') return;
      post(stashTarget(partyId, itemName), {
        op: 'transfer',
        direction,
        qty: quantity,
        character_id: character,
      });
    },
    sell(itemName, qty, price) {
      const quantity = Number(qty);
      if (!Number.isInteger(quantity) || quantity < 1) return;
      // §3: `{pp?, gp?, sp?, cp?}`, every present denomination a non-negative
      // integer, at least one positive. The server re-validates; invalid input
      // never leaves this layer (the module's own write rule, D6).
      /** @type {SellPrice} */
      const coins = {};
      /** @type {{op: string, qty: number, price: SellPrice}} */
      const frame = { op: 'sell', qty: quantity, price: coins };
      let positive = false;
      for (const coin of /** @type {const} */ (['pp', 'gp', 'sp', 'cp'])) {
        const amount = Number(price?.[coin] ?? 0);
        if (!Number.isInteger(amount) || amount < 0) return;
        if (amount > 0) positive = true;
        if (amount > 0) coins[coin] = amount;
      }
      if (!positive) return;
      post(stashTarget(partyId, itemName), frame);
    },
  };
}
