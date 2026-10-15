// E12 Task 6 — the stash screen is REACHABLE. A component that nothing routes
// to is a requirement unbuilt (AGENTS.md: "Sweep for reachability"), and FR-11
// is explicit that the screen joins the party surface for every seat,
// GM included. So the affordance gets a mounted test of its own: the party
// header carries a Stash action, and a seat that may write nothing still gets
// in to look.

import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';

import PartyView from '../../src/lib/party/PartyView.svelte';

afterEach(cleanup);

function fakeSync() {
  const listeners = new Set();
  return {
    connect() {},
    disconnect() {},
    hangUp() {},
    write() {},
    state: () => ({}),
    derived: () => null,
    isSyncing: () => false,
    snapshotForBoot: () => '{"fields":[]}',
    queue: () => [],
    connectionState: () => 'live',
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

function payload(id, name, owner) {
  return {
    character: { id, name, level: 3, class: 'Wizard', owner },
    base_sheet: { identity: { name } },
    vitals: { hp: 20, temp_hp: 0 },
    slots: [],
    inventory: [],
    item_bulk: {},
    item_traits: {},
  };
}

const sessionFor = (you) => ({
  roster: {
    party_id: 1,
    you,
    characters: [payload(7, 'Flinn', 'dev-sub-a')],
  },
  sync: fakeSync(),
  refresh: vi.fn(),
});

test('the party header offers the stash to a player seat', async () => {
  const onopenStash = vi.fn();
  render(PartyView, {
    props: { session: sessionFor({ sub: 'dev-sub-a', role: 'player' }), onopenStash },
  });

  const link = await screen.findByRole('button', { name: /stash/i });
  await fireEvent.click(link);
  assert.equal(onopenStash.mock.calls.length, 1, 'the header action fires the route');
});

test('the GM seat reaches the stash too — FR-11 says every seat, GM included', async () => {
  // Read-only is not the same as absent: the GM may not write a single row but
  // must still see the party's loot. A header that hid the link from the GM
  // would satisfy "no write affordances" by deleting the screen, which is not
  // what the rule asks for.
  const onopenStash = vi.fn();
  render(PartyView, {
    props: { session: sessionFor({ sub: 'dev-sub-gm', role: 'gm' }), onopenStash },
  });

  const link = await screen.findByRole('button', { name: /stash/i });
  await fireEvent.click(link);
  assert.equal(onopenStash.mock.calls.length, 1, 'the GM gets in to look');
});
