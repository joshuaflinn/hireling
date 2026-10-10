import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup, fireEvent, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';

import SheetView from '../../src/lib/sheet/SheetView.svelte';
import EffectsComposer from '../../src/lib/sheet/components/EffectsComposer.svelte';
import { targetKey } from '../../src/lib/sync/store.js';
import baseSheet from '../data/base_sheet_reference.json';
import engineA from '../data/engine_output_reference.json';

// The E8-residual composer (specs/010-effect-composer): the write surface the
// PRD's Step 3 names. Every test here exercises the production path — the
// mounted sheet wiring (entry point → dialog → composer → state layer → sync
// surface) or the mounted composer — per the house affordance rule. A green
// store unit test is not evidence for this feature.

afterEach(cleanup);

/** The me-shaped bootstrap payload over the reference sheet (id 7). */
const CHARACTER = {
  character: { id: 7, name: baseSheet.identity.name, owner: 'dev-sub-josh' },
  base_sheet: baseSheet,
  vitals: {
    hp: 20, temp_hp: 0, money_pp: 0, money_gp: 5, money_sp: 0, money_cp: 0,
    level_adjust: 0, focus_current: 1, hero_points: 1,
    daily: { staff_charge_rank: 0, staff_spent: 0, drain_used: false },
  },
  slots: [],
  inventory: [],
  item_bulk: {},
  item_traits: {},
};

const PARTY_ID = 1;
const ROSTER = [
  { id: 3, name: 'Josh' },
  { id: 5, name: 'Becky' },
  { id: 7, name: 'Bear' },
];

/** Fixture B — a martial with a DIFFERENT vocabulary: no casters, no Wizard
 *  skills, different lores. Two differing fixtures must yield two differing
 *  pickers, or the picker is a constant (the hardcoded-list failure). */
const engineB = {
  schema: 'hireling.engine.output.v1',
  character_id: 7,
  derived: {
    ac: engineA.derived.ac,
    fort: engineA.derived.fort,
    ref: engineA.derived.ref,
    will: engineA.derived.will,
    perception: engineA.derived.perception,
    speed: engineA.derived.speed,
    class_dc: engineA.derived.class_dc,
    strikes: [{ key: 'longbow', label: 'Longbow', map: 7, damage_expr: 'd8', damage_type: 'P', damage_type_name: 'piercing', traits: [], attack: { base: 8, total: 8, applied: [], suppressed: [] }, damage_flat: { base: 1, total: 1, applied: [], suppressed: [] } }],
    casters: [],
    skills: [
      { name: 'athletics', rank: 2, total: 7, applied: [], suppressed: [] },
      { name: 'intimidation', rank: 1, total: 5, applied: [], suppressed: [] },
      { name: 'lore:mercenary', rank: 1, total: 5, label: 'Mercenary', applied: [], suppressed: [] },
    ],
  },
  effects: [],
  render_base: engineA.render_base,
};

/** A recording session-sync stand-in: writes are captured, events are driven
 *  by hand, derived answers from a fixture map. The sheet must consume IT. */
function fakeSessionSync({ derivedByCharacter = { 7: engineA } } = {}) {
  const writes = [];
  const listeners = new Set();
  return {
    writes,
    /** Drive one sync-surface event through the state layer (the real path
     *  acks and exposures ride). */
    emit(event) {
      for (const cb of [...listeners]) cb(event);
    },
    connect() {},
    disconnect() {},
    hangUp() {},
    write(target, value, baseVersion) {
      writes.push({ target, value, base_version: baseVersion ?? -1 });
    },
    state: () => ({}),
    derived: (characterId) => derivedByCharacter[characterId] ?? null,
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

/** REST face stub for the manager rows (GET /api/parties/1/effects). */
const EFFECT_ROWS = [
  {
    effect_id: 41, name: 'Bless', source_character_id: 7, targets: [3, 5],
    modifiers: [{ type: 'status', stat: 'attack', value: 1 }],
    duration_note: '10 rounds', active: true, version: 1042, tracked_manually: false,
  },
];
function stubFetchEffects() {
  return (url) => {
    if (url === `/api/parties/${PARTY_ID}/effects`) {
      return { ok: true, status: 200, json: () => Promise.resolve(EFFECT_ROWS) };
    }
    return Promise.reject(new Error(`unexpected fetch ${url}`));
  };
}

function optionsOf(select) {
  return [...select.options].map((option) => ({
    value: option.value,
    text: option.textContent.trim(),
  }));
}

function openComposerViaSheet(sync, props = {}) {
  const rendered = render(SheetView, {
    props: {
      character: CHARACTER,
      accountSub: 'dev-sub-josh',
      sync,
      partyId: PARTY_ID,
      roster: ROSTER,
      ...props,
    },
  });
  fireEvent.click(rendered.getByRole('button', { name: 'New effect' }));
  return rendered;
}

// -- the sourced vocabulary (spec FR-C3, the differing-fixtures bar) --------

test('the stat picker is sourced from the delivered engine output: two differing outputs, two differing pickers', () => {
  const a = render(EffectsComposer, {
    props: { open: true, partyId: PARTY_ID, characterId: 7, roster: ROSTER, view: engineA },
  });
  const statA = optionsOf(within(a.container).getByLabelText('Stat'));
  assert.ok(statA.some((o) => o.value === 'skill:lore:underworld'), 'fixture A offers its underworld lore');
  assert.ok(statA.some((o) => o.value === 'spell_attack'), 'fixture A has casters, so the spell family is offered');
  assert.ok(statA.some((o) => o.value === 'attack'), 'per-instance attack offered with strikes present');
  assert.ok(statA.some((o) => o.text === 'Lore: Underworld'), 'labels are readable, not raw wire text');
  cleanup();

  const b = render(EffectsComposer, {
    props: { open: true, partyId: PARTY_ID, characterId: 7, roster: ROSTER, view: engineB },
  });
  const statB = optionsOf(within(b.container).getByLabelText('Stat'));
  assert.ok(statB.some((o) => o.value === 'skill:lore:mercenary'), 'fixture B offers its own lore');
  assert.ok(!statB.some((o) => o.value === 'skill:lore:underworld'), 'A\u2019s lore is not B\u2019s — no constant list');
  assert.ok(!statB.some((o) => o.value === 'spell_attack'), 'B has no casters — no spell family (a hardcoded superset would offer it)');
  assert.ok(!statB.some((o) => o.value === 'skill:arcana'), 'B lacks arcana — the picker follows the sheet');
});

test('the modifier types are the closed four, cited to the engine vocabulary', () => {
  const view = render(EffectsComposer, {
    props: { open: true, partyId: PARTY_ID, characterId: 7, roster: ROSTER, view: engineA },
  });
  const types = optionsOf(view.getByLabelText('Bonus type')).map((o) => o.value);
  assert.deepEqual(types, ['circumstance', 'status', 'item', 'untyped']);
});

// -- the create op through the sheet's write surface (spec FR-C4) -----------

test('apply emits the exact effect_new create op through the sheet\u2019s sync surface', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);

  await tick();
  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Bless' } });
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'attack' } });
  fireEvent.change(view.getByLabelText('Bonus type'), { target: { value: 'status' } });
  fireEvent.input(view.getByLabelText('Value'), { target: { value: '1' } });
  fireEvent.input(view.getByLabelText('Duration note'), { target: { value: '10 rounds' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Josh' }));
  fireEvent.click(view.getByRole('checkbox', { name: 'Becky' }));
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));

  assert.equal(sync.writes.length, 1, 'one op, nothing else');
  const op = sync.writes[0];
  assert.deepEqual(op.target, { kind: 'effect_new', party_id: PARTY_ID });
  assert.deepEqual(op.value, {
    op: 'create',
    name: 'Bless',
    source_character_id: 7,
    targets: [3, 5],
    modifiers: [{ type: 'status', stat: 'attack', value: 1 }],
    duration_note: '10 rounds',
  });
});

test('the modifier type choice rides the op (the stacking input)', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Heroism-ish' } });
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'will' } });
  fireEvent.change(view.getByLabelText('Bonus type'), { target: { value: 'circumstance' } });
  fireEvent.input(view.getByLabelText('Value'), { target: { value: '2' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Bear' }));
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));

  assert.equal(sync.writes.length, 1);
  assert.deepEqual(sync.writes[0].value.modifiers, [{ type: 'circumstance', stat: 'will', value: 2 }]);
});

test('a second modifier row composes the ordered array the create op carries', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Aura' } });
  fireEvent.change(view.getAllByLabelText('Stat')[0], { target: { value: 'fort' } });
  fireEvent.input(view.getAllByLabelText('Value')[0], { target: { value: '1' } });
  fireEvent.click(view.getByRole('button', { name: 'Add modifier' }));
  fireEvent.change(view.getAllByLabelText('Stat')[1], { target: { value: 'will' } });
  fireEvent.input(view.getAllByLabelText('Value')[1], { target: { value: '-2' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Bear' }));
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));

  assert.equal(sync.writes.length, 1);
  assert.deepEqual(sync.writes[0].value.modifiers, [
    { type: 'status', stat: 'fort', value: 1 },
    { type: 'status', stat: 'will', value: -2 },
  ]);
});

test('an incomplete form never emits (client bounds mirror the server\u2019s table)', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  const apply = view.getByRole('button', { name: 'Apply effect' });
  assert.equal(apply.disabled, true, 'no name, no modifiers target — Apply is dark');
  fireEvent.click(apply);
  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Bless' } });
  assert.equal(apply.disabled, true, 'still dark without a target and a modifier stat');
  fireEvent.click(view.getByRole('checkbox', { name: 'Josh' }));
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'attack' } });
  assert.equal(apply.disabled, false, 'a complete form arms Apply');
  fireEvent.click(apply);
  assert.equal(sync.writes.length, 1, 'exactly the valid op left the layer');
});

test('a server denial surfaces its reason inline; an applied ack closes the dialog', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Bless' } });
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'attack' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Josh' }));
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));

  const createKey = targetKey({ kind: 'effect_new', party_id: PARTY_ID });
  sync.emit({
    type: 'op_exposed',
    op_id: 'op-1',
    outcome: 'rejected',
    op: { op_id: 'op-1', target: { kind: 'effect_new', party_id: PARTY_ID }, base_version: 0, value: {}, created_at: '' },
    reason: 'target 5 is not a roster character of this party',
  });
  await tick();
  assert.match(view.container.textContent, /target 5 is not a roster character of this party/, 'the server\u2019s reason, verbatim');
  assert.ok(view.getByRole('button', { name: 'Apply effect' }), 'the dialog stays open on a denial — the caster can re-apply');

  // Reality: after a denial the caster fixes the form and re-applies; the
  // fresh op arms the state layer again, and ITS applied ack closes.
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));
  assert.equal(sync.writes.length, 2, 'the re-apply rode the same create op');
  sync.emit({ type: 'applied', key: createKey });
  await tick();
  await tick();
  assert.doesNotMatch(view.container.textContent, /Apply effect/, 'an applied ack closes the composer');
});

// -- the GM seat (spec FR-C1; PRD: Bruce never renders an edit control) -----

test('the GM seat (or any non-editable sheet) renders no composer entry point', async () => {
  const sync = fakeSessionSync();
  const view = render(SheetView, {
    props: {
      character: CHARACTER,
      accountSub: 'dev-sub-josh',
      sync,
      partyId: PARTY_ID,
      roster: ROSTER,
      editable: false,
    },
  });
  await tick();
  assert.doesNotMatch(view.container.textContent, /New effect/, 'no entry point, no disabled theatre');
});

test('without a party id the entry point is absent (standalone sheet)', async () => {
  const sync = fakeSessionSync();
  const view = render(SheetView, {
    props: { character: CHARACTER, accountSub: 'dev-sub-josh', sync },
  });
  await tick();
  assert.doesNotMatch(view.container.textContent, /New effect/);
});

// -- the manager face: remove-a-target and end ride the mutation ops --------

test('the manager lists the creator\u2019s effects from the REST face; remove and end emit the CAS mutation ops', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync, { fetchImpl: stubFetchEffects() });

  await waitFor(() => {
    assert.match(view.container.textContent, /Bless/, 'the REST rows render');
  });
  assert.match(view.container.textContent, /Josh/, 'current targets render by name');
  assert.match(view.container.textContent, /Becky/);

  fireEvent.click(view.getByRole('button', { name: 'Remove Josh from Bless' }));
  await tick();
  assert.equal(sync.writes.length, 1);
  assert.deepEqual(sync.writes[0].target, { kind: 'effect', effect_id: 41 });
  assert.equal(sync.writes[0].base_version, 1042, 'whole-row CAS on the fetched version (FR-14)');
  assert.deepEqual(sync.writes[0].value, { op: 'update', targets: [5] }, 'the remaining set, whole');

  fireEvent.click(view.getByRole('button', { name: 'End Bless' }));
  await tick();
  assert.equal(sync.writes.length, 2);
  assert.deepEqual(sync.writes[1].value, { op: 'end' });
});

test('another player\u2019s effects show in the manager data but carry no controls', async () => {
  const sync = fakeSessionSync();
  const rows = [
    { ...EFFECT_ROWS[0], effect_id: 77, name: 'Inspire Courage', source_character_id: 3, version: 900 },
  ];
  const view = openComposerViaSheet(sync, {
    fetchImpl: (url) =>
      url === `/api/parties/${PARTY_ID}/effects`
        ? { ok: true, status: 200, json: () => Promise.resolve(rows) }
        : Promise.reject(new Error(`unexpected fetch ${url}`)),
  });
  await waitFor(() => assert.match(view.container.textContent, /Inspire Courage/));
  assert.doesNotMatch(view.container.textContent, /End Inspire Courage/, 'not the creator — no end control');
  assert.doesNotMatch(view.container.textContent, /Remove .* from Inspire Courage/, 'not the creator — no retarget control');
});
