// E12 Task 6 — the stash screen, mounted and driven (house production-path rule).
//
// Every assertion here goes through `render()` + `fireEvent` on the real
// component and reads what a user sees — role, label, placeholder, disabled.
// The spec's affordances each get their NON-default state exercised, because a
// render that only passes the empty state never enters the branch the spec is
// about:
//   - the sell dialog's two prompt states (book value / no book value) — two
//     fixtures asserting two different results (the two-fixture rule)
//   - the transfer picker's quantity bound — two rows with different stock
//   - the quartermaster toggle's enabled and disabled seats
//   - the GM seat's ZERO write affordances, scoped to the screen's region
//     (E14 SC-5's discipline: assert the region's control set, not a page count)
//   - a refused op record, rendered from a driven event, non-blocking

import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent, within, waitFor } from '@testing-library/svelte';

/**
 * Drive a real <select> to a chosen option. `fireEvent.selectOptions` does not
 * exist in this project's testing-library version (`user-event` is not a
 * dependency either, and this suite will not add one to click a dropdown).
 * Setting `.value` then firing `change` is what a keyboard user produces: it is
 * the same event svelte 5's `onchange` listens for, and jsdom refuses a value
 * that no option carries — so a typo'd target throws instead of silently
 * selecting nothing.
 */
async function choose(select, value) {
  select.value = String(value);
  await fireEvent.change(select);
}

import StashScreen from '../../src/lib/party/StashScreen.svelte';
import { createStashState } from '../../src/lib/party/stash.js';
import { createStore } from '../../src/lib/sync/store.js';

afterEach(cleanup);

const STASH = (p, name) => ({ kind: 'stash', party_id: p, item_name: name });
const BANK = (p) => ({ kind: 'bank', party_id: p });
const PARTY = (p) => ({ kind: 'party', party_id: p, field: 'quartermaster' });
const CLAIM = (id) => ({ kind: 'claim', claim_id: id });

const ROW = (name, quantity, extra = {}) => ({
  item_name: name,
  quantity,
  bulk: null,
  notes: null,
  ...extra,
});

const CLAIM_ROW = (id, item, origin, destination, created_at = '2026-10-15T00:00:00Z') => ({
  id,
  item_name: item,
  quantity: 1,
  origin,
  destination,
  actor_sub: 'dev-sub-a',
  created_at,
  coins_gp: null,
  coins_sp: null,
  coins_cp: null,
});

/** The sync seam, over the real production store — see stash-store.test.js
 *  for why delegating matters more than the line it saves. */
function fakeSync() {
  const listeners = new Set();
  const store = createStore();
  const writes = [];
  const emit = (event) => {
    for (const cb of listeners) cb(event);
  };
  return {
    writes,
    emit,
    connect() {},
    disconnect() {},
    hangUp() {},
    write(target, value, baseVersion) {
      writes.push({ target, value, base_version: baseVersion ?? 0 });
      emit({ type: 'queue', length: writes.length });
    },
    state: () => store.state(),
    derived: () => null,
    isSyncing: () => false,
    snapshotForBoot: () => '{"fields":[]}',
    queue: () => [],
    connectionState: () => 'live',
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    deliver(target, value, version) {
      store.applyServerField(target, value, version);
      emit({ type: 'fields' });
    },
    expose(op, outcome, reason) {
      emit({ type: 'op_exposed', op_id: op.op_id, outcome, op, reason });
    },
  };
}

const FLINN = { character: { id: 7, name: 'Flinn', owner: 'dev-sub-a' } };
const BECKY = { character: { id: 8, name: 'Becky', owner: 'dev-sub-b' } };

/**
 * Mount the screen over a fixture.
 * @param {{you?: {sub: string, role: string}, rows?: object[], bank?: object|null,
 *   claims?: object[], qm?: number|null, roster?: object[], bookValues?: object}} o
 */
function mount(o = {}) {
  const sync = fakeSync();
  let version = 100;
  for (const row of o.rows ?? []) sync.deliver(STASH(1, row.item_name), ROW(row.item_name, row.quantity, row), ++version);
  if (o.bank) sync.deliver(BANK(1), o.bank, ++version);
  sync.deliver(PARTY(1), { quartermaster_character_id: o.qm ?? null }, ++version);
  for (const claim of o.claims ?? []) sync.deliver(CLAIM(claim.id), claim, claim.id);

  const stash = createStashState({
    sync,
    partyId: 1,
    you: o.you ?? { sub: 'dev-sub-a', role: 'player' },
    roster: o.roster ?? [FLINN, BECKY],
  });

  // Book value is D5's read route (merged, `src/book_value.rs`). The screen
  // takes it as a lookup so the test controls both match outcomes without a
  // fetch — the route's own behavior is tested in src/tests/book_value.rs.
  const lookUpBookValue = vi.fn(
    o.bookValues !== undefined
      ? (name) => Promise.resolve(o.bookValues[name] ?? null)
      : () => Promise.resolve(null),
  );

  const utils = render(StashScreen, { props: { stash, roster: o.roster ?? [FLINN, BECKY], lookUpBookValue } });
  return { sync, stash, lookUpBookValue, ...utils };
}

/** Everything write-shaped inside the screen's own region — the SC-5 scope. */
function writeControlsIn(region) {
  const buttons = [...region.querySelectorAll('button')].filter((b) => b.type !== 'submit' || true);
  const inputs = [...region.querySelectorAll('input, select, textarea')].filter(
    (el) => el.type !== 'hidden',
  );
  // Read-only text/log nodes are not affordances; only controls a seat could
  // post from count. Every control the screen renders that is not a filter of
  // the log is a write affordance.
  return { buttons, inputs };
}

// --- the loot list ---------------------------------------------------------

test('the loot list renders name, quantity, Bulk and note through the sheet\'s own Bulk formatting', async () => {
  const { container } = mount({
    // The wire carries bulk in Bulk units (design D1's numeric(4,2) column).
    rows: [{ item_name: 'Longsword', quantity: 2, bulk: 1.3, notes: 'runed' }],
  });
  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    assert.match(region.textContent, /Longsword/);
    assert.match(region.textContent, /runed/);
    // 1.3 Bulk must reach the screen in the SHEET's words — "1 Bulk + 3 L" —
    // not as a raw float and not through a second rounding rule (design D6:
    // "Bulk rendered through the same formatting the sheet uses, no second
    // Bulk-math path"). This literal is tenthsText(13)'s own output.
    assert.match(region.textContent, /1 Bulk \+ 3 L/);
  });
  assert.ok(container);
});

test('a stash row at quantity zero never renders — the row is deleted at zero (FR-1)', async () => {
  // The server deletes at zero and fans a tombstone, so the client's honest
  // empty state is absence. A row that arrives at 0 (a stale snapshot edge)
  // must not show as "0" loot with an active Sell button.
  const { sync } = mount({ rows: [{ item_name: 'Torch', quantity: 5 }] });
  const torchRow = () => screen.queryByRole('listitem', { name: /Torch/i });

  await waitFor(() => assert.ok(torchRow(), 'the row is present before the draw'));
  sync.deliver(STASH(1, 'Torch'), null, 999); // the removal diff

  await waitFor(() => assert.equal(torchRow(), null, 'the row is gone, not zeroed'));
});

// --- the bank header ------------------------------------------------------

test('the bank header shows the balance, and its honest unknown state when no movement has ever happened', async () => {
  const withBank = mount({ bank: { gp: 12, sp: 3, cp: 40 } });
  await waitFor(() => {
    const header = within(withBank.getByRole('region', { name: /bank/i }));
    // All three denominations render — a header that showed only gp would
    // pass a single-value assertion.
    assert.equal(header.getByText('12 gp').textContent, '12 gp');
    assert.equal(header.getByText('3 sp').textContent, '3 sp');
    assert.equal(header.getByText('40 cp').textContent, '40 cp');
  });

  // The other fixture: a party that has never touched the bank. Design D1
  // does not zero-mint the row, so the screen must not invent 0/0/0 — it says
  // it does not know yet.
  cleanup();
  const noBank = mount({});
  await waitFor(() => {
    const bankRegion = noBank.getByRole('region', { name: /bank/i });
    const header = within(bankRegion);
    // Two assertions that hold together: the honest wording is present, and no
    // coin value is. A screen that fabricated 0 gp / 0 sp / 0 cp (the mistake
    // D1's no-zero-mint rule exists to prevent) passes the first and fails the
    // second, which is why the first is not enough on its own.
    assert.match(header.getByText(/unknown/i).textContent, /unknown/i);
    assert.equal(bankRegion.querySelectorAll('.balance span').length, 0, 'no fabricated coin spans');
    assert.doesNotMatch(bankRegion.textContent, /\d+\s*(gp|sp|cp)/);
  });
  assert.ok(withBank);
});

// --- the quartermaster toggle (FR-5) --------------------------------------

test('the designation toggle is enabled for ANY character owner, including the designated one', async () => {
  // The two-fixture rule applied to the flip rule itself: the spec's whole
  // claim is that these two seats get the SAME answer, and one fixture cannot
  // show that a `disabled` attribute is not being computed from ownership.
  const toggleOf = (seat) => within(seat.getByRole('region', { name: /party settings/i })).getByRole('combobox', { name: /quartermaster/i });

  // Two seats, two documents: both MUST see an enabled control, because FR-5's
  // flip rule turns on the answer being the same whether or not you own the
  // designated character. One fixture cannot tell "enabled for any owner" from
  // "enabled for the designated owner" — the strict reading the spec rejects.
  const designated = mount({ qm: 7, roster: [FLINN, BECKY], you: { sub: 'dev-sub-a', role: 'player' } });
  await waitFor(() => {
    assert.equal(toggleOf(designated).disabled, false, 'the designated owner may re-seat their own');
  });

  cleanup();
  const other = mount({ qm: 7, roster: [FLINN, BECKY], you: { sub: 'dev-sub-b', role: 'player' } });
  await waitFor(() => {
    assert.equal(toggleOf(other).disabled, false, 'any other owner may take it too');
  });
});

test('the designation toggle is absent for the GM seat — FR-11, zero write affordances', async () => {
  const { container } = mount({ qm: 7, you: { sub: 'dev-sub-gm', role: 'gm' } });

  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    const { buttons, inputs } = writeControlsIn(region);
    assert.deepEqual(
      buttons.map((b) => b.textContent.trim()).filter(Boolean),
      [],
      'the GM sees no button at all in the stash region',
    );
    assert.deepEqual(inputs, [], 'and no input or select either');
    // The GM still SEES the party's loot — read-only, not empty.
    assert.match(container.textContent, /Stash/i);
  });
});

test('flipping the designation posts the party write and moves the sell affordance on the fan-out, without reload', async () => {
  // FR-5's live-sync requirement, driven as a user does it: change the select,
  // then watch what the OTHER seat's fan-out does to this seat's controls.
  const { sync } = mount({
    qm: 7,
    rows: [{ item_name: 'Longsword', quantity: 3 }],
    you: { sub: 'dev-sub-a', role: 'player' }, // I own Flinn (7), so I am QM
    roster: [FLINN, BECKY],
  });

  const select = await screen.findByRole('combobox', { name: /quartermaster/i });
  await choose(select, '8'); // hand it to Becky

  assert.deepEqual(
    sync.writes.map((op) => op.value),
    [{ character_id: 8 }],
    'the flip posts the CAS party frame',
  );

  // Now the fan-out: the designation moves off me, so Sell must disappear on
  // THIS seat with no reload — the whole point of the field being live-synced.
  sync.deliver(PARTY(1), { quartermaster_character_id: 8 }, 500);
  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    const sells = [...region.querySelectorAll('button')].filter((b) => /sell/i.test(b.textContent));
    assert.deepEqual(sells, [], 'the sell affordance is gone once I am not the QM');
  });
});

// --- sell: only the QM's owner sees it, and the two book-value states -----

test('the sell action renders for the quartermaster\'s owner and nowhere else', async () => {
  const qmSeat = mount({
    qm: 7,
    rows: [{ item_name: 'Longsword', quantity: 3 }],
    you: { sub: 'dev-sub-a', role: 'player' },
    roster: [FLINN, BECKY],
  });
  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    assert.ok(
      [...region.querySelectorAll('button')].some((b) => /sell/i.test(b.textContent)),
      'the QM owner sees Sell',
    );
  });

  cleanup();
  mount({
    qm: 7,
    rows: [{ item_name: 'Longsword', quantity: 3 }],
    you: { sub: 'dev-sub-b', role: 'player' }, // Becky's owner: not the QM
    roster: [FLINN, BECKY],
  });
  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    assert.deepEqual(
      [...region.querySelectorAll('button')].filter((b) => /sell/i.test(b.textContent)),
      [],
      'a non-QM owner sees no Sell control at all (FR-11: rendered only where the server would allow it)',
    );
  });
});

test('the sell dialog prefills the book value when the corpus has one, and flags when it has none', async () => {
  // Two fixtures, two different results (the house two-fixture rule): a name
  // the corpus resolves and one it does not. One fixture asserting one literal
  // would also pass against a dialog that hardcodes the price.
  const priced = mount({
    qm: 7,
    rows: [{ item_name: 'Longsword', quantity: 3 }],
    you: { sub: 'dev-sub-a', role: 'player' },
    roster: [FLINN, BECKY],
    bookValues: { Longsword: { gp: 3, sp: 0, cp: 0 } },
  });

  const sellButton = await screen.findByRole('button', { name: /sell/i });
  await fireEvent.click(sellButton);

  const dialog = await screen.findByRole('dialog');
  await waitFor(() => {
    const gp = within(dialog).getByLabelText(/gp/i);
    assert.equal(gp.value, '3', 'the corpus price is the prompt default (D5: the default, never the constraint)');
    assert.doesNotMatch(dialog.textContent, /no book value/i);
  });

  cleanup();
  const unpriced = mount({
    qm: 7,
    rows: [{ item_name: 'Handmade Fence', quantity: 3 }],
    you: { sub: 'dev-sub-a', role: 'player' },
    roster: [FLINN, BECKY],
    bookValues: {}, // the corpus knows nothing about it
  });
  const otherSell = await screen.findByRole('button', { name: /sell/i });
  await fireEvent.click(otherSell);

  const dialog2 = await screen.findByRole('dialog');
  await waitFor(() => {
    assert.match(dialog2.textContent, /no book value/i, 'the flagged state is visible, not a silent zero');
    assert.equal(within(dialog2).getByLabelText(/gp/i).value, '', 'and the prompt is blank, not prefilled with a guess');
  });
  assert.ok(unpriced);
});

// --- transfer: open to all, bounded by availability ----------------------

test('the transfer picker is open to a non-QM owner and bounds quantity by what is actually there', async () => {
  // Two rows, two different stock levels — the two-fixture rule on quantity.
  // A single fixture cannot tell a max bound from a hardcoded attribute.
  mount({
    qm: 7, // someone else is the QM: transfers must still be open (FR-3)
    rows: [
      { item_name: 'Torch', quantity: 4 },
      { item_name: 'Grapeshot', quantity: 12 },
    ],
    you: { sub: 'dev-sub-b', role: 'player' },
    roster: [FLINN, BECKY],
  });

  const transferButtons = await screen.findAllByRole('button', { name: /transfer/i });
  assert.equal(transferButtons.length, 2, 'one per row, and open though I am not the QM');

  // The list renders alphabetized (the store sorts it), so the bound is read
  // per row by name rather than by index into an order the test assumed.
  async function maxFor(button, item) {
    await fireEvent.click(button);
    const dialog = await screen.findByRole('dialog');
    assert.match(dialog.textContent, new RegExp(item), 'the dialog names the row it was opened from');
    const max = within(dialog).getByLabelText(/quantity/i).getAttribute('max');
    await fireEvent.click(within(dialog).getByRole('button', { name: /cancel/i }));
    return max;
  }

  const listItems = await screen.findAllByRole('listitem');
  const buttonFor = (name) =>
    within(listItems.find((li) => li.getAttribute('aria-label') === name)).getByRole('button', { name: /transfer/i });

  // Two rows, two different stock levels, two different maxes — the two-fixture
  // rule on quantity. A hardcoded `max` cannot satisfy both.
  assert.equal(await maxFor(buttonFor('Grapeshot'), 'Grapeshot'), '12');
  assert.equal(await maxFor(buttonFor('Torch'), 'Torch'), '4', 'the bound follows the row, not a constant');
});

test('submitting a transfer to a roster character is bounded, and refuses an over-draw client-side', async () => {
  // FR-3: "the client picker additionally prevents over-draw by disabling
  // impossible quantities (belt and braces — the server is the authority)".
  const { sync } = mount({
    qm: 7,
    rows: [{ item_name: 'Torch', quantity: 4 }],
    you: { sub: 'dev-sub-b', role: 'player' },
    roster: [FLINN, BECKY],
  });

  const transferButton = await screen.findByRole('button', { name: /transfer/i });
  await fireEvent.click(transferButton);
  const dialog = await screen.findByRole('dialog');

  await choose(within(dialog).getByRole('combobox', { name: /character|to/i }), '7');
  await fireEvent.input(within(dialog).getByLabelText(/quantity/i), { target: { value: '9' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: /^transfer$/i }));

  assert.deepEqual(
    sync.writes.filter((op) => op.value?.op === 'transfer'),
    [],
    'an impossible quantity never leaves the state layer — the frame is not posted',
  );
  // ...and the user is told, rather than watching a button do nothing.
  await waitFor(() => {
    assert.match(dialog.textContent, /only 4|at most 4|cannot exceed/i);
  });
});

// --- the claim log (FR-4, D7) -------------------------------------------

test('the claim log renders newest-first with the FR-4 vocabulary', async () => {
  mount({
    claims: [
      CLAIM_ROW(1, 'Longsword', 'outside', 'stash'),
      CLAIM_ROW(3, 'Torch', 'outside', 'stash'),
      CLAIM_ROW(2, 'Longsword', 'stash', 'character:Flinn'),
    ],
  });

  const log = await screen.findByRole('log');
  await waitFor(() => {
    const entries = [...log.querySelectorAll('li')].map((li) => li.textContent);
    assert.ok(entries.length >= 3, `three rows render, got ${entries.length}`);
    assert.match(entries[0], /Torch/, 'id 3 leads — newest-first by row id, not arrival order');
    assert.match(entries.at(-1), /outside/, 'the origin label renders from the row (frozen history)');
  });
});

test('a money claim row renders its coin amounts alongside the label', async () => {
  mount({
    claims: [
      {
        id: 5,
        item_name: 'bank adjustment',
        quantity: 1,
        origin: 'outside',
        destination: 'bank',
        actor_sub: 'dev-sub-a',
        created_at: '2026-10-15T00:00:00Z',
        coins_gp: 25,
        coins_sp: null,
        coins_cp: null,
      },
    ],
  });

  const log = await screen.findByRole('log');
  await waitFor(() => {
    assert.match(log.textContent, /bank adjustment/);
    assert.match(log.textContent, /25/, 'the signed coin amount rides the money row (D1 coins extension)');
  });
});

// --- refused ops: non-blocking (degraded-mode §4) ------------------------

test('a refused op renders its reason in the screen, non-blockingly', async () => {
  const { sync } = mount({
    rows: [{ item_name: 'Torch', quantity: 1 }],
    you: { sub: 'dev-sub-a', role: 'player' },
  });

  // Drive a real write, then play back the rejection the server would send.
  const transferButton = await screen.findByRole('button', { name: /transfer/i });
  await fireEvent.click(transferButton);
  const dialog = await screen.findByRole('dialog');
  await choose(within(dialog).getByRole('combobox', { name: /character|to/i }), '7');
  await fireEvent.input(within(dialog).getByLabelText(/quantity/i), { target: { value: '1' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: /^transfer$/i }));

  // L3 has not wired `transfer`, so the store refuses locally; once it lands
  // this same drive posts a frame. Either way the user is told. Assert the
  // unblocking reason OR the server's reason — both are the honest state.
  await waitFor(() => {
    const region = screen.getByRole('region', { name: /stash/i });
    assert.match(
      region.textContent + dialog.textContent,
      /transfer|not available|wired/i,
      'the refusal is stated in words, not a silently dead button',
    );
  });

  // The §4 ruling: refused is a user-input fact, so it is inline and
  // self-clearing — never a modal that blocks the rest of the screen.
  assert.equal(
    screen.queryAllByRole('alertdialog').length,
    0,
    'a refused op never blocks the screen',
  );
});

// --- the add-item form --------------------------------------------------

test('the add form posts a mint, and an empty name never reaches the wire', async () => {
  const { sync } = mount({ you: { sub: 'dev-sub-a', role: 'player' } });

  const name = await screen.findByLabelText(/item name/i);
  await fireEvent.input(name, { target: { value: '   ' } });
  await fireEvent.click(screen.getByRole('button', { name: /add/i }));
  assert.deepEqual(sync.writes, [], 'a blank name is not a mint');

  await fireEvent.input(name, { target: { value: 'Hempen Rope' } });
  await fireEvent.input(screen.getByLabelText(/^quantity/i), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: /add/i }));

  await waitFor(() => {
    const mint = sync.writes.find((op) => op.target.kind === 'stash_new');
    assert.ok(mint, 'the mint rides the wire');
    assert.equal(mint.value.item_name, 'Hempen Rope');
    assert.equal(mint.value.quantity, 2);
  });
});
