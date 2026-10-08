import { afterEach, beforeEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';

import App from '../../src/App.svelte';
import engineFixture from '../data/engine_output_reference.json';
import baseSheetFixture from '../data/base_sheet_reference.json';

beforeEach(() => {
  localStorage.clear();
  // The socket list is per-test state — a stale instance from an earlier
  // test makes `instances.at(-1)` bind to the wrong socket and every
  // closed/absence assertion vacuous.
  FakeSocket.instances = [];
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const JOSH = { sub: 'dev-sub-josh', display_name: 'Josh', role: 'player' };
const GM = { sub: 'dev-sub-gm', display_name: 'Bruce', role: 'gm' };

function payload(id, name, owner, hp = 20) {
  return {
    character: { id, name, level: 3, class: 'Wizard', owner },
    base_sheet: { ...baseSheetFixture, identity: { ...baseSheetFixture.identity, name } },
    vitals: { hp, temp_hp: 0, money_pp: 0, money_gp: 0, money_sp: 0, money_cp: 0, level_adjust: 0, focus_current: 1, hero_points: 1, daily: { staff_charge_rank: 0, staff_spent: 0, drain_used: false } },
    slots: [],
    inventory: [],
    item_bulk: {},
    item_traits: {},
  };
}

const FLINN = payload(7, 'Flinn', 'dev-sub-josh');
const BECKY = payload(8, 'Becky', 'dev-sub-becky');

/** The server, as far as this file's tests go: /api/me, the roster, logout. */
function stubServer({ me = JOSH, roster, rosterFails = false, secondRoster = null } = {}) {
  const calls = [];
  const handler = async (url) => {
    calls.push(String(url));
    if (url === '/api/me') return { ok: true, status: 200, json: async () => me };
    if (url === '/api/party/roster') {
      if (rosterFails) throw new TypeError('network is gone');
      const body = calls.filter((c) => c === '/api/party/roster').length > 1 && secondRoster ? secondRoster : roster;
      return { ok: true, status: 200, json: async () => body };
    }
    if (url === '/api/auth/logout') return { ok: true, status: 204, json: async () => ({}) };
    throw new Error(`unexpected fetch ${url}`);
  };
  vi.stubGlobal('fetch', vi.fn(handler));
  return calls;
}

/** A WebSocket stand-in the tests drive by hand. */
class FakeSocket {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.sent = [];
    this.closed = false;
    FakeSocket.instances.push(this);
  }
  send(data) {
    this.sent.push(data);
  }
  close() {
    this.closed = true;
    this.onclose?.();
  }
  open() {
    this.onopen?.();
  }
  message(data) {
    this.onmessage?.({ data });
  }
}

/** The snapshot frame the real server sends on connect. */
function snapshotFrame(fields = [], derived = []) {
  return JSON.stringify({ t: 'snapshot', fields, derived, snapshot_bytes: 1 });
}

test('boot: the party screen is home — roster cards render after login', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN, BECKY] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));
  screen.getByText('Becky');
  screen.getByRole('button', { name: 'Log out' });
  assert.equal(
    screen.queryByRole('button', { name: 'Import your character' }),
    null,
    'an owner player sees no import CTA',
  );
});

test('GM boot: the roster renders, zero edit affordances, no CTA', async () => {
  stubServer({ me: GM, roster: { party_id: 1, you: GM, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));
  assert.equal(screen.queryByRole('button', { name: /damage|heal|import|new day/i }), null);
  assert.equal(screen.queryByText('Import your character'), null);
});

test('the roster is live: a snapshot frame moves a card with no drill-in', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));

  // The session opened its own socket at boot (FR-1) — this test never
  // drills in; the roster link does not wait for a sheet to be opened.
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(
    snapshotFrame(
      [{ field: { kind: 'vitals', character_id: 7, field: 'hp' }, value: 12, version: 2 }],
      [{ ...engineFixture, character_id: 7 }],
    ),
  );

  // The card's own HP bar moves: 12 live from the wire, ceiling 32 from
  // the engine output — the same numbers the sheet would show.
  await waitFor(() => screen.getByRole('meter', { name: 'Hit Points 12 of 32' }));
});

test('page hide flushes the trailing boot-cache write — last-known state survives the tab', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));

  // Produce a pending write: drill in and take 5 damage. The throttled
  // writer holds it for 2 s — the flush must land it now, not never.
  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(
    snapshotFrame(
      [{ field: { kind: 'vitals', character_id: 7, field: 'hp' }, value: 20, version: 2 }],
      [{ ...engineFixture, character_id: 7 }],
    ),
  );
  await waitFor(() => screen.getByRole('button', { name: 'Damage 5' }));
  fireEvent.click(screen.getByRole('button', { name: 'Damage 5' }));

  window.dispatchEvent(new Event('pagehide'));
  const cached = JSON.parse(localStorage.getItem('hireling:boot:dev-sub-josh'));
  const field = cached?.snapshot?.fields?.find((/** @type {any} */ f) => f.value === 15);
  assert.ok(field, 'the ≤2 s trailing write must not die with the tab');
  // Flush-only by design: the socket dies with the page, and a bfcache
  // restore reconnects through the normal death path. Page hide must NOT
  // hang the link up — that is logout's job, and a hang-up is one-way.
  assert.equal(socket.closed, false, 'page hide flushes, it does not hang up');
});

test('offline cold boot: the cached roster renders read-only, no error modal', async () => {
  localStorage.setItem(
    'hireling:boot:dev-sub-josh',
    JSON.stringify({ roster: { party_id: 1, you: JOSH, characters: [FLINN] }, snapshot: { fields: [] }, saved_at: 1 }),
  );
  stubServer({ rosterFails: true });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText(/Offline — showing the party as it was last known/));
  screen.getByText('Flinn');
  assert.equal(screen.queryByRole('alert'), null, 'no error theatre');
});

test('drill-in, own card: the sheet renders editable from the session sync', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN, BECKY] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));

  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  // The session sync connects through the real socket factory — feed it
  // the handshake the server would: snapshot + this character's output.
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(
    snapshotFrame(
      [{ field: { kind: 'vitals', character_id: 7, field: 'hp' }, value: 20, version: 2 }],
      [{ ...engineFixture, character_id: 7 }],
    ),
  );
  await waitFor(() => screen.getByRole('button', { name: 'Damage 5' }), { timeout: 2000 });
  assert.ok(screen.getByRole('button', { name: /New Day/ }), 'an edit affordance exists');

  fireEvent.click(screen.getByRole('button', { name: '← The party' }));
  await waitFor(() => screen.getByText('Becky'), 'back lands on the roster');
});

test('drill-in, another member: the sheet renders view-only, zero edit buttons', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN, BECKY] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Becky'));

  fireEvent.click(screen.getByRole('button', { name: /Becky/ }));
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(
    snapshotFrame(
      [],
      [{ ...engineFixture, character_id: 8 }],
    ),
  );
  await waitFor(() => screen.getByText('Becky'));
  await tick();
  assert.equal(
    screen.queryByRole('button', { name: /damage|heal|new day|import/i }),
    null,
    'zero edit affordances in the read-only sheet',
  );
});

test('GM drill-in, member card: the sheet is view-only — zero edit affordances', async () => {
  stubServer({ me: GM, roster: { party_id: 1, you: GM, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));

  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(snapshotFrame([], [{ ...engineFixture, character_id: 7 }]));
  // The panes are really rendered (not skeletons) before the zero-edit
  // assertion can mean anything — SC-3's "all drill-ins" half.
  await waitFor(() => screen.getByRole('meter'));
  assert.equal(
    screen.queryByRole('button', { name: /damage|heal|new day|import/i }),
    null,
    'the GM seat opens every sheet read-only',
  );
});

test('GM drill-in, own character row: the role clause holds — still view-only', async () => {
  // Nothing in the schema forbids a GM owning a character row. The
  // drill-in gate is owner AND player — this fixture is the case only the
  // role clause covers, so this test fails if the clause is dropped.
  const gmOwns = payload(9, 'Bruce', 'dev-sub-gm');
  stubServer({ me: GM, roster: { party_id: 1, you: GM, characters: [gmOwns] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Bruce'));

  fireEvent.click(screen.getByRole('button', { name: /Bruce/ }));
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();
  socket.message(snapshotFrame([], [{ ...engineFixture, character_id: 9 }]));
  await waitFor(() => screen.getByRole('meter'));
  assert.equal(
    screen.queryByRole('button', { name: /damage|heal|new day|import/i }),
    null,
    'owner-match alone must not unlock the GM seat',
  );
});

test('backing out of a drill-in leaves the shell\'s socket alive', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));

  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  await waitFor(() => { assert.ok(FakeSocket.instances.length > 0, "the session opened its socket"); });
  const socket = FakeSocket.instances.at(-1);
  socket.open();

  // The view borrowed the shell's sync — its teardown must not close the
  // tab's one link (one socket per tab, D2).
  fireEvent.click(screen.getByRole('button', { name: '← The party' }));
  await waitFor(() => screen.getByText('Flinn'), 'back lands on the roster');
  assert.equal(socket.closed, false, 'a borrowed sync is not the borrower\'s to close');
});

test('import flow: CTA opens the import view; returning re-fetches and the new card renders', async () => {
  const empty = { party_id: 1, you: JOSH, characters: [] };
  const grown = { party_id: 1, you: JOSH, characters: [FLINN] };
  stubServer({ roster: empty, secondRoster: grown });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByRole('button', { name: 'Import your character' }));

  fireEvent.click(screen.getByRole('button', { name: 'Import your character' }));
  screen.getByRole('heading', { name: 'Import a character' });

  fireEvent.click(screen.getByRole('button', { name: 'Back to the party' }));
  await waitFor(() => screen.getByText('Flinn'));
  assert.ok(
    screen.queryByRole('heading', { name: 'Import a character' }) === null,
    'the import view is gone',
  );
});

test('logout hangs the session up: the tab never reopens the party socket', async () => {
  stubServer({ roster: { party_id: 1, you: JOSH, characters: [FLINN] } });
  vi.stubGlobal('WebSocket', FakeSocket);
  render(App);
  await waitFor(() => screen.getByText('Flinn'));
  await waitFor(() => {
    assert.ok(FakeSocket.instances.length > 0, 'the session opened its socket');
  });
  const socket = FakeSocket.instances[0];
  socket.open();

  fireEvent.click(screen.getByRole('button', { name: 'Log out' }));
  await waitFor(() => screen.getByText('You are signed out.'));
  assert.equal(socket.closed, true, 'the socket dies with the account');

  // Real time on purpose — the reconnect scheduler holds the sync
  // module's module-bound timers, so an injected clock cannot prove this
  // absence. The pre-fix bug reopened a second socket within ~1 s of
  // logout and kept reopening on backoff; this wait is the proof it cannot.
  await new Promise((resolve) => setTimeout(resolve, 1300));
  assert.equal(FakeSocket.instances.length, 1, 'a signed-out tab opens no further socket');
});
