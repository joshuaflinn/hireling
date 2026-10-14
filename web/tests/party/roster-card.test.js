import { test } from 'vitest';
import assert from 'node:assert/strict';
/* global structuredClone, Node */
import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

import { createRosterState } from '../../src/lib/party/state.js';
import RosterCard from '../../src/lib/party/RosterCard.svelte';
import { targetKey } from '../../src/lib/sync/store.js';
import engine from '../data/engine_output_reference.json';

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
    effects: [BLESS, { ...GUIDANCE, tracked_manually: false }],
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

// ---- E14 task 4: the stat block on the card -------------------------------

/** A card whose engine output is the reference fixture — optionally with
 * Bless's status bonus on `ac.applied` (SC-8's non-default state). */
function seededStatCard({ blessAc = false } = {}) {
  const sync = fakeSync();
  const roster = { characters: [rosterCharacter(7, 'Flinn', 20)] };
  const view = structuredClone(engine);
  view.effects = [BLESS, { ...GUIDANCE, tracked_manually: false }];
  if (blessAc) {
    view.derived.ac.applied = [
      { type: 'status', value: 2, effect_id: 11, effect_name: 'Bless', source_character_id: 3 },
    ];
  }
  sync.deliverDerived(view);
  const state = createRosterState({ sync, roster });
  return { sync, state, card: state.cards[0] };
}

test('the card renders the stat block between the bar and the effects strip', async () => {
  const { state } = seededStatCard();
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  const group = screen.getByRole('group', { name: 'Stats' });
  assert.ok(group, 'the stat block mounts, labelled');
  assert.match(group.textContent ?? '', /\b16\b/, 'AC from the engine fixture');
  assert.match(group.textContent ?? '', /Mror Holds History Lore/, 'a key skill renders');
  // Below the bar, above the effects strip (design D2's mount point).
  const bar = container.querySelector('.hp');
  const strip = container.querySelector('.strip');
  assert.ok(bar && strip, 'bar and strip both render');
  assert.ok(
    /** @type {HTMLElement} */ (bar).compareDocumentPosition(group) &
      Node.DOCUMENT_POSITION_FOLLOWING,
    'the stat block sits below the HP bar',
  );
  assert.ok(
    group.compareDocumentPosition(/** @type {HTMLElement} */ (strip)) &
      Node.DOCUMENT_POSITION_FOLLOWING,
    'the effects strip stays below the stat block',
  );
  state.destroy();
});

test('SC-5 scoped: the stat-block group holds only provenance affordances — effects ConditionTips stay outside it', async () => {
  // The two traps a coarser query falls into: a card-level count (fails on
  // any card with effects — the chips carry role="button" ConditionTips) and
  // an empty-region claim (vacuous without non-empty `applied`). This mounts
  // the card on the non-empty-applied fixture WITH effects present, then
  // scopes to the stat block's own group (design D7).
  const { state } = seededStatCard({ blessAc: true });
  let opened = 0;
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => (opened += 1) },
  });
  await tick();
  const group = screen.getByRole('group', { name: 'Stats' });
  // The fixture is live: affordances exist inside the group, and the effects
  // strip really renders role="button" ConditionTips — outside the group.
  assert.ok(
    group.querySelectorAll('[tabindex="0"]').length >= 1,
    'provenance affordances present on this fixture',
  );
  const stripTips = container.querySelectorAll('.strip [role="button"]');
  assert.ok(stripTips.length >= 2, 'effects ConditionTips render outside the group');
  for (const tip of stripTips) {
    assert.equal(
      /** @type {HTMLElement} */ (tip).closest('[role="group"]'),
      null,
      'no effect ConditionTip inside the stat-block group',
    );
  }
  // The region's focusable/interactive set: exactly E6's provenance
  // affordances (a role="note" span or the ConditionTip nested in one).
  const focusable = group.querySelectorAll(
    '[tabindex], button, input, select, textarea, a[href], [role="button"]',
  );
  for (const el of focusable) {
    assert.ok(
      /** @type {HTMLElement} */ (el).closest('.prov'),
      `focusable element is a provenance affordance: ${el.outerHTML.slice(0, 80)}`,
    );
  }
  assert.equal(
    group.querySelectorAll('button, input, select, textarea, a[href]').length,
    0,
    'no control of the unit own inside the stat block',
  );
  // The drill-in remains the card's one navigation control (FR-5).
  fireEvent.click(screen.getByRole('button', { name: /Flinn/ }));
  assert.equal(opened, 1, 'the card-level tap still navigates');
  state.destroy();
});

test('stat block loads honestly: skeleton entries, no numbers, when the view has not landed', async () => {
  const { state } = seededCard(); // view without a derived payload
  render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  const group = screen.getByRole('group', { name: 'Stats' });
  assert.equal(group.querySelectorAll('.tile').length, 0, 'no tiles without the view');
  assert.equal(group.querySelectorAll('dt').length, 0, 'no key skills without the view');
  assert.ok(
    group.querySelectorAll('.skeleton').length >= 1,
    'the stat-block skeleton renders beside the bar skeleton',
  );
  assert.doesNotMatch(group.textContent ?? '', /[0-9]/, 'no invented numbers');
  state.destroy();
});
