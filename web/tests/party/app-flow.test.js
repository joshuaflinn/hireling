import { afterEach, beforeEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';

import App from '../../src/App.svelte';
import engineFixture from '../data/engine_output_reference.json';
import baseSheetFixture from '../data/base_sheet_reference.json';

beforeEach(() => {
  localStorage.clear();
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
    FakeSocket.instances.push(this);
  }
  send(data) {
    this.sent.push(data);
  }
  close() {
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
  await waitFor(() => FakeSocket.instances.length > 0);
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
  await waitFor(() => FakeSocket.instances.length > 0);
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
