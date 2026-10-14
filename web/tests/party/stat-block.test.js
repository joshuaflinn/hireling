/* global structuredClone */
import { render, screen, cleanup } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';
import { tick } from 'svelte';

import fixture from '../data/base_sheet_reference.json';
/** The reference EngineOutput, verbatim as the wire carries it. */
import engine from '../data/engine_output_reference.json';

import StatBlock from '../../src/lib/party/StatBlock.svelte';
import StatsPane from '../../src/lib/sheet/components/StatsPane.svelte';
import RosterCard from '../../src/lib/party/RosterCard.svelte';
import { createRosterState } from '../../src/lib/party/state.js';

// StatBlock (E14 tasks 3, design D2/D3/D6/D7): SC-1 presence, SC-2 fixture
// tracking (every FR-1 entry's backing field mutated one at a time), SC-3
// sheet equality (absence included), SC-7 skeleton, SC-8 provenance in its
// non-default state, SC-5's unit half. Every data-sourced assertion carries
// both sides of its boundary.

afterEach(cleanup);

/** The reference output: every `applied` list empty — the default-state
 * fixture the two-fixture pairs lean against. */
const FULL = () => structuredClone(engine);

/** The non-default-state fixture (SC-8): Bless's status bonus on the wire's
 * `ac.applied` — the provenance shape of contract §3. */
const BLESS_AC = () => {
  const view = structuredClone(engine);
  view.derived.ac.applied = [
    { type: 'status', value: 2, effect_id: 11, effect_name: 'Bless', source_character_id: 3 },
  ];
  return view;
};

/** The stat block's tile value for a visible label, or null when the entry
 * is absent. Text queries on `StatTile`'s `<div class="k">`/`<div class="v">`
 * (design D7 — StatTile forwards no attributes, so labels are visible text).
 * @param {HTMLElement} container @param {string} label */
function tileValue(container, label) {
  for (const tile of container.querySelectorAll('.tile')) {
    if (tile.querySelector('.k')?.textContent?.trim() === label) {
      return tile.querySelector('.v')?.textContent?.trim() ?? null;
    }
  }
  return null;
}

/** The stat block's key-skill rows, in order.
 * @param {HTMLElement} container */
function keySkillRows(container) {
  const dl = container.querySelector('dl');
  assert.ok(dl, 'the key-skills list renders');
  return [.../** @type {Element} */ (dl).querySelectorAll('dt')].map(
    (dt) => dt.textContent?.trim() ?? '',
  );
}

// ---- SC-1: every FR-1 entry visible, exact fixture values -----------------

test('the full fixture renders every FR-1 entry with its exact value (SC-1)', () => {
  const { container } = render(StatBlock, { props: { view: FULL() } });
  assert.equal(tileValue(container, 'AC'), '16');
  assert.equal(tileValue(container, 'Fort'), '+7');
  assert.equal(tileValue(container, 'Reflex'), '+6');
  assert.equal(tileValue(container, 'Will'), '+7');
  assert.equal(tileValue(container, 'Perception'), '+5');
  assert.equal(tileValue(container, 'Init'), '+5', 'the Init entry shows perception.total');
  assert.equal(tileValue(container, 'Spell DC'), '19', 'the non-innate block DC');
  assert.equal(
    tileValue(container, 'Class DC'),
    '19',
    'class_dc.base 19 renders the entry',
  );
  // The attack note rides the Spell DC tile.
  const spellTile = [...container.querySelectorAll('.tile')].find(
    (tile) => tile.querySelector('.k')?.textContent?.trim() === 'Spell DC',
  );
  assert.match(
    /** @type {HTMLElement} */ (spellTile).querySelector('.x')?.textContent ?? '',
    /^attack \+9$/,
  );
  // Exactly three key skills, in comparator order: the lore (11) first,
  // then deception (10), then arcana beating the other 9s on the name
  // tie-break — lore labels through the partition convention.
  assert.deepEqual(keySkillRows(container), [
    'Mror Holds History Lore',
    'Deception',
    'Arcana',
  ]);
});

// ---- SC-2: the mutation table, one backing field at a time ----------------

test('ac.total moved: the AC entry tracks it', () => {
  const view = FULL();
  view.derived.ac.total += 2;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'AC'), '18');
});

test('each save moved: Fort, Reflex, Will track their fields', () => {
  const view = FULL();
  view.derived.fort.total = 9;
  view.derived.ref.total = 8;
  view.derived.will.total = 5;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Fort'), '+9');
  assert.equal(tileValue(container, 'Reflex'), '+8');
  assert.equal(tileValue(container, 'Will'), '+5');
});

test('perception.total moved: Perception and Init move with it', () => {
  const view = FULL();
  view.derived.perception.total = 3;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Perception'), '+3');
  assert.equal(tileValue(container, 'Init'), '+3', 'FR-4: Init is perception.total');
});

test('spell_dc.total and spell_attack.total moved: value and attack note track', () => {
  const view = FULL();
  view.derived.casters[0].spell_dc.total = 21;
  view.derived.casters[0].spell_attack.total = 11;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Spell DC'), '21');
  const spellTile = [...container.querySelectorAll('.tile')].find(
    (tile) => tile.querySelector('.k')?.textContent?.trim() === 'Spell DC',
  );
  assert.match(
    /** @type {HTMLElement} */ (spellTile).querySelector('.x')?.textContent ?? '',
    /^attack \+11$/,
    'the note is a displayed value and moves with spell_attack.total',
  );
});

test('class_dc.total moved: the displayed value tracks it (hardcoded-DC guard)', () => {
  const view = FULL();
  view.derived.class_dc.total = 22;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Class DC'), '22');
});

test('class_dc.base null: the Class DC entry disappears', () => {
  const view = FULL();
  view.derived.class_dc.base = null;
  view.derived.class_dc.total = null;
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Class DC'), null, 'absence, never a dash or zero');
  assert.equal(container.textContent?.includes('Class DC'), false);
});

test('casters emptied: the Spell DC entry and its attack note disappear', () => {
  const view = FULL();
  view.derived.casters = [];
  const { container } = render(StatBlock, { props: { view } });
  assert.equal(tileValue(container, 'Spell DC'), null);
  assert.equal(container.textContent?.includes('attack'), false, 'the note goes too');
});

test('a key-skill total moved far enough re-selects the three', () => {
  const view = FULL();
  const athletics = view.derived.skills.find((/** @type {any} */ s) => s.name === 'athletics');
  athletics.total = 12;
  const { container } = render(StatBlock, { props: { view } });
  assert.deepEqual(keySkillRows(container), [
    'Athletics',
    'Mror Holds History Lore',
    'Deception',
  ]);
});

test('render_base.hp_max moves the bar the card renders (SC-2, through RosterCard)', async () => {
  // The stat block does not duplicate the HP bar (FR-1) — the field's
  // production path is the card, so the mutation runs through RosterCard.
  const listeners = new Set();
  const outputs = new Map();
  const sync = {
    state: () => ({}),
    derived: (/** @type {number} */ id) => outputs.get(id) ?? null,
    isSyncing: () => false,
    subscribe: (/** @type {(e: any) => void} */ cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    deliverDerived(output) {
      outputs.set(output.character_id, output);
      for (const cb of listeners) cb({ type: 'derived', character_id: output.character_id });
    },
  };
  const roster = {
    characters: [
      {
        character: { id: 7, name: 'Flinn', level: 3, class: 'Wizard', owner: 'dev' },
        vitals: { hp: 20, temp_hp: 0 },
      },
    ],
  };
  sync.deliverDerived({ ...FULL(), render_base: { ...FULL().render_base, hp_max: 32 } });
  const state = createRosterState({ sync: /** @type {any} */ (sync), roster });
  const { container } = render(RosterCard, {
    props: { card: state.cards[0], onopen: () => {} },
  });
  await tick();
  assert.match(container.textContent ?? '', /\/\s*32/, 'the delivered max renders');
  sync.deliverDerived({ ...FULL(), render_base: { ...FULL().render_base, hp_max: 40 } });
  await tick();
  assert.match(container.textContent ?? '', /\/\s*40/, 'the bar tracks hp_max');
  assert.doesNotMatch(container.textContent ?? '', /\/\s*32\b/, 'the old max is gone');
  state.destroy();
});

// ---- SC-3: the same fixture through the sheet's StatsPane -----------------

/** StatsPane mounted the way the sheet tests mount it.
 * @param {any} view */
function mountPane(view) {
  const noop = () => {};
  return render(StatsPane, {
    props: {
      view,
      baseSheet: fixture,
      hp: { value: 20, pending: false },
      temp: { value: 0, pending: false },
      focusCurrent: { value: 0, pending: false },
      focusMax: 1,
      heroPoints: { value: 1, pending: false },
      heroMax: 3,
      ondamage: noop,
      onheal: noop,
      onfull: noop,
      ontemp: noop,
      onfocus: noop,
      onhero: noop,
    },
  });
}

/** The pane tile's value for one of its t-* classes.
 * @param {Element} paneRoot @param {string} cls */
function paneTileValue(paneRoot, cls) {
  const tile = paneRoot.querySelector(`.${cls}`);
  return tile ? (tile.querySelector('.v')?.textContent?.trim() ?? null) : null;
}

test('one engine source, two views: card values equal the sheet values (SC-3)', () => {
  const view = FULL();
  const card = render(StatBlock, { props: { view } }).container;
  const pane = mountPane(structuredClone(view)).container;
  assert.equal(tileValue(card, 'AC'), paneTileValue(pane, 't-ac'));
  assert.equal(tileValue(card, 'Fort'), paneTileValue(pane, 't-fort'));
  assert.equal(tileValue(card, 'Reflex'), paneTileValue(pane, 't-reflex'));
  assert.equal(tileValue(card, 'Will'), paneTileValue(pane, 't-will'));
  assert.equal(tileValue(card, 'Perception'), paneTileValue(pane, 't-perception'));
  assert.equal(tileValue(card, 'Spell DC'), paneTileValue(pane, 't-spelldc'));
  assert.equal(tileValue(card, 'Class DC'), paneTileValue(pane, 't-classdc'));
});

test('absence included: class_dc.base null omits the entry on the card AND the tile on the sheet (SC-3)', () => {
  const view = FULL();
  view.derived.class_dc.base = null;
  view.derived.class_dc.total = null;
  const card = render(StatBlock, { props: { view } }).container;
  const pane = mountPane(structuredClone(view)).container;
  assert.equal(tileValue(card, 'Class DC'), null, 'no card entry');
  assert.equal(paneTileValue(pane, 't-classdc'), null, 'no sheet tile');
});

// ---- SC-8: the provenance hover in its non-default state ------------------

test('empty applied lists render no provenance affordance on any tile', () => {
  const card = render(StatBlock, { props: { view: FULL() } }).container;
  assert.equal(card.querySelectorAll('.prov').length, 0, 'card: no affordances');
  const pane = mountPane(FULL()).container;
  assert.equal(pane.querySelectorAll('.prov').length, 0, 'sheet: no affordances');
});

test('non-empty applied: the breakdown renders on card and sheet AC tiles, engine text verbatim (SC-8)', () => {
  const card = render(StatBlock, { props: { view: BLESS_AC() } }).container;
  const cardTile = [...card.querySelectorAll('.tile')].find(
    (tile) => tile.querySelector('.k')?.textContent?.trim() === 'AC',
  );
  const cardProv = /** @type {HTMLElement} */ (cardTile)?.querySelector('.prov');
  assert.ok(cardProv, 'the card AC tile carries the hover');
  assert.match(
    cardProv.getAttribute('aria-label') ?? '',
    /\+2 status — Bless/,
    'signed(value) type — effect_name, verbatim',
  );
  const pane = mountPane(BLESS_AC()).container;
  const paneProv = /** @type {HTMLElement} */ (pane.querySelector('.t-ac'))?.querySelector('.prov');
  assert.ok(paneProv, 'the sheet AC tile carries the hover');
  assert.match(
    paneProv.getAttribute('aria-label') ?? '',
    /\+2 status — Bless/,
    'the same engine text on both surfaces',
  );
});

// ---- SC-7: loading state ---------------------------------------------------

test('view null: skeleton entries, zero stat numbers (SC-7)', () => {
  const { container } = render(StatBlock, { props: { view: null } });
  assert.equal(container.querySelectorAll('.tile').length, 0, 'no tiles');
  assert.equal(container.querySelectorAll('dt').length, 0, 'no key-skill rows');
  assert.ok(
    container.querySelectorAll('.skeleton').length >= 1,
    'the skeleton entries render',
  );
  assert.doesNotMatch(
    container.querySelector('.stats')?.textContent ?? 'x',
    /[0-9]/,
    'no number anywhere in the block',
  );
});

// ---- SC-5: the unit half — a labelled group of provenance affordances -----

test("the block's root is a labelled group; its focusable set is exactly the provenance affordances (SC-5)", () => {
  // SC-8's non-empty-applied fixture: the affordances must be present to be
  // counted — on an empty fixture this assertion passes vacuously.
  render(StatBlock, { props: { view: BLESS_AC() } });
  const group = screen.getByRole('group', { name: 'Stats' });
  const focusable = group.querySelectorAll(
    '[tabindex], button, input, select, textarea, a[href], [role="button"]',
  );
  assert.ok(focusable.length >= 1, 'the affordances are present on this fixture');
  for (const el of focusable) {
    assert.ok(
      /** @type {HTMLElement} */ (el).closest('.prov'),
      `focusable element is a provenance affordance (or nested in one): ${el.outerHTML.slice(0, 80)}`,
    );
  }
  // No control of its own — nothing that mutates state.
  assert.equal(
    group.querySelectorAll('button, input, select, textarea, a[href]').length,
    0,
    'no real control inside the stat block',
  );
});
