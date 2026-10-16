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
import { createStore } from '../../src/lib/sync/store.js';

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

test('editMeta with both fields null posts the CLEAR frame', () => {
  // The null-meta short-circuit review round 1 asked about, and it is real: bulk
  // is an `Option<f64>` (unset it by sending null) and notes is nullable, so
  // `{bulk: null, notes: null}` is the affordance that clears a row's bulk — not
  // a no-op. A `meta.bulk !== null` short-circuit would look right in a unit test
  // that never reached the branch and would ship a dead control.
  //
  // The screen's Edit row already gets this far, which is why the test goes
  // through the store rather than trusting the DOM: its bulk input is
  // `Number(bulkDraft) || null`, and clearing the input makes `Number('')` 0,
  // which `|| null` turns back into null. So an empty Bulk box on save IS a clear,
  // every time, and that path must stay open.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(STASH_TARGET(1, 'Lantern'), ROW('Lantern', 1, { bulk: 0.1, notes: 'bullseye' }), 44);
  stash.refresh();
  stash.editMeta('Lantern', { bulk: null, notes: null });

  const [op] = sync.writes;
  assert.deepEqual(op.value, { op: 'meta', bulk: null, notes: null }, 'a clear posts, it is not swallowed');
  assert.equal(op.base_version, 44);
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

test('two offline draws on ONE field echo the sum, not the last delta', () => {
  // degraded-mode §4 makes multiple ops on one field the expected offline shape,
  // not an edge case: "a write applies to view state immediately, tagged
  // `pending`", over a queue that is an unbounded FIFO with no coalescing. A
  // player tapping draw twice offline has drawn twice.
  //
  // `readFields` used to file `mine` in a Map keyed by target with `set` inside
  // the queue loop, so the second op OVERWROTE the first and the fold saw one
  // delta — three taps on ten arrows showed one gone, and every test stayed green
  // because each one had a single pending op (sample of one). Driven through
  // `adjustQty`, the production write path, so an op that never reaches the queue
  // cannot fake this green either.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 10), 3);

  stash.adjustQty('Arrow', -1);
  assert.deepEqual(quantities(stash.rows()), { Arrow: 9 }, 'the first draw echoes');
  stash.adjustQty('Arrow', -1);
  assert.deepEqual(quantities(stash.rows()), { Arrow: 8 }, 'so does the second, on top of it');
  stash.adjustQty('Arrow', -2);
  assert.deepEqual(quantities(stash.rows()), { Arrow: 6 }, 'and a third, folded in FIFO order');
});

test('two offline bank withdrawals on ONE field echo the sum', () => {
  // The same collision on the other additive kind, and the reason a
  // quantity-equality test would not have caught it: two withdrawals of
  // DIFFERENT sizes are exactly what a real offline session produces.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(BANK_TARGET(1), { gp: 20, sp: 0, cp: 0 }, 4);

  stash.adjustBank({ gp: -3 });
  assert.deepEqual(stash.bank(), { gp: 17, sp: 0, cp: 0 }, 'the first withdrawal echoes');
  stash.adjustBank({ gp: -5 });
  assert.deepEqual(stash.bank(), { gp: 12, sp: 0, cp: 0 }, 'the second lands on the first');
  // And the third is refused whole: the server tracks a bank per denomination and
  // refuses the adjustment if ANY denomination would go below zero
  // (write.rs:1853-1859, reason selected at :1882-1890), so a -1 sp against a 0 sp
  // bank never lands. What happens to anything queued BEHIND it is the other half
  // of the story, and it is `continue` rather than `break`: the guard lives in the
  // UPDATE's WHERE clause, so a refused op matches no row, the base is untouched,
  // and the next op applies to it — see the two [refused, applicable] tests below.
  // Folding gp while refusing sp would render a balance the server cannot hold.
  stash.adjustBank({ gp: -2, sp: -1 });
  assert.deepEqual(stash.bank(), { gp: 12, sp: 0, cp: 0 }, 'a partial overdraw is refused whole, so the balance holds');
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

test('a draw to zero renders no row, and a draw past it is refused whole', () => {
  // Two-sided on the threshold, because a refusal boundary is a threshold like
  // any other and AGENTS.md's *Rules exactness* asks for both sides of it: at
  // exactly the committed quantity the row is gone (FR-1 renders nothing at
  // zero), and past it the row shows the COMMITTED count, not a clamped zero.
  //
  // This is finding 3, and the old assertion here was the bug. It demanded a
  // 5-draw on a 2-row echo `0`, but the server does not clamp and it does not
  // zero: the guard at write.rs:1853-1859 refuses the adjustment ENTIRELY ("that
  // adjustment
  // would drive {name} below zero", :1882-1890), which is FR-7's "refused whole, with a
  // reason." An echo of 0 is therefore just as much a number the server will
  // never hold as -3 was — the floor was the wrong conclusion from a correct
  // premise ("do not show -7"). The committed value is the only number on the
  // client that survives the refusal, and it is what the screen must keep showing
  // while the refusal notice arrives.
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
  assert.equal(drawn.Torch, 2, 'the overdraw is skipped, so the committed 2 stays on screen');

  const bank = fakeSync();
  const stash3 = createStashState({ sync: bank, partyId: 1 });
  bank.deliver(BANK_TARGET(1), { gp: 3, sp: 0, cp: 0 }, 50);
  stash3.adjustBank({ gp: -10, sp: 0, cp: 0 });
  assert.deepEqual(
    stash3.bank(),
    { gp: 3, sp: 0, cp: 0 },
    'an overdraw keeps the committed 3 gp — never -7 (no debt) and never 0 (a balance the server will never hold)',
  );
});

test('B1 bank: a refused op ends only itself, the op queued behind it still folds', () => {
  // B1. The fold used to `break` on the first refusal, on the premise that a
  // refused delta leaves "no base to fold onto" for the rest of the field's
  // queue. That premise is false. `apply_bank_delta` (write.rs:1795, :1965)
  // returns `PartyApply::rejected` for the ONE op whose UPDATE matched no row;
  // the queue driver then applies the NEXT op against the unchanged base. So
  // the server ends up holding 5 + 100 = 105 gp while the client, having broken
  // out of the fold at the -10, showed 5. degraded-mode §4 is unconditional that
  // a write applies to view state immediately — so a view that stops at the
  // refusal shows a balance the server will never hold, which is the very fault
  // the floor was introduced to prevent.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(BANK_TARGET(1), { gp: 5, sp: 0, cp: 0 }, 80);

  stash.adjustBank({ gp: -10, sp: 0, cp: 0 }); // would drive gp below zero: refused whole
  stash.adjustBank({ gp: 100, sp: 0, cp: 0 }); // applies against the unchanged 5

  assert.deepEqual(
    stash.bank(),
    { gp: 105, sp: 0, cp: 0 },
    'the refused op folds to nothing and the next op still folds onto the base',
  );
  // And the refusal itself is not silently swallowed — the queue has to step
  // over it, not pretend it never happened.
  sync.expose(sync.writes[0], 'rejected', 'would drive gp below zero');
  assert.equal(stash.opErrors().length, 1, 'the refused op is still exposed to the user');
});

test('B1 stash row: a refused draw ends only itself, the draw behind it still folds', () => {
  // B1, the stash-row site. Same server semantics: `stash_delta` returns
  // `Refused` for the one op (write.rs:1781) and the driver goes on to the next.
  // Breaking here showed Arrow at its committed 2 where the server would land on
  // 2 + 10 = 12.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 2), 81);

  stash.adjustQty('Arrow', -5); // 2 - 5 < 0: refused whole
  stash.adjustQty('Arrow', 10); // applies against the unchanged 2

  assert.deepEqual(
    quantities(stash.rows()),
    { Arrow: 12 },
    'the refused draw folds to nothing and the next draw still folds onto the base',
  );
});

test('B1b bank fold guards the i32 ceiling on BOTH sides of the threshold', () => {
  // B1b. The floor guard had no ceiling partner. The server's apply is
  // `gp::bigint + $1 BETWEEN 0 AND $5` with `$5 = i32::MAX` (write.rs:1855-1858),
  // so it refuses the top end exactly as firmly as the bottom one and for the
  // same reason: the columns are int4. A fold that steps over a refusal at zero
  // but not at the ceiling diverges from the server at the other end of the
  // number line. AGENTS.md's *Rules exactness* asks for both sides of an asserted
  // threshold, so this asserts `MAX` and `MAX - 1` with different results.
  const MAX = 2147483647;

  const atCeiling = fakeSync();
  const stash = createStashState({ sync: atCeiling, partyId: 1 });
  atCeiling.deliver(BANK_TARGET(1), { gp: MAX, sp: 0, cp: 0 }, 82);
  stash.adjustBank({ gp: 1, sp: 0, cp: 0 }); // MAX + 1 > i32::MAX: refused whole
  assert.deepEqual(
    stash.bank(),
    { gp: MAX, sp: 0, cp: 0 },
    'at the ceiling the fold shows the committed balance, never 2147483648',
  );

  const underCeiling = fakeSync();
  const stash2 = createStashState({ sync: underCeiling, partyId: 1 });
  underCeiling.deliver(BANK_TARGET(1), { gp: MAX - 1, sp: 0, cp: 0 }, 83);
  stash2.adjustBank({ gp: 1, sp: 0, cp: 0 }); // MAX - 1 + 1 == MAX: applies
  assert.deepEqual(
    stash2.bank(),
    { gp: MAX, sp: 0, cp: 0 },
    'one below the ceiling the same +1 lands — the guard is the threshold, not a wall',
  );

  // The ceiling is per denomination and a refusal is whole, so a sp overflow
  // must not let a gp addition through either (the same partial-refusal rule the
  // floor already enforces at the bottom end).
  const spOverflow = fakeSync();
  const stash3 = createStashState({ sync: spOverflow, partyId: 1 });
  spOverflow.deliver(BANK_TARGET(1), { gp: 3, sp: MAX, cp: 0 }, 84);
  stash3.adjustBank({ gp: 2, sp: 1, cp: 0 });
  assert.deepEqual(
    stash3.bank(),
    { gp: 3, sp: MAX, cp: 0 },
    'a single overflowing denomination refuses the whole frame, so no denomination moves',
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

test('a subscriber may read a plain accessor during its own notification', () => {
  // The exact shape StashScreen.svelte takes: it subscribes to `changed()` and,
  // inside the callback, re-reads the projection through the plain accessors.
  //
  // `current()` used to `projection.set(next)` on EVERY read, so that callback
  // wrote the very writable that was mid-notification — svelte 5 re-queued the
  // subscriber, which read again, which wrote again. Not a slow render: an
  // unbounded re-entrancy that never yields, so vitest could not even time it
  // out. The reproduction is the subscriber below; if the loop is back, this
  // file goes red with a counted error instead of wedging the run at 100% CPU.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });

  sync.deliver(BANK_TARGET(1), { gp: 4, sp: 2, cp: 1 }, 70);

  let reads = 0;
  let firstError = null;
  const unsubscribe = stash.changed().subscribe(() => {
    try {
      reads += 1;
      // The accessor, called from inside the notification — the line that
      // re-entered the writable.
      stash.bank();
      stash.rows();
      if (reads > 50) throw new Error(`a read inside a notification re-notified ${reads} times`);
    } catch (e) {
      firstError ??= e;
      throw e;
    }
  });

  try {
    assert.equal(firstError, null, `reading the projection re-notified: ${firstError?.message}`);
    // And the accessor still reads live after the subscription settles — the
    // fix must not trade the loop for staleness.
    sync.deliver(BANK_TARGET(1), { gp: 9, sp: 0, cp: 0 }, 71);
    assert.deepEqual(stash.bank(), { gp: 9, sp: 0, cp: 0 }, 'a later frame still moves the read');
  } finally {
    unsubscribe();
  }
});

test('adjustBank refuses a fractional or wrapped coin count instead of truncating it', () => {
  // The client must build a frame the SERVER accepts. §4 fixes only what a blank
  // means (no adjustment); write.rs's `validate_bank` fixes what the
  // input may be, and it is a hard boundary — each of `gp_delta`/`sp_delta`/
  // `cp_delta` must be present, a JSON number, and an integer in i32 range
  // (`as_i64` + `i32::try_from`, write.rs:3182-3199), so a string or absent key is refused whole. `Number(x)
  // | 0` aimed at that boundary and missed it twice: 2.7 posted 2, and 1e10
  // wrapped to 1410065408 — a party's whole bank, mis-posted by an arithmetic
  // wrap. Both are refused; a blank still encodes as the integer 0, and a string
  // integer is still a number.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  stash.adjustBank({ gp: 2.7 });
  stash.adjustBank({ gp: 1e10 });
  // `1e99` is not an integer either (it is Infinity, and Infinity fails
  // Number.isInteger), so this is the third refusal rather than a fourth case.
  stash.adjustBank({ gp: 1e99 });
  assert.deepEqual(sync.writes.map((w) => w.value), [], 'a refused count posts nothing at all');

  // A string integer is a real number, so it posts — and a single-denomination
  // frame still carries the other two as the INTEGER 0.
  stash.adjustBank({ gp: '4' });
  assert.equal(sync.writes.length, 1);
  assert.deepEqual(sync.writes[0].value, { gp_delta: 4, sp_delta: 0, cp_delta: 0 });

  // A blank slot encodes as the INTEGER 0, it does not omit the key: write.rs
  // refuses an absent `gp_delta` outright, so omitting would lose the whole
  // adjustment rather than one denomination. Here the three blanks together are
  // a no-op, and a no-op is not sent.
  stash.adjustBank({ gp: '', sp: null, cp: undefined });
  assert.equal(sync.writes.length, 1, 'a no-op adjustment is not sent');
  assert.deepEqual(
    Object.keys(sync.writes[0].value).sort(),
    ['cp_delta', 'gp_delta', 'sp_delta'],
    'a bank frame always carries all three deltas',
  );
});

test('a stash row fold guards the i32 ceiling on both sides too', () => {
  // The row's guard is `quantity::bigint + $1 BETWEEN 1 AND $4` (write.rs:1289)
  // — same two-sided shape as the bank's, and FR-1's delete-at-zero arm
  // (write.rs:1310) is a separate apply path, so zero is legal while MAX + 1 is
  // not. Both sides of the ceiling are asserted per the two-fixture rule.
  const rowCeiling = fakeSync();
  const stash4 = createStashState({ sync: rowCeiling, partyId: 1 });
  rowCeiling.deliver(STASH_TARGET(1, 'Coin'), ROW('Coin', 2_147_483_647), 90);
  stash4.adjustQty('Coin', -1);
  assert.deepEqual(quantities(stash4.rows()), { Coin: 2_147_483_646 }, 'one below the row ceiling moves');
  stash4.adjustQty('Coin', 1);
  assert.deepEqual(quantities(stash4.rows()), { Coin: 2_147_483_647 }, 'landing exactly on it is inside the guard');
  stash4.adjustQty('Coin', 1);
  assert.deepEqual(quantities(stash4.rows()), { Coin: 2_147_483_647 }, 'one past it is refused, so the row stays at MAX');
});

// --- round-2 finding 2: a refusal removes ONE op, not the queue behind it ---
//
// `apply_bank_delta` (write.rs:1853-1862) is an UPDATE whose WHERE clause carries
// the guard: `gp::bigint + $1 BETWEEN 0 AND $5`. When the guard fails the UPDATE
// matches no row, so the balance is left EXACTLY where it was and the next op in
// the queue applies to that same untouched base. `apply_stash_delta`
// (write.rs:1285-1292) is the same shape for its ceiling. So the client's fold
// must SKIP a refused delta, not END the fold — and the comment that claimed "the
// rest of that field's queue has no base to fold onto either" had the server's
// semantics backwards. These two tests are the [refused, applicable] pair that
// comment made unreachable.

test('a refused bank withdrawal does not swallow a valid deposit queued behind it', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(BANK_TARGET(1), { gp: 5, sp: 0, cp: 0 }, 4);

  // Refused whole: 5 - 10 < 0 (FR-7).
  stash.adjustBank({ gp: -10 });
  assert.deepEqual(stash.bank(), { gp: 5, sp: 0, cp: 0 }, 'the refused draw leaves the balance where it was');

  // Valid, and the server applies it to the SAME base the refused one saw.
  stash.adjustBank({ gp: 100 });
  assert.deepEqual(
    stash.bank(),
    { gp: 105, sp: 0, cp: 0 },
    'the deposit still folds — the server would hold 105, not 5',
  );
});

test('a refused row draw does not swallow a valid draw queued behind it', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 2), 3);

  // Refused: 2 - 5 is below the UPDATE's `BETWEEN 1 AND i32::MAX` and is not
  // exactly zero, so neither apply arm fires and the row keeps quantity 2.
  stash.adjustQty('Arrow', -5);
  assert.deepEqual(quantities(stash.rows()), { Arrow: 2 }, 'the refused draw leaves the row intact');

  stash.adjustQty('Arrow', 10);
  assert.deepEqual(
    quantities(stash.rows()),
    { Arrow: 12 },
    'the second draw still folds — the server would hold 12, not 2',
  );
});

// --- round-2 finding 3: the server's guard is two-sided, so the fold's is too ---
//
// Both guards read `BETWEEN 0 AND $5` / `BETWEEN 1 AND $4` with the bound set to
// `i64::from(i32::MAX)` (write.rs:1858, :1289), and the bank's refusal reason for
// that half is "that adjustment would overflow the balance" (write.rs:1884). A
// fold that only tests `< 0` echoes a balance the int4 column cannot hold. Both
// sides of the threshold are asserted, per the two-fixture rule: a one-sided
// fixture passes against every wrong ceiling on its own side.

test('a bank deposit past the i32 ceiling is refused, and one landing exactly on it is not', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(BANK_TARGET(1), { gp: 2_147_483_647, sp: 0, cp: 0 }, 4);

  // Just under: 2147483647 - 2 + 1 lands on i32::MAX. Inside the guard, so it folds.
  stash.adjustBank({ gp: -2 });
  assert.deepEqual(stash.bank(), { gp: 2_147_483_645, sp: 0, cp: 0 }, 'the base moves off the ceiling');
  stash.adjustBank({ gp: 2 });
  assert.deepEqual(
    stash.bank(),
    { gp: 2_147_483_647, sp: 0, cp: 0 },
    'a deposit landing exactly ON i32::MAX is inside BETWEEN 0 AND i32::MAX',
  );

  // One over, on top of a base already at the ceiling: the server's `+ $1 BETWEEN
  // 0 AND $5` fails and refuses whole, so the fold must refuse too.
  stash.adjustBank({ gp: 1 });
  assert.deepEqual(
    stash.bank(),
    { gp: 2_147_483_647, sp: 0, cp: 0 },
    'a deposit past the ceiling is refused whole, exactly as the server refuses it',
  );
});

// --- gh#340: a delete is not a refusal, so the fold must tell them apart -----
//
// A draw that lands a row on exactly zero is the server's DELETE arm
// (`quantity::bigint + $3 = 0`, write.rs:1310-1313) — APPLIED, not a refusal.
// Every op behind it then addresses no row and `apply_stash_delta` refuses it
// `unknown target` (write.rs:1326-1336). A refusal is different: its guard lives
// in the UPDATE's WHERE, so nothing is written and the base survives for the op
// behind it — which is why #311's finding-2 fix made a refused delta `continue`
// rather than end the fold. `continue` is right for a refusal and wrong for a
// delete, which removes the base. Three tests, three shapes:
//
//   1. [-2]           lands on zero, nothing behind it.
//                                        Echo: no row            (test 1)
//   2. [-2, +10]      lands on zero, then an applicable-looking deposit. On the
//                      unfixed fold the row is still in the displayed set when
//                      that op folds, so it lands on the base of 2 the DELETE had
//                      already consumed and the projection shows `Arrow: 10` -- a
//                      row the server will never hold, and one the sum reaches
//                      without ever resting on zero, so the render-after-loop
//                      never sees it either.
//                                        Echo: no row            (test 2)  <-- RED pre-fix
//   3. [-2, -10]      lands on zero, then an op the server refuses `unknown
//                      target`. Locally the `< 0` guard skips it, so this is the
//                      shape that was ALREADY honest before the fix -- it is here
//                      as the control that keeps test 2's phantom honest, and it
//                      is green on the unfixed tree.
//                                        Echo: no row            (test 3)
//
// So exactly one test here is red before the fix: test 2. That is the whole of the
// suite's claim about gh#340, and it is stated rather than padded. Two mutants
// survive every shape in this section, and both are claims about the tests:
//   - a stop at zero-or-below (`<= 0`) instead of exactly zero. Nothing can reach
//     the difference: the delta that would show it is refused whole by the `< 0`
//     guard before the stop is consulted, so `<= 0` is inert here -- but it is also
//     wrong in a way the row's floor makes visible only if a sum can land below
//     zero, which the guard forbids. `=== 0` is the arm the server has; keep it.
//   - dropping the loop's `break`. Once the row is spliced out of the displayed
//     set the ops behind the delete fold onto a `quantity` nothing renders, so the
//     projection is identical with or without it. The `break` saves the walk and
//     states the rule; it is not what the tests see.
//
// What IS pinned, and measured: the splice inside the loop. Hoist it back out to
// the render after the loop and four tests in this file go red, this one among
// them -- that is the pair the fix turns on.

test('a draw to exactly zero shows no row, with nothing queued behind it (gh#340)', () => {
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 2), 77);

  stash.adjustQty('Arrow', -2);

  assert.deepEqual(quantities(stash.rows()), {}, 'the row is gone, as FR-1 requires');
});

test('an op refused locally behind a delete folds to nothing and the row stays deleted (gh#340)', () => {
  // `-10` behind the delete is refused on the server (`unknown target`), so it
  // folds to nothing there. Locally it is NOT skippable -- against the base the
  // fold is holding, it would drive the sum to -10, which is neither a legal
  // quantity (the row's floor is 1, write.rs:1289) nor a state the server can ever
  // hold, since the row is already gone. The stop is the only thing between the
  // fold and that.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 2), 77);

  stash.adjustQty('Arrow', -2); // sum 0: the server DELETES the row
  stash.adjustQty('Arrow', -10); // no row exists: refused `unknown target`

  assert.deepEqual(
    quantities(stash.rows()),
    {},
    'nothing behind the delete is folded, so nothing revives or corrupts the row',
  );
});

test('an applicable deposit behind a delete is not folded onto the deleted row (gh#340)', () => {
  // THE defect, asserted directly. On the unfixed fold the row survives the delete
  // until the end of the loop, so `+10` lands on the base of 2 that the delete had
  // just consumed and the projection renders `Arrow: 10` -- a quantity the server
  // will never hold. Ordering matters here and the comment says why: in `[-2,
  // +10]` the running sum never rests on zero, so a render-after-loop has no zero
  // state to catch either. `quantities()` flattens a duplicated row to its last
  // entry, so the assertion is written against the row being GONE rather than
  // against a row count: the phantom is the thing that must be impossible.
  //
  // This is the one test red on the unfixed tree. The other two in this section
  // bound the stop from the other side and from FR-1's own rule, and both read
  // green either way -- they are here so the fix cannot be made by something
  // wilder than a stop (wiping the row on any later op passes all three; so does
  // always rendering no row).
  //
  // The section header above carries the two mutants this run cannot kill and why
  // (`<= 0` survives; so does dropping the `break`). What this test alone pins is
  // the phantom: without the in-loop splice, `Arrow: 10` here, red on the unfixed
  // tree.
  const sync = fakeSync();
  const stash = createStashState({ sync, partyId: 1 });
  sync.deliver(STASH_TARGET(1, 'Arrow'), ROW('Arrow', 2), 77);

  stash.adjustQty('Arrow', -2); // sum 0: the server DELETES the row
  stash.adjustQty('Arrow', 10); // the op behind it must not fold onto the removed base

  assert.deepEqual(
    quantities(stash.rows()),
    {},
    'nothing behind the delete is folded, so the row stays deleted',
  );
});
