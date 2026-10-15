// E12 Task 7 (design §7 test strategy item 3) — the stash under a dead link.
//
// The epic's real claim is not "the stash works online"; it is that it behaves
// correctly when the link is gone. Four things are pinned here, and each one is
// a way an offline-first screen usually lies:
//
//  1. COLD, EMPTY, GENUINELY OFFLINE. No snapshot, no server, no network:
//     "Nothing in the stash", not a spinner, not a skeleton, not a hang. This
//     is the state a screen can fake by accident — a loader waiting on a fetch
//     that will never answer looks identical to a loader that will resolve.
//  2. THE REPLAY ORDER SURVIVES AN OFFLINE EDIT. Two queue pushes while down
//     come back in push order with their client op ids, or the server applies
//     the second one against a state the first never produced.
//  3. NO OPTIMISTIC ROW. A write that goes out at `reconnecting` must not
//     conjure a row, or the screen shows loot that does not exist and the row
//     appears twice when the diff lands.
//  4. A STALE RECONCILE MUST NOT CLOBBER A PENDING QUEUE PUSH. The spec's
//     hardest case (D1: "the claim-log read is reconciled by monotonic
//     `created_at` and NOT by `fetched_at`") and the one a naive cache rewrite
//     gets wrong.
//
// No fake timers: the store's queue flush is promise-driven, and the suite
// drives the transport state machine directly rather than racing a clock.

import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { render, screen, cleanup, within, fireEvent, waitFor } from '@testing-library/svelte';

import StashScreen from '../../src/lib/party/StashScreen.svelte';
import { createStashState } from '../../src/lib/party/stash.js';
import { createSync } from '../../src/lib/sync/index.js';
import { fakeClock, mockSockets, fakeStorage } from '../sync/fakes.js';


/**
 * Every wait in this file is bounded, and the bound is stated rather than left
 * to the default. `waitFor` (from @testing-library/svelte, the helper every
 * suite on main already uses) retries until its assertion passes, and nothing
 * in this file's config stops it — an unbounded wait whose condition never
 * becomes true fails as a worker SIGABRT after twenty minutes of spinning
 * rather than as a failed assertion, which is a worse report than a red test.
 * 2s is a fifth of the per-test budget and many times the few microtasks a
 * fan-out frame needs to land, so reaching it means the projection never moved:
 * exactly the thing each assertion here is for.
 */
const TEST_WAIT_TIMEOUT = 2000;


// A party socket URL carries the party id — the suite asserts the stash rides
// that one socket rather than a second endpoint.
const PARTY_WS = 'wss://house.invalid/ws/party/1';
globalThis.WebSocket = class {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
};

afterEach(cleanup);

const STASH = (name) => ({ kind: 'stash', party_id: 1, item_name: name });

/**
 * Choose an option in a mounted <select>. jsdom fires no change event from a
 * plain value assignment and Svelte's bind:value commits only on `change`, so a
 * picker driven without it silently keeps its first option — which reads as "the
 * component ignored my input" and is in fact the harness.
 */
async function choose(select, value) {
  await fireEvent.change(select, { target: { value } });
}
const BANK = () => ({ kind: 'bank', party_id: 1 });
// The designation rides the PARTY field, in its `quartermaster` subfield — the
// shape the contract pin accepts and the projection reads (stash.js matches
// `kind === 'party' && field === 'quartermaster'`), and the value is an object
// carrying the character id, not a bare number.
const QM = () => ({ kind: 'party', party_id: 1, field: 'quartermaster' });
const QM_VALUE = (id) => ({ quartermaster_character_id: id });

const PLAYER_A = { sub: 'dev-sub-a', role: 'player' };

/** @{item_name: quantity} — the rows by name, the shape these assertions read. */
const quantities = (rows) => Object.fromEntries(rows.map((r) => [r.item_name, r.quantity]));
// The transfer picker needs a real roster to pick from; character 7 is owned by
// this seat so the transfer path is the shipped one (a non-owner's character
// would be refused by the ownership rule, not the quantity rule).
const ROSTER = [{ character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } }];

// ---------------------------------------------------------------------------
// 1. Cold, empty, offline — through the mounted screen.
// ---------------------------------------------------------------------------

test('cold offline boot with an empty cache renders "Nothing in the stash", not a spinner', async () => {
  // Three empties at once: no snapshot in IndexedDB, no server, no network.
  // The screen's only honest answer is the empty state.
  const stash = createStashState({ sync: fakeSync(), partyId: 1, you: PLAYER_A, roster: [] });
  assert.deepEqual(stash.rows(), [], 'no rows exist to render');
  assert.deepEqual(stash.claims(), [], 'no claims exist to render');

  render(StashScreen, { props: { stash, roster: [] } });

  const region = screen.getByRole('region', { name: /stash/i });
  assert.match(region.textContent, /nothing in the stash/i);
  // A loader would also be a lie here: there is nothing it could be waiting
  // for. Assert the absence of every spinner idiom, not just the emptiness.
  assert.equal(region.querySelector('[role="status"][aria-busy="true"]'), null, 'no aria-busy loader');
  assert.equal(region.querySelector('.skeleton, .spinner, [data-loading]'), null, 'no skeleton or spinner');
  // And the claim log states its own empty, not a header over a blank hole.
  assert.match(screen.getByRole('region', { name: /claim history/i }).textContent, /no movements recorded/i);
});

test('the claim log is absent (never an empty header) and the add form still appears offline', async () => {
  // The empty state is the one the epic's spec names for the stash itself:
  // "renders the 'Nothing in the stash' empty state, not a spinner" — and the
  // add form is still offered offline, because the write is queued rather than
  // refused (spec: an offline write "queues for send").
  const stash = createStashState({ sync: fakeSync(), partyId: 1, you: PLAYER_A, roster: [] });
  render(StashScreen, { props: { stash, roster: [] } });

  const region = screen.getByRole('region', { name: /stash/i });
  assert.ok(within(region).getByRole('button', { name: /add to stash/i }), 'the form is present offline');
  assert.equal(within(region).queryAllByRole('listitem').length, 0, 'no phantom rows');
});

// ---------------------------------------------------------------------------
// 2. Replay order across a disconnect.
// ---------------------------------------------------------------------------

test('two queue pushes made offline replay in push order with their client op ids', async () => {
  const h = harness();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  h.goOffline();

  // A design comment in stash.js asserts this survives a flush; nothing
  // asserted it until now, and "the queue preserves order and the op id" is
  // exactly the kind of property a comment can be wrong about.
  stash.addRow({ item_name: 'First', quantity: 1 });
  stash.addRow({ item_name: 'Second', quantity: 2 });

  const queued = h.queue();
  assert.equal(queued.length, 2, 'both writes are held while the link is down');
  // `stash_new` is the mint: the row's identity IS its name, so the frame
  // carries no `op` discriminator (contract D1). Order is read off the names.
  assert.deepEqual(
    queued.map((op) => op.value.item_name),
    ['First', 'Second'],
    'push order preserved — the server applies them in this order',
  );
  assert.equal(queued[0].op_id, 'op-1', 'first push carries the first op id');
  assert.equal(queued[1].op_id, 'op-2', 'second carries the second, not a repeat');
});

test('an offline add then sell replays in the order the user did them', async () => {
  // Order matters beyond cosmetics: selling something the server has never
  // seen is refused. If the queue replayed the sell first, the user's own
  // sequence would be refused by the server they were following.
  const h = harness();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  h.goOffline();

  // A mint followed by a draw (the shipped additive op) — the sequence the
  // order actually protects: selling/drawing something the server has never
  // seen is refused, so if the queue replayed the draw first the user's own
  // sequence would be refused by the server they were following.
  //
  // `sell` itself is lane L3's and the store refuses it locally until the
  // contract fixture carries it, so no sell frame can be on any wire here —
  // that refusal is pinned in the store suite. Order is proven on the pair that
  // ships, and the label function names which frame is which.
  stash.addRow({ item_name: 'Chained', quantity: 5 });
  stash.adjustQty('Chained', -2);

  const label = (value) => (value.op === 'qty' ? 'draw' : value.op === 'meta' ? 'meta' : `mint:${value.item_name}`);
  assert.deepEqual(h.queue().map((op) => label(op.value)), ['mint:Chained', 'draw']);

  // And the order survives the replay itself, not just the holding. Read it off
  // sentAll(): the tab opened no socket before this link, and after a reconnect
  // the newest socket is not where the earlier frames live.
  await h.goLive();
  await waitFor(() => {
    const writes = h.sentAll().filter((frame) => frame.t === 'write');
    assert.equal(writes.length, 2, 'both held frames ride the reconnect');
    assert.deepEqual(
      writes.map((frame) => label(frame.value)),
      ['mint:Chained', 'draw'],
      'the wire sees them in the order the user did them',
    );
  }, { timeout: TEST_WAIT_TIMEOUT });
});

test('reconnect replays the held queue once and once only', async () => {
  const h = harness();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  h.goOffline();

  stash.addRow({ item_name: 'Once', quantity: 1 });
  stash.addRow({ item_name: 'Twice', quantity: 1 });

  await h.goLive();
  await waitFor(
    () => assert.equal(h.sent().length, 2, 'everything held goes out on the reconnect'),
    { timeout: TEST_WAIT_TIMEOUT },
  );
  // What empties the durable queue is the ACK, not the send (queue.js removes
  // ops only in dequeue(), which settleAck calls). A replay that is not yet
  // acked MUST still be held — if this read 0, a write would have been dropped
  // on the floor and the offline queue would be a lie.
  assert.equal(h.queue().length, 2, 'unacked ops stay held — the queue is not emptied by sending');
  await h.ackWrite('op-1', 11);
  await h.ackWrite('op-2', 12);
  assert.equal(h.queue().length, 0, 'and the queue drains once the server has settled them');

  // The "and once only" half, with the settled ops genuinely settled: a link
  // that drops and returns after the server has acked must put nothing back on
  // the wire. This is the at-least-once duplicate the spec's op_id idempotency
  // is meant to cover — but the client should not be manufacturing the
  // duplicates in the first place.
  h.goOffline();
  await h.goLive();
  assert.equal(
    h.sentAll().filter((frame) => frame.t === 'write').length,
    2,
    'a reconnect after the acks resends nothing already settled',
  );
});

// ---------------------------------------------------------------------------
// 3. A write accepted at `reconnecting` must not fabricate a row.
// ---------------------------------------------------------------------------

test('a queued write echoes into the view immediately, and lands exactly once', async () => {
  // degraded-mode.md §4 is the contract and it says the opposite of "wait for
  // the ack": "a write applies to view state immediately, tagged `pending`
  // (carrying `op_id`). The tag clears on the ack." design.md D6 excludes only
  // the CLAIM LOG from local echo, and says so while explicitly naming the write
  // targets as covered. So the mint shows at once, and the claim this suite
  // exists to defend is the second half: when the server's diff lands, the row
  // is ONE row, not the guess plus the truth.
  const h = harness();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  h.goOffline();

  stash.addRow({ item_name: 'Ghost', quantity: 3 });

  assert.equal(h.queue().length, 1, 'the write is genuinely held, not dropped');
  assert.deepEqual(
    quantities(stash.rows()),
    { Ghost: 3 },
    'the mint is echoed into the view immediately, per §4',
  );
  render(StashScreen, { props: { stash, roster: [] } });
  const region = () => screen.getByRole('region', { name: /stash/i });
  assert.match(region().textContent, /Ghost/, 'and the user can see what they just queued');
  assert.doesNotMatch(region().textContent, /nothing in the stash/i, 'the echo is not blanked by the empty state');

  // ...and it appears exactly once when the server's diff actually lands. The
  // tab was never connected in this test, so there is no socket to deliver on
  // until the link is brought up (which also replays the held write — see the
  // queue-length assertion below, which counts that replay).
  await h.goLive();
  h.deliver(STASH('Ghost'), { item_name: 'Ghost', quantity: 3, bulk: null, notes: null }, 11);
  await waitFor(() => assert.equal(stash.rows().length, 1, 'one row, from the server'), { timeout: TEST_WAIT_TIMEOUT });
  await waitFor(() => {
    const list = within(region()).getAllByRole('listitem');
    assert.equal(list.length, 1, 'exactly one row on screen — never the guess plus the truth');
    assert.equal(list[0].getAttribute('aria-label'), 'Ghost');
  }, { timeout: TEST_WAIT_TIMEOUT });
});

test('an offline bank entry echoes the projected balance, and the server has the last word', async () => {
  // Same rule one level up, and the same §4 sentence: the delta folds onto the
  // balance this seat read, so the user sees 15 gp of their own doing while the
  // link is down. What the client must NEVER do is render the raw `{gp_delta:…}`
  // frame or invent a balance the server never stated -- that is the shape the
  // projection's delta guards exist for.
  const h = harness();
  await h.goLive();
  h.deliver(BANK(), { gp: 10, sp: 0, cp: 0 }, 5);
  h.deliver(STASH('Coinbox'), { item_name: 'Coinbox', quantity: 2, bulk: null, notes: null }, 6);
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  h.goOffline();

  // `sell` is lane L3's and the store refuses it locally until the contract
  // fixture carries it, so the local-echo claim is proven on the shipped bank
  // op -- same rule, one call away.
  stash.adjustBank({ gp: 5, sp: 0, cp: 0 });
  assert.deepEqual(
    stash.bank(),
    { gp: 15, sp: 0, cp: 0 },
    'the queued delta folds onto the balance the seat read — the §4 echo',
  );

  // The link returns, the held delta replays, and the server's committed
  // balance is what lands on the screen. Where the client's arithmetic and the
  // server's disagree, the server's wins and the echo follows it -- which is the
  // half that matters: the client never gets to keep its guess.
  assert.equal(h.queue().length, 1, 'the entry is held while the link is down');
  const opId = /** @type {string} */ (h.queue()[0].op_id);
  await h.goLive(); // the held frame rides the reconnect's drain
  // The ack carries the op's version (7); the committed balance is the NEXT
  // statement on the global sequence (8). Delivering it at 7 would be
  // arithmetically impossible — §4: "the global `field_version_seq` is the only
  // order", and the ack's version was minted from that same sequence.
  await h.ackWrite(opId, 7);
  h.deliver(BANK(), { gp: 18, sp: 0, cp: 0 }, 8);
  await waitFor(() => assert.deepEqual(stash.bank(), { gp: 18, sp: 0, cp: 0 }), { timeout: TEST_WAIT_TIMEOUT });
});

// ---------------------------------------------------------------------------
// 4. A stale reconcile must not clobber a pending queue push.
// ---------------------------------------------------------------------------

test('a server read does not clobber a pending push, and the queued intent still replays', async () => {
  // D1: the server's bank balance is an authoritative single-row read, so a
  // response can arrive carrying a balance from before the local user's queued
  // withdrawal. Two things must both hold, and they pull opposite ways: the
  // PENDING push keeps its place in the queue (a "latest write wins" cache
  // rewrite would drop the user's intent on the floor), and the AUTHORITATIVE
  // statement is what the field's committed value becomes -- the echo folds onto
  // it rather than overriding it. §4: the server is authoritative.
  const h = harness();
  await h.goLive();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });

  // The party's opening balance, read while connected — so there IS a balance
  // for a late response to be stale about.
  h.deliver(BANK(), { gp: 20, sp: 0, cp: 0 }, 30);
  await waitFor(() => assert.deepEqual(stash.bank(), { gp: 20, sp: 0, cp: 0 }), { timeout: TEST_WAIT_TIMEOUT });

  // Go offline and spend. The user's own action is queued, and echoed.
  h.goOffline();
  stash.adjustBank({ gp: -5, sp: 0, cp: 0 });
  assert.equal(h.queue().length, 1, 'the withdrawal is pending');

  // Now the server's next statement lands on the same field.
  h.deliver(BANK(), { gp: 20, sp: 0, cp: 0 }, 31);

  // The pending push must survive it: a cache rewrite keyed on "latest write
  // wins" would drop the user's queued intent on the floor here.
  assert.equal(h.queue().length, 1, 'the pending push is NOT clobbered by the server read');
  assert.deepEqual(
    stash.bank(),
    { gp: 15, sp: 0, cp: 0 },
    'the echo folds onto the server statement: 20 committed, minus the 5 still queued',
  );

  // And when the link returns, the withdraw still replays and the server's
  // next word moves the balance.
  await h.goLive();
  // The first link of this test sent nothing (the withdrawal was written after
  // the link dropped), so "what replayed" is read off the session's frames, not
  // off the newest socket's count — which is 0 until the drain lands and is a
  // `sent()[0]` TypeError once it has never happened.
  await waitFor(() => {
    const writes = h.sentAll().filter((frame) => frame.t === 'write');
    assert.equal(writes.length, 1, 'the pending withdrawal still replays');
    assert.deepEqual(
      writes[0].value,
      { gp_delta: -5, sp_delta: 0, cp_delta: 0 },
      'and replays as the DELTA the user asked for, never as a balance',
    );
  }, { timeout: TEST_WAIT_TIMEOUT });

  // The server's own word moves the balance once the write is settled.
  const opId = /** @type {string} */ (h.queue()[0].op_id);
  await h.ackWrite(opId, 32);
  h.deliver(BANK(), { gp: 15, sp: 0, cp: 0 }, 33);
  await waitFor(() => assert.deepEqual(stash.bank(), { gp: 15, sp: 0, cp: 0 }), { timeout: TEST_WAIT_TIMEOUT });
});

test('a pending quartermaster push is not clobbered by a stale designation read', async () => {
  // The same race on the other single-row field.
  const h = harness();
  await h.goLive();
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });

  h.deliver(QM(), QM_VALUE(7), 40);
  await waitFor(() => assert.equal(stash.quartermasterId(), 7), { timeout: TEST_WAIT_TIMEOUT });

  h.goOffline();
  stash.setQuartermaster(8); // "give it to Becky"
  assert.equal(h.queue().length, 1);

  h.deliver(QM(), QM_VALUE(7), 41); // a stale read of the OLD designation
  assert.equal(h.queue().length, 1, 'the pending push survives the stale read');

  await h.goLive();
  await waitFor(() => assert.equal(h.sent().length, 1), { timeout: TEST_WAIT_TIMEOUT });
  assert.deepEqual(
    h.sent()[0].value,
    { character_id: 8 },
    'the replayed push carries the user\'s choice, not the value the stale read returned',
  );
});

// ---------------------------------------------------------------------------
// One transport for both lanes; and the L3 ceiling holds offline too.
// ---------------------------------------------------------------------------

test('the stash rides the party socket — the store is handed the party transport, not a second one', async () => {
  // The review's second question: a fresh socket would mean a second connection
  // per screen and a second snapshot path.
  //
  // This cannot be asserted by reading `App.svelte`'s text. `openStash` runs
  // only after `boot()` succeeds, and `boot()` reaches the real `createSync`, so
  // no test can reach that function — a source scan would assert a string, not a
  // contract. The contract is about what the store ACCEPTS, so it is asserted
  // where it is enforceable: the store takes the transport by reference and talks
  // to it, and `createSync` is the only module in the app that constructs a
  // socket (`grep -rn "new WebSocket" src/` — one hit, src/lib/sync/index.js).
  const h = harness();
  await h.goLive();
  const before = h.sockets().length;

  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: [] });
  stash.addRow({ item_name: 'Rope', quantity: 2 });
  await waitFor(() => assert.ok(h.sentAll().some((frame) => frame.t === 'write')), { timeout: TEST_WAIT_TIMEOUT });

  assert.equal(h.sockets().length, before, 'the store opened no socket of its own');
  assert.equal(before, 1, 'the tab is on one connection — the session\'s');

  // And the store's frames are riding THAT transport: the op arrived on the
  // socket the session opened, before any reconnect existed.
  const first = JSON.parse(h.firstSocket().sent[0]);
  assert.equal(first.t, 'write');
  // A mint rides the `stash_new` target (contract §3) — the row target carries
  // an `item_name` that does not exist yet, so minting is keyed by party.
  assert.equal(first.target.kind, 'stash_new');

  // The store holds the transport it was handed rather than a copy of it, which
  // is what makes the above the whole claim: same object, same socket.
  assert.equal(stash.sync, h.sync, 'the store keeps the session transport by reference');
  stash.destroy();
});

// --- the quantity bound, offline ------------------------------------------
//
// Where the bound lives, stated plainly so the ownership is not a mystery:
// the client clamps the input and the SERVER is the authority (the bank
// ceiling's own wording in `stash.js`, and design D3). So the bound is the
// dialog's — `max` on the picker plus the submit-time refusal — and the store
// deliberately posts whatever it is handed, because a client-side ceiling
// cannot know the row's true quantity. Proving "the bound holds offline"
// therefore means proving the clamp is in force when the link is down, which
// is exactly when the server cannot catch a bad number.
//
// `transfer`/`sell` are on the wire now (lane L3 merged them: d1ec31b), so a
// legal compound frame DOES ride the queue offline. That is what makes the
// two sides of this test worth having: the over-draw is clamped away and
// nothing queues, the legal draw queues exactly one frame.

test('offline, the picker is bounded by the row and an over-draw never reaches the queue', async () => {
  const h = harness();
  await h.goLive();
  h.deliver(STASH('Rope'), { item_name: 'Rope', quantity: 6, bulk: null, notes: null }, 50);
  const stash = createStashState({ sync: h.sync, partyId: 1, you: PLAYER_A, roster: ROSTER });
  h.goOffline();
  render(StashScreen, { props: { stash, roster: ROSTER } });

  await fireEvent.click(await screen.findByRole('button', { name: /transfer/i }));
  const dialog = await screen.findByRole('dialog');
  assert.equal(
    within(dialog).getByLabelText(/quantity/i).getAttribute('max'),
    '6',
    'the bound comes from the row and is in force with the link down',
  );

  // Both sides of the boundary through the mounted form, differing only in the
  // number: a single fixture cannot tell a bound from a hardcoded attribute.
  await choose(within(dialog).getByRole('combobox', { name: /character|to/i }), '7');
  await fireEvent.input(within(dialog).getByLabelText(/quantity/i), { target: { value: '9' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: /^transfer$/i }));

  assert.equal(h.queue().length, 0, 'the over-draw is clamped before it can be queued');
  // The banner and the book-value hint are `status` elements too; the
  // dialog's own error line is the last one inside it.
  const errors = within(dialog).getAllByRole('status');
  assert.match(
    errors[errors.length - 1].textContent,
    /cannot exceed/i,
    'the clamp says what the bound was, in the dialog the user is in',
  );

  cleanup();
  render(StashScreen, { props: { stash, roster: ROSTER } });
  await fireEvent.click(await screen.findByRole('button', { name: /transfer/i }));
  const second = await screen.findByRole('dialog');
  await choose(within(second).getByRole('combobox', { name: /character|to/i }), '7');
  await fireEvent.input(within(second).getByLabelText(/quantity/i), { target: { value: '6' } });
  await fireEvent.click(within(second).getByRole('button', { name: /^transfer$/i }));

  // The transfer op is on the wire (lane L3 merged it), so the legal draw
  // DOES queue while the link is down — that is the whole point of the
  // outbox — and it queues as exactly one frame. What the clamp bought is
  // that the over-draw never added a second one.
  assert.equal(h.queue().length, 1, 'the legal draw is queued, not dropped');
  assert.deepEqual(h.queue()[0].value, {
    op: 'transfer',
    direction: 'out',
    qty: 6,
    character_id: 7,
  });
});

// ---------------------------------------------------------------------------
// transports — the PRODUCTION sync, driven by the suite's own socket fakes.
// ---------------------------------------------------------------------------

/**
 * A real `createSync` over mock sockets, so the queue, the drain ordering and
 * the state machine under test are the shipping ones. A hand-rolled transport
 * would test a reimplementation of offline behaviour rather than the offline
 * behaviour the app has — the exact fault this suite exists to prevent.
 */
function harness() {
  const clock = fakeClock();
  const mocks = mockSockets();
  let seq = 0;
  const sync = createSync({
    url: PARTY_WS,
    storage: fakeStorage(),
    accountSub: 'dev-sub-a',
    socketFactory: mocks.factory,
    rng: () => 1,
    now: clock.now,
    timers: clock.timers,
    // Op ids come from this factory, so the suite can pin the op id a queued
    // write carries instead of asserting it is "some uuid".
    idFactory: () => `op-${++seq}`,
  });
  const socket = () => mocks.sockets[mocks.sockets.length - 1];
  // The connection's first backoff step (BASE_BACKOFF_MS in connection.js);
  // the fake clock is advanced past it to let a scheduled retry fire.
  const RECONNECT_MS = 2000;
  const SNAPSHOT_FRAME = JSON.stringify({ t: 'snapshot', fields: [], derived: [], snapshot_bytes: 0 });

  /**
   * Push a field the way the server pushes it. `diff` is the steady-state
   * frame a fan-out rides and the wire calls the field `field`, not `target`
   * (`wire-protocol.md` §2, and `index.js`'s own `frame.field`) — a typo here
   * does not throw, it silently mints a field keyed `undefined`, which is how
   * this harness first presented: a crash three layers away in `project()`.
   * `snapshot` is reserved for the connection's opening marker below.
   */
  const deliver = (target, value, version) => {
    socket().message(JSON.stringify({ t: 'diff', field: target, value, version }));
  };
  /** What the client has actually put on the wire, parsed, newest order. */
  /** Frames on the newest socket only — what the latest link actually put out. */
  const sent = () => (socket() ? socket().sent : []).map((raw) => JSON.parse(raw));
  /** Every frame across ALL sockets this session opened, parsed: the whole
   *  conversation, which is what an ack lookup needs once a reconnect has
   *  opened a second socket under a write queued on the first. */
  const sentAll = () => mocks.sockets.flatMap((s) => s.sent.map((raw) => JSON.parse(raw)));
  /**
   * The held queue, with a loud accessor. `sync.queue()` is documented API
   * (index.js's own jsdoc), but a bare `() => sync.queue()` would turn a missing
   * method into `undefined.length === undefined` — and every
   * `assert.equal(x.length, 0)` downstream then passes against a write that
   * never happened. That is exactly how three of these tests were green while
   * broken, so the boundary asserts the method exists and returns an array.
   */
  const queue = () => {
    assert.equal(typeof sync.queue, 'function', 'the sync transport exposes queue()');
    const ops = sync.queue();
    assert.ok(Array.isArray(ops), 'queue() returns an array of held ops');
    return ops;
  };

  return {
    sync,
    clock,
    deliver,
    sent,
    /** Every socket the session opened — the count is the "one socket per tab" claim. */
    sockets: () => mocks.sockets,
    /** The socket the session opened first, before any reconnect existed. */
    firstSocket: () => mocks.sockets[0],
    // A loud queue accessor: `sync.queue()` is the API's documented surface
    // (index.js's own jsdoc), and a plain `() => sync.queue()` would turn a
    // missing method into `undefined.length === undefined`, which then makes
    // every `assert.equal(x.length, 0)` pass against a write that never
    // happened. Fail here, at the boundary, with the name of what is missing.
    queue,
    /**
     * Bring the link up and stay until the drain completes.
     *
     * Two shapes, one path. The first call opens the socket directly; after a
     * `goOffline()` the connection has already scheduled its backoff retry (the
     * app's own reconnect loop), so the clock is advanced and the socket that
     * RETIMER opens is the one driven. Driving the retry rather than calling
     * `connect()` again is deliberate: the retried socket is what the app
     * actually gets when a phone walks back into coverage, and it is where a
     * queue that failed to replay would hide.
     */
    async goLive() {
      if (mocks.sockets.length === 0) {
        sync.connect(); // first link of the session: open it now
      } else {
        clock.advance(RECONNECT_MS); // let the scheduled reconnect fire
      }
      await waitFor(() => {
        const s = socket();
        assert.ok(s && !s.closed, 'a live-or-opening socket exists');
      }, { timeout: TEST_WAIT_TIMEOUT });
      const s = socket();
      s.open();
      // The snapshot frame is the drain gate: the queue replays between its
      // phase markers, and only then does the link report `live` — the exact
      // ordering index.js composes against.
      s.message(SNAPSHOT_FRAME);
      await waitFor(() => assert.equal(sync.connectionState(), 'live'), { timeout: TEST_WAIT_TIMEOUT });
      return s;
    },
    /** Drop the link the way the app does: the browser-offline transition,
     *  which is also what closes the socket and arms the retry. */
    goOffline() {
      sync.disconnect();
      assert.equal(sync.connectionState(), 'offline');
    },
    /** Every frame the client put on the wire across ALL sockets this session
     *  opened, parsed. Used where a test cares about the whole conversation
     *  rather than the newest socket's share of it. */
    sentAll,
    /**
     * Settle a write the way the server does: find the `write` frame carrying
     * that op id and answer it with an `applied` ack.
     *
     * Not decoration. Until a client write's ack lands, the store holds a
     * PENDING ECHO of that field, and a server diff cannot leapfrog the echo —
     * that is the optimistic-echo rule the sheet lanes run on too. A test that
     * delivers a fan-out without acking the user's own write therefore reads the
     * echo, not the server, and reports a false failure. So any test that
     * asserts a post-write server value acks the write first.
     */
    async ackWrite(opId, version, serverValue) {
      await waitFor(() => {
        assert.ok(
          sentAll().some((frame) => frame.t === 'write' && frame.op_id === opId),
          `the write for ${opId} is on the wire before it can be acked`,
        );
      }, { timeout: TEST_WAIT_TIMEOUT });
      // Ack on the socket that CARRIED the write. After a reconnect there are
      // two sockets in the array, and answering on the newest is answering a
      // frame that was never sent there — the op then stays held forever, and
      // the test reads that as a product bug rather than its own wiring.
      const carrier = mocks.sockets.find((s) =>
        s.sent.some((raw) => {
          const frame = JSON.parse(raw);
          return frame.t === 'write' && frame.op_id === opId;
        }),
      );
      assert.ok(carrier, `some socket carried the write for ${opId}`);
      // The ack frame the server actually builds (src/sync/protocol.rs:701) is
      // `{t, op_id, outcome, version}` — it carries NO value, and this stays
      // byte-faithful to it. `serverValue` is accepted only so a caller can name
      // the value it expects the fan-out to carry; it is deliberately not put on
      // the wire here.
      assert.equal(serverValue, undefined, 'the wire\'s ack frame carries no value (protocol.rs)');
      carrier.message(JSON.stringify({ t: 'ack', op_id: opId, outcome: 'applied', version }));
      // Wait on THIS op, not on "the queue is empty": when a batch is acked in
      // sequence, a bare queue-length assertion blames the wrong op for the
      // previous one's unsettled state.
      await waitFor(() => {
        assert.ok(
          queue().every((held) => held.op_id !== opId),
          `${opId} left the queue once the server settled it`,
        );
      }, { timeout: TEST_WAIT_TIMEOUT });
    },
  };
}

/** A transport with none of the above: for tests that only read empty state. */
function fakeSync() {
  return {
    connect() {}, disconnect() {}, hangUp() {}, write() {},
    state: () => ({}), derived: () => null, isSyncing: () => false,
    snapshotForBoot: () => '{"fields":[]}', queue: () => [],
    connectionState: () => 'offline',
    subscribe() { return () => {}; },
  };
}
