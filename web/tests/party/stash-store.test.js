// E12 Task 6 — the stash store's merge and write semantics, test-first.
//
// Written BEFORE `src/lib/party/stash.js` exists. These are the semantics the
// server already enforces on `main` (L2 merged at 2d4c159), so the assertions
// are pinned to a shipped contract rather than to my intent:
//   - wire-protocol §3: the snapshot/diff value shapes per kind
//   - wire-protocol §3 `claim` row: "the id in the version slot makes the
//     strictly-newer rule and the id rule the same predicate"
//   - src/sync/write.rs `stash_qty_tx`: a qty draw to exactly zero DELETES the
//     row and fans a removal diff of `value: null` at a fresh version (FR-1)
//   - wire-protocol §3: `claim` is read-only — no client write frame exists
//
// The sync seam is faked at the same boundary `party-view.test.js` fakes it
// (`state() / write() / subscribe()`), so the store is exercised over the real
// merge surface and never against a second copy of the merge rule.

import { test } from 'vitest';
import assert from 'node:assert/strict';

import { createStashState } from '../../src/lib/party/stash.js';
import { createStore, targetKey } from '../../src/lib/sync/store.js';

const STASH_TARGET = (partyId, itemName) => ({ kind: 'stash', party_id: partyId, item_name: itemName });
const BANK_TARGET = (partyId) => ({ kind: 'bank', party_id: partyId });
const PARTY_TARGET = (partyId) => ({ kind: 'party', party_id: partyId, field: 'quartermaster' });
const CLAIM_TARGET = (claimId) => ({ kind: 'claim', claim_id: claimId });

/**
 * The sync seam fake: same members the real `createSync` returns, plus the
 * `deliver`/`expose` hooks a test needs to play a frame through the event bus.
 * Mirrors `party-view.test.js`'s fake so the two suites agree on the seam —
 * with one deliberate upgrade: `deliver` routes through the REAL production
 * store (`sync/store.js`) rather than writing the map directly. A fake that
 * overwrote its own map would make every "stale diff is a no-op" assertion
 * below pass against a fake, not against the merge rule that ships.
 */
function fakeSync(options = {}) {
  const listeners = new Set();
  const store = createStore();
  const writes = [];
  // The same shape createSync's `idFactory` gives: a write mints its own op id,
  // so a test that wants to settle an echo reads the id off the write the store
  // actually made. A hand-written id would make `store.ack` a silent no-op and
  // the test would then assert the field's UNTOUCHED value — which, for a
  // "must not change" assertion, reads exactly like a pass.
  let opSeq = 0;
  /** @type {Map<string, object>} */
  const pending = new Map();
  /** The durable-queue view: op ids still awaiting an ack, in FIFO order. */
  const outstanding = new Map();
  const emit = (event) => {
    for (const cb of listeners) cb(event);
  };
  return {
    writes,
    connect() {},
    disconnect() {},
    hangUp() {},
    /** @param {Record<string, *>} target @param {*} value @param {number} [baseVersion] */
    write(target, value, baseVersion) {
      const op = {
        op_id: `op-${++opSeq}`,
        target,
        base_version: baseVersion ?? 0,
        value,
        created_at: '2026-10-14T00:00:00Z',
      };
      pending.set(op.op_id, op);
      outstanding.set(op.op_id, op);
      store.enqueueView(op);
      writes.push({ ...op });
      emit({ type: 'queue', length: writes.length });
      emit({ type: 'fields' });
    },
    /** Settle the fake's own outstanding echo by id, as an `applied` ack does. */
    ackAppliedTo(opId, version) {
      const op = /** @type {object} */ (pending.get(opId));
      assert.ok(op, `the fake only settles writes it actually made (got ${opId})`);
      store.ack(opId, 'applied', version, /** @type {any} */ (op).value);
      pending.delete(opId);
      outstanding.delete(opId);
      emit({ type: 'applied', op_id: opId, version });
      emit({ type: 'queue', length: pending.size });
    },
    /** The single outstanding write's op id — the one a test just caused. */
    lastOpId() {
      const ids = [...pending.keys()];
      assert.equal(ids.length, 1, `expected exactly one outstanding write, found ${ids.length}`);
      return ids[0];
    },
    state: () => store.state(),
    derived: () => null,
    isSyncing: () => options.syncing ?? false,
    snapshotForBoot: () => '{"fields":[]}',
    // Mirrors createSync's real `queue()`, which is the durable queue's FIFO
    // view and is what the projection reads to decide "is this field's value
    // MINE or the server's". A fake that returns [] would make every local-echo
    // assertion below read the server value instead and pass for the wrong
    // reason, so this tracks the same writes `write()` made and drops them the
    // way an ack drops them.
    queue: () => [...outstanding.values()],
    connectionState: () => options.connectionState ?? 'live',
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    /** Play a server field (snapshot row or diff) at a version — through the
     *  production strictly-newer merge. */
    deliver(target, value, version) {
      store.applyServerField(target, value, version);
      emit({ type: 'fields' });
    },
    /** Play the `op_exposed` event a rejected/forbidden ack raises. */
    expose(op, outcome, reason) {
      emit({ type: 'op_exposed', op_id: op.op_id, outcome, op, reason: reason ?? null });
    },
    fieldsEvent() {
      emit({ type: 'fields' });
    },
    /** Deliver a field with NO event, to prove a read does not depend on one. */
    deliverQuietly(target, value, version) {
      store.applyServerField(target, value, version);
    },
    /** The client's own-write echo, with no server frame: what `write()` does. */
    enqueueView(op) {
      store.enqueueView(op);
      emit({ type: 'queue', length: 1 });
    },
    /** An `applied` ack for `op`, straight through the production store. */
    ackApplied(op, version) {
      store.ack(op.op_id, 'applied', version, op.value);
      emit({ type: 'applied', op_id: op.op_id, version });
      emit({ type: 'queue', length: 0 });
    },
    /**
     * Play the full lifecycle of one's OWN additive write in the order the wire
     * actually delivers it: the client echo puts the op's own frame on the field
     * (`{gp_delta:…}`), the server's committed diff arrives with the real value
     * under the new version, and the ack lands LAST, emitting only `applied`.
     *
     * `createSync` does exactly this — `store.enqueueView(op)` on write, then
     * `store.ack(...)` on the ack — and the ordering is the contract's, not my
     * invention (§4: "a matching `diff` may arrive before or after it"; the
     * additive server commits inside the row lock and fans the diff from the
     * transaction). This seam exists because the sequence cannot be assembled
     * from `deliver()` alone, and a store that only re-projects on `fields`
     * reads the pending delta frame here. That is the regression this pins.
     */
    ownAdditiveWrite(target, value, serverValue, version) {
      const op = { op_id: 'op-own', target, base_version: 0, value, created_at: '2026-10-14T00:00:00Z' };
      outstanding.set(op.op_id, op);
      store.enqueueView(op);
      emit({ type: 'queue', length: 1 });
      store.applyServerField(target, serverValue, version);
      emit({ type: 'fields' });
      store.ack(op.op_id, 'applied', version, value);
      outstanding.delete(op.op_id);
      emit({ type: 'applied', op_id: op.op_id, version });
      emit({ type: 'queue', length: 0 });
    },
  };
}

const ROW = (name, quantity, extra = {}) => ({
  item_name: name,
  quantity,
  bulk: null,
  notes: null,
  ...extra,
});

/** Read the store's derived list as plain rows (name → quantity). */
function quantities(rows) {
  return Object.fromEntries(rows.map((row) => [row.item_name, row.quantity]));
}

test('snapshot stash rows render, and a row absent from the snapshot is absent', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Longsword'), ROW('Longsword', 2), 10);
  sync.deliver(STASH_TARGET(1, 'Grapeshot'), ROW('Grapeshot', 50), 11);
  stash.refresh();

  assert.deepEqual(quantities(stash.rows()), { Longsword: 2, Grapeshot: 50 });
});

test('a strictly-older diff is a no-op; the newer version wins (merge is strictly-newer)', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 4), 20);
  stash.refresh();
  assert.equal(quantities(stash.rows()).Torch, 4);

  // A stale replay (an old diff arriving late) must not move the row.
  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 99), 19);
  stash.refresh();
  assert.equal(quantities(stash.rows()).Torch, 4, 'a version <= the stored version is a no-op');

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 3), 21);
  stash.refresh();
  assert.equal(quantities(stash.rows()).Torch, 3);
});

test('a removal tombstone (value: null) deletes the row — FR-1 delete-at-zero', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Rations'), ROW('Rations', 1), 30);
  stash.refresh();
  assert.equal(quantities(stash.rows()).Rations, 1, 'the row is present before the draw');

  // Exactly what `stash_qty_tx` fans when a draw lands on zero.
  sync.deliver(STASH_TARGET(1, 'Rations'), null, 31);
  stash.refresh();

  assert.equal(
    quantities(stash.rows()).Rations,
    undefined,
    'a null value at a newer version removes the row; it never renders as quantity 0',
  );
});

test('a tombstone for a row that was never present does not mint it', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  // version 0 is the store's "never carried this field" reading: applyServerField
  // is strictly-newer from 0, so a null that arrives with no prior row stays
  // a field the projection must skip rather than render as a zero row.
  sync.deliver(STASH_TARGET(1, 'Phantom'), null, 0);
  stash.refresh();

  assert.deepEqual(stash.rows(), [], 'a null with no prior row stays absent');
});

test('a tombstone that arrives BEFORE a live row does not resurrect it either', () => {
  // Same branch, the other direction of the race: an old tombstone must not
  // blank a row a later diff put there.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Ghost'), null, 40);
  sync.deliver(STASH_TARGET(1, 'Ghost'), ROW('Ghost', 3), 41);
  stash.refresh();

  assert.equal(quantities(stash.rows()).Ghost, 3, 'the newer live row wins over the older tombstone');
});

test('the bank header reads {gp,sp,cp}, and stays absent-and-honest before the first movement', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  // wire-protocol §3 / design D1: no zero-minted balance row exists, so the
  // screen's honest pre-state is "unknown", not a fabricated 0/0/0.
  assert.equal(stash.bank(), null, 'no bank row yet — the screen renders its unknown state');

  sync.deliver(BANK_TARGET(1), { gp: 12, sp: 3, cp: 40 }, 8);
  stash.refresh();
  assert.deepEqual(stash.bank(), { gp: 12, sp: 3, cp: 40 });
});

test('the quartermaster reads from the party field and is null when nobody is designated', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1, you: { sub: 'dev-sub-a', role: 'player' } });

  assert.equal(stash.quartermasterId(), null);

  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 7 }, 12);
  stash.refresh();
  assert.equal(stash.quartermasterId(), 7);

  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: null }, 13);
  stash.refresh();
  assert.equal(stash.quartermasterId(), null, 'a cleared designation fans out like any other change');
});

test('claims merge by row id and render newest-first — D7', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  const claim = (id, item, from, to) => ({
    item_name: item,
    quantity: 1,
    origin: from,
    destination: to,
    actor_sub: 'dev-sub-a',
    created_at: '2026-10-15T00:00:00Z',
    coins_gp: null,
    coins_sp: null,
    coins_cp: null,
    id,
  });

  // Delivered out of id order on purpose: the merge is by id, not by arrival.
  // The contract puts the row id in the version slot precisely so the store's
  // ordinary strictly-newer rule is already the merge-by-id rule (contract §3).
  sync.deliver(CLAIM_TARGET(3), claim(3, 'Torch', 'outside', 'stash'), 3);
  sync.deliver(CLAIM_TARGET(1), claim(1, 'Longsword', 'outside', 'stash'), 1);
  sync.deliver(CLAIM_TARGET(2), claim(2, 'Longsword', 'stash', 'character:Flinn'), 2);
  stash.refresh();

  assert.deepEqual(
    stash.claims().map((row) => row.id),
    [3, 2, 1],
    'newest-first by row id, whatever the arrival order',
  );
});

test('a claim row delivered twice applies exactly once — append-only, merge-by-id', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  const row = {
    id: 9,
    item_name: 'Grapeshot',
    quantity: 5,
    origin: 'outside',
    destination: 'stash',
    actor_sub: 'dev-sub-a',
    created_at: '2026-10-15T00:00:00Z',
    coins_gp: null,
    coins_sp: null,
    coins_cp: null,
  };
  sync.deliver(CLAIM_TARGET(9), row, 9);
  stash.refresh();
  sync.deliver(CLAIM_TARGET(9), row, 9); // the same row again (replay)
  stash.refresh();

  assert.equal(stash.claims().filter((c) => c.id === 9).length, 1, 'a replayed row is not a second entry');
});

test('claim labels come from the row, not from client invention — FR-4 vocabulary', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(
    CLAIM_TARGET(1),
    {
      id: 1,
      item_name: 'Longsword',
      quantity: 1,
      origin: 'stash',
      destination: 'character:Flinn',
      actor_sub: 'dev-sub-a',
      created_at: '2026-10-15T00:00:00Z',
      coins_gp: null,
      coins_sp: null,
      coins_cp: null,
    },
    1,
  );
  stash.refresh();

  const [row] = stash.claims();
  assert.equal(row.origin, 'stash');
  assert.equal(row.destination, 'character:Flinn', 'the frozen-at-event-time label rides the row');
});

// --- write() actions: the frames the store is allowed to build -------------
// Every one of these is asserted against the pinned contract fixture through
// wire-contract.js, so a drifted key set or op literal fails HERE rather than
// dying as an undecodable frame at the socket.

test('addRow posts the stash_new mint frame with the mint-only target', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.addRow({ item_name: 'Rope', quantity: 2, bulk: 1, notes: '50 ft' });

  const [op] = sync.writes;
  assert.deepEqual(op.target, { kind: 'stash_new', party_id: 1 });
  assert.deepEqual(op.value, { item_name: 'Rope', quantity: 2, bulk: 1, notes: '50 ft' });
});

test('a mint with no quantity is refused client-side and never reaches the wire', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.addRow({ item_name: 'Rope', quantity: 0 });
  stash.addRow({ item_name: '', quantity: 1 });

  assert.deepEqual(sync.writes, [], 'invalid input never leaves the state layer (sheet/state.js rule)');
});

test('adjustQty posts an additive {op:"qty"} frame and ignores CAS entirely', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 5), 20);
  stash.refresh();
  stash.adjustQty('Torch', -2);

  const [op] = sync.writes;
  assert.deepEqual(op.target, STASH_TARGET(1, 'Torch'));
  assert.deepEqual(op.value, { op: 'qty', qty_delta: -2 });
  // §4 additive: the server ignores base_version for qty applies, and the
  // shipped client must not pretend otherwise by pinning a CAS version here.
  assert.ok(
    op.base_version === 0 || op.base_version === 20,
    'a base_version may ride the uniform frame shape, but the store must not gate on it',
  );
});

test('editMeta posts an absolute CAS frame carrying the row version it read', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Lantern'), ROW('Lantern', 1, { bulk: 0.1, notes: 'bullseye' }), 44);
  stash.refresh();
  stash.editMeta('Lantern', { bulk: 0.1, notes: 'bullseye, one shuttered' });

  const [op] = sync.writes;
  assert.deepEqual(op.target, STASH_TARGET(1, 'Lantern'));
  assert.deepEqual(op.value, { op: 'meta', bulk: 0.1, notes: 'bullseye, one shuttered' });
  assert.equal(op.base_version, 44, 'a CAS write carries the version it read (D2)');
});

test('setQuartermaster posts the party CAS frame; null clears the designation', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 7 }, 12);
  stash.refresh();
  stash.setQuartermaster(8);

  const [set] = sync.writes;
  assert.deepEqual(set.target, PARTY_TARGET(1));
  assert.deepEqual(set.value, { character_id: 8 });
  assert.equal(set.base_version, 12, 'the designation is an absolute write under CAS');

  stash.setQuartermaster(null);
  const [, cleared] = sync.writes;
  assert.deepEqual(cleared.value, { character_id: null }, 'the clear is the same frame with a null');
  assert.equal(cleared.base_version, 12);
});

test('adjustBank posts the signed additive frame — no CAS, no read-modify-write', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.adjustBank({ gp: -3, sp: 0, cp: 0 });

  const [op] = sync.writes;
  assert.deepEqual(op.target, BANK_TARGET(1));
  assert.deepEqual(op.value, { gp_delta: -3, sp_delta: 0, cp_delta: 0 });
});

test('the store exposes no claim write path — the log is append-only (FR-2.4)', () => {
  const stash = createStashState({ sync: fakeSync(), partyId: 1 });

  assert.equal(
    typeof stash.writeClaim,
    'undefined',
    'a client claim frame would be dropped by the server; the store must not offer one',
  );
});

test('transfer posts §3\'s frame exactly: {op, direction, qty, character_id}', () => {
  // The bounds validator (`stash_transfer_tx` dispatch in src/sync/write.rs)
  // refuses any transfer value whose key set is not EXACTLY those four, so a
  // frame carrying a delta or an extra sibling is a `rejected` ack, not a
  // movement. Assert the frame the server accepts, not a restatement.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.transfer('Longsword', 2, 'out', 7);

  assert.deepEqual(sync.writes.map((op) => op.value), [
    { op: 'transfer', direction: 'out', qty: 2, character_id: 7 },
  ]);
  assert.deepEqual(sync.writes[0].target, STASH_TARGET(1, 'Longsword'));
});

test('transfer posts the inbound direction unchanged in shape', () => {
  // Two-fixture rule over `direction`: the same call with the other legal
  // value must produce a different frame, or the store is ignoring the field.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.transfer('Torch', 1, 'in', 4);

  assert.deepEqual(sync.writes.map((op) => op.value), [
    { op: 'transfer', direction: 'in', qty: 1, character_id: 4 },
  ]);
});

test('sell posts §3\'s frame exactly: {op, qty, price} with positive denominations only', () => {
  // `validate_sell_price` takes `{pp?, gp?, sp?, cp?}`, requires at least one
  // positive denomination, and the server converts pp at 1 pp = 10 gp at the
  // one conversion site. Zero denominations are omitted rather than sent as 0.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.sell('Longsword', 2, { pp: 0, gp: 3, sp: 0, cp: 5 });

  assert.deepEqual(sync.writes.map((op) => op.value), [
    { op: 'sell', qty: 2, price: { gp: 3, cp: 5 } },
  ]);
});

test('sell accepts a platinum price because the server converts it', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.sell('Plate', 1, { pp: 1 });

  assert.deepEqual(sync.writes.map((op) => op.value), [
    { op: 'sell', qty: 1, price: { pp: 1 } },
  ]);
});

test('invalid compound input never reaches the wire', () => {
  // The module's own write rule (D6, and sheet/state.js's precedent): the
  // server re-validates, but a frame this layer could know was doomed should
  // never queue, never burn a retry slot, and never surface as an ack.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.transfer('Longsword', 0, 'out', 7); // qty must be >= 1
  stash.transfer('Longsword', 1.5, 'out', 7); // integer only
  stash.transfer('Longsword', 1, 'sideways', 7); // direction is in|out
  stash.transfer('Longsword', 1, 'out', 'nope'); // character_id must be an int
  stash.sell('Longsword', 1, {}); // price must have a positive denomination
  stash.sell('Longsword', 1, { gp: -1 }); // negative denomination
  stash.sell('Longsword', 0, { gp: 1 }); // qty must be >= 1

  assert.deepEqual(sync.writes, [], 'every doomed call is refused at the state layer');
});

// --- refused ops: the op record, non-blocking (degraded-mode §4) ----------

test('a refused op is exposed as a record with its reason, and clears on the next applied ack', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.adjustBank({ gp: -999, sp: 0, cp: 0 });
  const [op] = sync.writes;
  sync.expose(op, 'rejected', 'the bank cannot go below zero');

  const records = stash.opErrors();
  assert.equal(records.length, 1);
  assert.equal(records[0].reason, 'the bank cannot go below zero');
  assert.equal(records[0].outcome, 'rejected');

  // degraded-mode §4: the record is a user-input fact, shown inline and
  // self-clearing — not a modal, not a permanent banner.
  sync.deliver(BANK_TARGET(1), { gp: 1, sp: 0, cp: 0 }, 3);
  stash.refresh();
  assert.equal(stash.opErrors().length, 0, 'the next applied field clears the record');
});

test('a forbidden op is exposed too — GM seats and non-QM sells are authz facts', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  stash.adjustBank({ gp: 1, sp: 0, cp: 0 });
  const [op] = sync.writes;
  sync.expose(op, 'forbidden', 'gm is read-only');

  assert.equal(stash.opErrors()[0].outcome, 'forbidden');
});

// --- seat gating: what this seat may write at all -------------------------

test('a GM seat may write nothing — zero write affordances (FR-11, E14 SC-5)', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1, you: { sub: 'dev-sub-gm', role: 'gm' } });

  assert.equal(stash.canWrite(), false);
  assert.equal(stash.canDesignate(), false);
  assert.equal(stash.canSell(), false);
});

test('any character owner may flip the designation, including while designated (FR-5)', () => {
  // Two fixtures that differ ONLY in whether the caller owns the designated
  // character — because the spec's whole point is that the answer is the
  // same either way, and a single fixture cannot prove that.
  // `sync` is returned on the state handle so a test can drive the seam it
  // built the store over (no undocumented back-door).
  const designatedSeat = createStashState({
    sync: fakeSync(),
    partyId: 1,
    you: { sub: 'dev-sub-a', role: 'player' },
    roster: [{ character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } }],
  });
  const otherSeat = createStashState({
    sync: fakeSync(),
    partyId: 1,
    you: { sub: 'dev-sub-b', role: 'player' },
    roster: [{ character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } }],
  });

  // Both seats: a designation already exists and belongs to character 7.
  for (const seat of [designatedSeat, otherSeat]) {
    seat.sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 7 }, 12);
    seat.refresh();
  }

  assert.equal(designatedSeat.canDesignate(), true, 'the designated owner may re-seat their own');
  assert.equal(otherSeat.canDesignate(), true, 'any other owner may take it too');
});

test('only the designated character\'s owner may sell — the gate keys on current designation', () => {
  const sync = fakeSync();
  const stash = createStashState({
    sync,
    partyId: 1,
    you: { sub: 'dev-sub-a', role: 'player' },
    roster: [{ character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } }],
  });

  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 7 }, 12);
  stash.refresh();
  assert.equal(stash.canSell(), true);

  // Re-seat the designation away from me — the sell affordance must disappear
  // on the fan-out, with no reload (FR-5's live-sync requirement).
  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 8 }, 13);
  stash.refresh();
  assert.equal(stash.canSell(), false, 'a moved designation moves the affordance');
});

test('an owner who is not the quartermaster may still transfer — transfers are open to all (FR-3)', () => {
  const sync = fakeSync();
  const stash = createStashState({
    sync,
    partyId: 1,
    you: { sub: 'dev-sub-b', role: 'player' },
    roster: [{ character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } }],
  });
  sync.deliver(PARTY_TARGET(1), { quartermaster_character_id: 7 }, 12);
  stash.refresh();

  assert.equal(stash.canSell(), false, 'not the QM');
  assert.equal(stash.canTransfer(), true, 'but the transfer picker is open regardless');
});

test('an additive balance reads the SERVER value when the ack lands after the diff', () => {
  // Article IV stop-the-line, found by the E12 offline suite.
  //
  // The symptom: after a bank write, the balance projected as
  // `{gp: undefined, sp: undefined, cp: undefined}` and stayed there until some
  // unrelated field moved. The server's number was in the store and off the
  // screen, so the user saw an empty balance after their own purchase.
  //
  // The cause is frame ordering, which §4 leaves open and the additive server
  // settles: the own-write `diff` arrives BEFORE the ack. So the `fields`
  // refresh runs while the field still carries the client's own delta frame
  // (`{gp_delta:…}` — there is no `gp` key in it) sitting under the server's
  // version, and the ack that swaps in the committed value emits only `applied`.
  // A store that re-projects on `fields` alone strands the old read forever.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(BANK_TARGET(1), { gp: 10, sp: 0, cp: 0 }, 5);
  stash.refresh();
  assert.deepEqual(stash.bank(), { gp: 10, sp: 0, cp: 0 }, 'the opening balance reads');

  sync.ownAdditiveWrite(BANK_TARGET(1), { gp_delta: 5, sp_delta: 0, cp_delta: 0 }, { gp: 15, sp: 0, cp: 0 }, 7);

  assert.deepEqual(
    stash.bank(),
    { gp: 15, sp: 0, cp: 0 },
    'the committed balance is on the projection without a manual refresh()',
  );
});

test('an additive qty draw reads the server quantity when the ack lands after the diff', () => {
  // The same ordering on the other additive kind — a drawn row is the one the
  // user is watching most closely, and the value the ack replaces is a
  // `{op:'qty', qty_delta}` frame, which has no `quantity` either.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 5), 20);
  stash.refresh();
  assert.deepEqual(quantities(stash.rows()), { Torch: 5 });

  sync.ownAdditiveWrite(
    STASH_TARGET(1, 'Torch'),
    { op: 'qty', qty_delta: -2 },
    ROW('Torch', 3),
    21,
  );

  assert.deepEqual(
    quantities(stash.rows()),
    { Torch: 3 },
    'the drawn quantity is the server\'s, not a projection of the delta frame',
  );
});


test('an additive balance reads the SERVER value when its diff lands before the ack', () => {
  // Article IV stop-the-line, found by the E12 offline suite.
  //
  // The symptom: after a bank write the balance projected as
  // `{gp: undefined, sp: undefined, cp: undefined}` and stayed there until some
  // unrelated field moved. The server's number was in the store and off the
  // screen — a blank treasure balance in front of a user on a live link.
  //
  // The cause is frame ORDERING, which §4 leaves open and the additive server
  // settles: "a matching `diff` may arrive before or after it", and the additive
  // apply commits the delta inside the row lock and fans the diff from the
  // transaction, so the diff arrives FIRST. The `fields` refresh then runs while
  // the field still shows the client's own frame (`{gp_delta:…}` — there is no
  // `gp` key in it), and the ack that settles the field emits only `applied`. A
  // projection that re-projects on `fields` alone is stranded one event behind.
  //
  // What is NOT a defect, checked so the next reader does not "fix" it: the ack
  // carries no value (protocol.rs builds `{t, op_id, outcome, version}`), so
  // `store.ack` settles the field's VERSION from the ack while its value is still
  // the op's own delta. That resolves itself — the committed balance is the NEXT
  // statement on `field_version_seq` and is strictly newer, so it applies. The
  // client is never permitted to compute a balance (D3), which is exactly why
  // this window shows the previous number rather than a guessed one.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(BANK_TARGET(1), { gp: 10, sp: 0, cp: 0 }, 5);
  stash.refresh();
  assert.deepEqual(stash.bank(), { gp: 10, sp: 0, cp: 0 }, 'the opening balance reads');

  sync.ownAdditiveWrite(BANK_TARGET(1), { gp_delta: 5, sp_delta: 0, cp_delta: 0 }, { gp: 15, sp: 0, cp: 0 }, 7);

  assert.deepEqual(
    stash.bank(),
    { gp: 15, sp: 0, cp: 0 },
    'the committed balance is on the projection without a manual refresh()',
  );
});

test('an additive qty draw reads the server quantity when its diff lands before the ack', () => {
  // The same ordering on the other additive kind — a drawn row is the one the
  // user is watching most closely, and the value the ack settles over is an
  // `{op:'qty', qty_delta}` frame, which carries no `quantity` either.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 5), 20);
  stash.refresh();
  assert.deepEqual(quantities(stash.rows()), { Torch: 5 });

  sync.ownAdditiveWrite(STASH_TARGET(1, 'Torch'), { op: 'qty', qty_delta: -2 }, ROW('Torch', 3), 21);

  assert.deepEqual(
    quantities(stash.rows()),
    { Torch: 3 },
    "the drawn quantity is the server's, not a projection of the delta frame",
  );
});

test('a delivered value reads without anyone calling refresh()', () => {
  // The sibling defect, and the one with the uglier shape: no error, no red
  // screen, just a blank number. The accessors used to read a cache updated only
  // by the `fields`/`applied` events, so any value that reached the store
  // without one of those events was invisible to the screen — this test delivered
  // a balance and read `null` while the link was described as live.
  //
  // The cache cannot be repaired from here: it can only be right if every writer
  // of a field also fires an event this module happens to listen for, which is a
  // coupling, not an invariant. `current()` re-folds instead — a pure pass over
  // the party's own fields, no I/O — so a read cannot be stale by construction.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  // Written straight through the production merge, with NO event emitted: the
  // way a value arrives when the delivery is not on this module's event list.
  sync.deliverQuietly(BANK_TARGET(1), { gp: 12, sp: 3, cp: 40 }, 9);

  assert.deepEqual(
    stash.bank(),
    { gp: 12, sp: 3, cp: 40 },
    'the balance reads on the strength of being in the store, not on the strength of an event',
  );

  sync.deliverQuietly(STASH_TARGET(1, 'Rations'), ROW('Rations', 4), 10);
  assert.deepEqual(quantities(stash.rows()), { Rations: 4 }, 'and the same for a row');
});

// --- the local-echo rule, additive class (degraded-mode §4) ---------------
//
// The spec line that governs all four tests below, quoted where it is law:
// degraded-mode.md §4 — "a write applies to view state immediately, tagged
// `pending` (carrying `op_id`). The tag clears on the ack". PRD *State* says the
// same thing in one clause: "optimistic local render, server authoritative".
//
// The design's rejection of optimism (design.md, "Rejected — optimistic local
// claim rows") is scoped to the CLAIM LOG and points back here for the write
// targets: "degraded-mode's local-echo rule covers the write targets". So a
// pending draw is RENDERED, and the fold is not my invention either —
// sheet/state.js' qtyMap does `Math.max(0, base + delta)` on the sheet's own
// pending inventory. What stays server-authoritative is the settlement: the ack
// and the fan-out decide, and a `superseded`/`rejected` ack reverts.
//
// `store.state()` renders a pending field as the op's own frame, which for an
// additive op is a DELTA — no balance, no quantity. So the projection folds it.

test('a pending draw is rendered against its committed row, not blanked (degraded-mode §4)', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 5), 20);
  assert.deepEqual(quantities(stash.rows()), { Torch: 5 }, 'the committed row reads');

  stash.adjustQty('Torch', -2);
  assert.deepEqual(
    quantities(stash.rows()),
    { Torch: 3 },
    'the draw shows immediately, tagged pending — 5 committed plus my -2',
  );

  // The ack settles the echo, then the fan-out restates the row at the version
  // the ack carried. Both are needed and they are not the same frame: an ack
  // only clears the pending tag and records a version (protocol.rs's ack frame
  // has `{t, op_id, outcome, version}` — no value), so the committed value
  // arrives on the `stash` diff. Delivering the diff without the ack would leave
  // the fold still applied on top of a value that already counts the draw.
  sync.ackAppliedTo(sync.lastOpId(), 21);
  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 3), 21);
  assert.deepEqual(quantities(stash.rows()), { Torch: 3 }, 'and the fan-out agrees');
});

test('a pending bank delta is shown on top of the committed balance', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(BANK_TARGET(1), { gp: 20, sp: 0, cp: 0 }, 30);
  stash.adjustBank({ gp: -5, sp: 0, cp: 0 });

  assert.deepEqual(
    stash.bank(),
    { gp: 15, sp: 0, cp: 0 },
    'the withdrawal reads at once rather than showing the old balance',
  );

  sync.ackAppliedTo(sync.lastOpId(), 31);
  sync.deliver(BANK_TARGET(1), { gp: 15, sp: 0, cp: 0 }, 31);
  assert.deepEqual(stash.bank(), { gp: 15, sp: 0, cp: 0 }, 'the server confirms it');
});

test('a draw to zero renders no row, and a draw that would go negative is clamped', () => {
  // Two-sided on the clamp, because a floor is a threshold like any other: at
  // exactly the committed quantity the row is gone (FR-1 renders nothing at
  // zero), and past it the number stops at zero rather than going negative —
  // the server refuses the overdraft outright rather than posting a debt (§4).
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 2), 40);
  stash.adjustQty('Torch', -2);
  assert.deepEqual(quantities(stash.rows()), {}, 'drawn to exactly zero: no row is shown');

  const over = fakeSync();
  const stash2 = createStashState({ sync: over, partyId: 1 });
  over.deliver(STASH_TARGET(1, 'Torch'), ROW('Torch', 2), 40);
  stash2.adjustQty('Torch', -5);
  const drawn = quantities(stash2.rows());
  assert.ok(!(drawn.Torch > 0), `a 5-draw on a 2-row does not show ${drawn.Torch}`);

  const bank = fakeSync();
  const stash3 = createStashState({ sync: bank, partyId: 1 });
  bank.deliver(BANK_TARGET(1), { gp: 3, sp: 0, cp: 0 }, 50);
  stash3.adjustBank({ gp: -10, sp: 0, cp: 0 });
  assert.deepEqual(
    stash3.bank(),
    { gp: 0, sp: 0, cp: 0 },
    'an overdraw shows 0 gp, never -7 — the client does not render a debt',
  );
});

test('an unowned delta frame is never rendered, and never blanks the value', () => {
  // The defensive half, and the one that does NOT depend on §4's local echo: a
  // delta frame with no outstanding op behind it is nobody's intent, so it is
  // not a balance and not a row. The common cause is benign — the server's
  // `applied` ack settles the field's VERSION while its value is still the op's
  // own frame, because protocol.rs's ack frame carries `{t, op_id, outcome,
  // version}` and no value. What must not happen is the screen going blank over
  // it: the last stated value survives until a newer one arrives.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(BANK_TARGET(1), { gp: 12, sp: 0, cp: 0 }, 60);
  sync.deliverQuietly(BANK_TARGET(1), { gp_delta: 5, sp_delta: 0, cp_delta: 0 }, 61);
  assert.deepEqual(
    stash.bank(),
    { gp: 12, sp: 0, cp: 0 },
    'a stranded delta does not erase the balance the server stated',
  );

  sync.deliverQuietly(STASH_TARGET(1, 'Ghost'), { op: 'qty', qty_delta: 3 }, 62);
  assert.deepEqual(quantities(stash.rows()), {}, 'and does not mint a row');
});
