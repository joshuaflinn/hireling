import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

import PartyView from '../../src/lib/party/PartyView.svelte';
import RosterCard from '../../src/lib/party/RosterCard.svelte';
import SheetView from '../../src/lib/sheet/SheetView.svelte';
import { createRosterState } from '../../src/lib/party/state.js';
import { targetKey } from '../../src/lib/sync/store.js';

afterEach(cleanup);

/** A read-half sync fake — state(), derived(), isSyncing(), subscribe(). */
function fakeSync(options = {}) {
  const listeners = new Set();
  const fields = new Map();
  const outputs = new Map();
  const emit = (event) => {
    for (const cb of listeners) cb(event);
  };
  return {
    connect() {},
    disconnect() {},
    write() {},
    state: () => Object.fromEntries(fields),
    derived: (characterId) => outputs.get(characterId) ?? null,
    isSyncing: () => options.syncing ?? false,
    snapshotForBoot: () => '{"fields":[]}',
    queue: () => [],
    connectionState: () => 'live',
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    deliver(target, value, version) {
      fields.set(targetKey(target), { target, value, version });
      emit({ type: 'fields' });
    },
    deliverDerived(output) {
      outputs.set(output.character_id, output);
      emit({ type: 'derived', character_id: output.character_id });
    },
  };
}

const BLESS = {
  effect_id: 11,
  name: 'Bless',
  source_name: 'Becky',
  duration_note: '1 minute',
  tracked_manually: false,
};

// The reference EngineOutput (full shape — StatsPane reads it all) retargeted
// at character 7 and carrying one chip for the equality assertion.
import engineFixture from '../data/engine_output_reference.json';
import baseSheetFixture from '../data/base_sheet_reference.json';
const ENGINE_OUTPUT = { ...engineFixture, character_id: 7, effects: [BLESS] };

function payload(id, name, owner, hp = 20) {
  return {
    character: { id, name, level: 3, class: 'Wizard', owner },
    base_sheet: { ...baseSheetFixture, identity: { ...baseSheetFixture.identity, name } },
    vitals: { hp, temp_hp: 0 },
    slots: [],
    inventory: [],
    item_bulk: {},
    item_traits: {},
  };
}

const FLINN = payload(7, 'Flinn', 'dev-sub-josh');
const BECKY = payload(8, 'Becky', 'dev-sub-becky');

/** A party session fixture: roster + the fake sync + a refresh spy. */
function session({ characters, you, syncing = false }) {
  const sync = fakeSync({ syncing });
  return { roster: { party_id: 1, you, characters }, sync, refresh: vi.fn() };
}

test('player, non-empty party: cards in roster order, tap navigates, own card shows the indicator', async () => {
  const s = session({
    characters: [FLINN, BECKY],
    you: { sub: 'dev-sub-josh', role: 'player' },
    syncing: true,
  });
  const opened = [];
  render(PartyView, {
    props: {
      session: s,
      onopenCharacter: (id) => opened.push(id),
      onimport: () => {},
      onlogout: () => {},
    },
  });
  await tick();
  const cards = screen.getAllByRole('listitem');
  assert.match(cards[0].textContent, /Flinn/);
  assert.match(cards[1].textContent, /Becky/);

  // Own card carries the syncing indicator; the other member's does not —
  // the queue is account-local (design D3). (Skeleton bars are statuses
  // too, so the assertion is on the indicator's text, not the role.)
  assert.match(cards[0].textContent, /syncing/i, 'own card: the queue indicator');
  assert.doesNotMatch(cards[1].textContent, /syncing/i, 'other card: none');

  fireEvent.click(screen.getByRole('button', { name: /Becky/ }));
  assert.deepEqual(opened, [8], 'the tap names the tapped character');
});

test('GM: every card renders and is tappable, zero edit affordances anywhere', async () => {
  const s = session({
    characters: [FLINN, BECKY],
    you: { sub: 'dev-sub-gm', role: 'gm' },
  });
  const opened = [];
  render(PartyView, {
    props: { session: s, onopenCharacter: (id) => opened.push(id) },
  });
  await tick();
  assert.equal(screen.getAllByRole('listitem').length, 2, 'the GM sees the whole party');
  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  assert.deepEqual(opened, [7], 'the GM drills in like anyone');

  assert.equal(
    screen.queryByRole('button', { name: /damage|heal|import|new day/i }),
    null,
    'no edit affordance renders',
  );
  assert.equal(
    screen.queryByText('Import your character'),
    null,
    'the import CTA never renders for the GM',
  );
});

test('empty party: the player gets the CTA, the GM gets the note — two roles, two screens', async () => {
  const player = session({ characters: [], you: { sub: 'dev-sub-josh', role: 'player' } });
  render(PartyView, { props: { session: player, onimport: () => {} } });
  await tick();
  screen.getByRole('button', { name: 'Import your character' });
  cleanup();

  const gm = session({ characters: [], you: { sub: 'dev-sub-gm', role: 'gm' } });
  render(PartyView, { props: { session: gm } });
  await tick();
  screen.getByText('The party has no characters yet.');
  assert.equal(screen.queryByRole('button', { name: 'Import your character' }), null);
});

test('characterless player in a non-empty party: header CTA; the owner sees none', async () => {
  const characterless = session({
    characters: [FLINN, BECKY],
    you: { sub: 'dev-sub-bear', role: 'player' },
  });
  render(PartyView, { props: { session: characterless, onimport: () => {} } });
  await tick();
  screen.getByRole('button', { name: 'Import your character' });
  cleanup();

  const owner = session({
    characters: [FLINN, BECKY],
    you: { sub: 'dev-sub-josh', role: 'player' },
  });
  render(PartyView, { props: { session: owner, onimport: () => {} } });
  await tick();
  assert.equal(screen.queryByRole('button', { name: 'Import your character' }), null);
});

test('offline cold boot: the roster renders read-only with the note, no error theatre', async () => {
  const s = session({
    characters: [FLINN],
    you: { sub: 'dev-sub-josh', role: 'player' },
  });
  render(PartyView, { props: { session: s, offlineColdBoot: true, onlogout: () => {} } });
  await tick();
  screen.getByText(/Offline — showing the party as it was last known/);
  screen.getByText('Flinn');
  assert.equal(screen.queryByRole('alert'), null, 'no modal, no alarm');
});

test('chip truth (SC-2): the card and the sheet render the SAME chips from one sync state', async () => {
  const sync = fakeSync();
  sync.deliverDerived(ENGINE_OUTPUT);
  const roster = {
    party_id: 1,
    you: { sub: 'dev-sub-josh', role: 'player' },
    characters: [FLINN],
  };
  const state = createRosterState({ sync, roster });

  const cardView = render(RosterCard, { props: { card: state.cards[0], onopen: () => {} } });
  const cardChips = [...cardView.container.querySelectorAll('.strip .chip')].map((chip) =>
    chip.textContent.trim(),
  );
  cleanup();

  const sheetView = render(SheetView, {
    props: {
      character: FLINN,
      accountSub: 'dev-sub-josh',
      sync,
      editable: true,
    },
  });
  await tick();
  // Scoped to the effects strip — strikes and feats carry .chip too, but
  // the chip-truth contract is about the engine's effects (FR-2).
  const sheetChips = [...sheetView.container.querySelectorAll('.strip .chip')].map((chip) =>
    chip.textContent.trim(),
  );
  assert.deepEqual(cardChips, sheetChips, 'one engine source, two views, one truth');
  assert.deepEqual(cardChips, ['BlessBecky']);
  state.destroy();
  cleanup();
});
