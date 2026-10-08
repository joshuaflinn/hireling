import { test } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

import { createRosterState } from '../../src/lib/party/state.js';
import RosterCard from '../../src/lib/party/RosterCard.svelte';
import { targetKey } from '../../src/lib/sync/store.js';

/** A read-half sync fake: state(), derived(), isSyncing(), subscribe(). */
function fakeSync() {
  const listeners = new Set();
  const fields = new Map();
  const outputs = new Map();
  const emit = (event) => {
    for (const cb of listeners) cb(event);
  };
  return {
    state: () => Object.fromEntries(fields),
    derived: (characterId) => outputs.get(characterId) ?? null,
    isSyncing: () => false,
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

/** One roster character: summary + bootstrap vitals. */
function rosterCharacter(id, name, hp = 20) {
  return {
    character: { id, name, level: 3, class: 'Wizard', owner: 'dev-sub-josh' },
    base_sheet: { identity: { name, level: 3, class: 'Wizard' } },
    vitals: { hp, temp_hp: 0 },
    slots: [],
    inventory: [],
  };
}

const BLESS = {
  effect_id: 11,
  name: 'Bless',
  source_name: 'Becky',
  duration_note: '1 minute',
  tracked_manually: false,
};
const GUIDANCE = {
  effect_id: 12,
  name: 'Guidance',
  source_name: 'Becky',
  duration_note: '',
  tracked_manually: true,
};

function seededCard() {
  const sync = fakeSync();
  const roster = { characters: [rosterCharacter(7, 'Flinn', 20)] };
  sync.deliverDerived({
    schema: 'hireling.engine.output.v1',
    character_id: 7,
    render_base: { level: 3, hp_max: 32 },
    effects: [BLESS, GUIDANCE],
  });
  const state = createRosterState({ sync, roster });
  return { sync, state, card: state.cards[0] };
}

test('the card renders name, initial, HP, and the engine chips with sources', async () => {
  const { state, card } = seededCard();
  render(RosterCard, { props: { card, editable: false, onopen: () => {} } });
  await tick();
  screen.getByText('Flinn');
  screen.getByText('F', { selector: '.avatar' });
  screen.getByText(/20/);
  screen.getByText('Bless');
  assert.equal(screen.getAllByText('Becky').length, 2, 'each chip names its source');
  assert.equal(
    screen.queryByText('tracked'),
    null,
    'tracked badge only on tracked_manually chips',
  );
  cleanup();

  // The tracked chip carries the badge — same component, same data.
  const solo = fakeSync();
  const onlyTracked = { characters: [rosterCharacter(7, 'Flinn')] };
  solo.deliverDerived({
    schema: 'hireling.engine.output.v1',
    character_id: 7,
    render_base: { hp_max: 32 },
    effects: [GUIDANCE],
  });
  const soloState = createRosterState({ sync: solo, roster: onlyTracked });
  render(RosterCard, { props: { card: soloState.cards[0], onopen: () => {} } });
  await tick();
  screen.getByText('tracked');
  soloState.destroy();
  state.destroy();
});

test('two fixtures, two initials — the imported name is base data, not a constant', () => {
  const mk = (name) => {
    const sync = fakeSync();
    const state = createRosterState({
      sync,
      roster: { characters: [rosterCharacter(7, name)] },
    });
    render(RosterCard, { props: { card: state.cards[0], onopen: () => {} } });
    const initial = screen.getByText(/^[A-Z?]$/, { selector: '.avatar' }).textContent;
    cleanup();
    state.destroy();
    return initial;
  };
  assert.equal(mk('Flinn'), 'F');
  assert.equal(mk('Becky'), 'B');
});

test('down state: hp 0 renders the down class', async () => {
  const sync = fakeSync();
  const state = createRosterState({
    sync,
    roster: { characters: [rosterCharacter(7, 'Flinn', 0)] },
  });
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  assert.ok(container.querySelector('.card.down'), 'the card carries the down class');
  state.destroy();
});

test('full state: hp equals hpMax renders the full class', async () => {
  const sync = fakeSync();
  sync.deliverDerived({
    schema: 'hireling.engine.output.v1',
    character_id: 7,
    render_base: { hp_max: 20 },
    effects: [],
  });
  const state = createRosterState({
    sync,
    roster: { characters: [rosterCharacter(7, 'Flinn', 20)] },
  });
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  assert.ok(container.querySelector('.card.full'), 'hp === max reads as full');
  state.destroy();
});

test('derived not yet delivered: skeleton bar, no placeholder number, ? initial', async () => {
  const sync = fakeSync(); // no derived output, no fields
  const state = createRosterState({
    sync,
    roster: { characters: [rosterCharacter(7, '')] }, // empty name — never a crash
  });
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  assert.ok(container.querySelector('.skeleton'), 'an honest loading bar');
  assert.equal(
    /\/\s*\d/.test(container.textContent),
    false,
    'no "x / max" placeholder numbers without the engine output',
  );
  assert.equal(
    screen.getByText('?', { selector: '.avatar' }).textContent,
    '?',
    'missing name renders the neutral initial',
  );
  state.destroy();
});

test('live fields move the card: a delivered hp diff re-renders the bar', async () => {
  const { state, card, sync } = seededCard();
  render(RosterCard, { props: { card, onopen: () => {} } });
  await tick();
  screen.getByText(/20/);
  sync.deliver({ kind: 'vitals', character_id: 7, field: 'hp' }, 13, 4);
  await tick();
  screen.getByText(/13/);
  state.destroy();
});

test('tapping the card calls onopen — navigation, not an edit', async () => {
  const { state, card } = seededCard();
  let opened = 0;
  render(RosterCard, { props: { card, onopen: () => (opened += 1) } });
  await tick();
  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  assert.equal(opened, 1);
  state.destroy();
});
