/* global console */
import { afterEach, test, vi } from 'vitest';
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
/** @param {Array<any>} rows the REST face's answer */
function fetchStubFor(rows) {
  return (url) => {
    if (url === `/api/parties/${PARTY_ID}/effects`) {
      return { ok: true, status: 200, json: () => Promise.resolve(rows) };
    }
    return Promise.reject(new Error(`unexpected fetch ${url}`));
  };
}
function stubFetchEffects() {
  return fetchStubFor(EFFECT_ROWS);
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
  const view = openComposerViaSheet(sync, { fetchImpl: fetchStubFor(rows) });
  await waitFor(() => assert.match(view.container.textContent, /Inspire Courage/));
  assert.doesNotMatch(view.container.textContent, /End Inspire Courage/, 'not the creator — no end control');
  assert.doesNotMatch(view.container.textContent, /Remove .* from Inspire Courage/, 'not the creator — no retarget control');
  assert.doesNotMatch(view.container.textContent, /Your active effects/, 'not the creator\u2019s — never under that heading (finding 7)');
  const headings = [...view.container.querySelectorAll('h4')].map((h) => h.textContent?.trim());
  assert.deepEqual(headings, ['Party effects']);
});

// -- review rework (MOR-103 / gh#74): the seven findings, each proven at
//    the mounted component -------------------------------------------------

// Finding 1 — P0 unbuilt: PRD FG3 says the creator ADDS targets too.
test('the creator can ADD a target — the whole set, member added (PRD FG3, spec FR-C6)', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync, { fetchImpl: stubFetchEffects() });
  await waitFor(() => assert.match(view.container.textContent, /Bless/));

  assert.ok(view.getByRole('button', { name: 'Add Bear to Bless' }), 'roster members not targeted render an Add control');
  assert.equal(view.queryByRole('button', { name: 'Add Josh to Bless' }), null, 'already targeted — no Add for Josh');
  assert.equal(view.queryByRole('button', { name: 'Add Becky to Bless' }), null, 'already targeted — no Add for Becky');

  fireEvent.click(view.getByRole('button', { name: 'Add Bear to Bless' }));
  await tick();
  assert.equal(sync.writes.length, 1);
  assert.deepEqual(sync.writes[0].target, { kind: 'effect', effect_id: 41 });
  assert.equal(sync.writes[0].base_version, 1042, 'whole-row CAS on the fetched version (FR-14)');
  assert.deepEqual(sync.writes[0].value, { op: 'update', targets: [3, 5, 7] }, 'the whole set, member added');
});

// Finding 2 — settleAck on `superseded` dequeues the op and emits ONLY the
// queue event; the composer must not park on “Applying…” forever.
test('a lost CAS race unsticks the composer — the queue event with the op gone clears \u201cApplying\u2026\u201d', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Bless' } });
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'attack' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Josh' }));
  fireEvent.click(view.getByRole('button', { name: 'Apply effect' }));
  await tick();
  const writing = view.getByRole('button', { name: 'Applying\u2026' });
  assert.equal(writing.disabled, true, 'the op is in flight');

  sync.emit({ type: 'queue', length: 0 });
  await tick();
  assert.doesNotMatch(view.container.textContent, /Applying/);
  const rearmed = view.getByRole('button', { name: 'Apply effect' });
  assert.equal(rearmed.disabled, false, 'Apply re-armed — the dialog is not dead');
});

// Finding 3 — an applied MANAGER op must not close the dialog (the
// phase-only settle discarded the half-composed form); the rows refetch.
test('an applied manager op settles without closing — the rows refetch (FR-C6)', async () => {
  const sync = fakeSessionSync();
  let fetches = 0;
  const view = openComposerViaSheet(sync, {
    fetchImpl: (url) => {
      if (url === `/api/parties/${PARTY_ID}/effects`) {
        fetches += 1;
        return { ok: true, status: 200, json: () => Promise.resolve(EFFECT_ROWS) };
      }
      return Promise.reject(new Error(`unexpected fetch ${url}`));
    },
  });
  await waitFor(() => assert.equal(fetches, 1, 'the open fetch'));

  fireEvent.click(view.getByRole('button', { name: 'Remove Josh from Bless' }));
  assert.equal(sync.writes.length, 1, 'the retarget op left the layer');
  sync.emit({ type: 'applied', key: targetKey({ kind: 'effect', effect_id: 41 }) });
  await waitFor(() => assert.equal(fetches, 2, 'the settle refetch (FR-C6: refetch after every op)'));
  assert.ok(
    view.getByRole('button', { name: 'Apply effect' }),
    'the dialog stayed open — the half-composed form survives',
  );
});

// Finding 4 — a failed effects fetch is visible; no silent failures.
test('a failed effects fetch is visible, not a silent empty list', async () => {
  const sync = fakeSessionSync();
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const view = openComposerViaSheet(sync, {
      fetchImpl: () => Promise.resolve({ ok: false, status: 503, json: () => Promise.resolve([]) }),
    });
    await waitFor(() => assert.match(view.container.textContent, /Couldn't load active effects/));
    assert.equal(view.queryByRole('button', { name: 'End Bless' }), null, 'no rows — no controls');
    assert.ok(
      warn.mock.calls.some((call) => call.join(' ').includes('503')),
      'the warn carries the status',
    );
  } finally {
    warn.mockRestore();
  }
});

// Finding 5 — the empty-set guard is right; the dead ✕ on the last target
// is the defect. Both sides of the threshold asserted.
test('the last remaining target offers no remove control — End is the operation', async () => {
  const sync = fakeSessionSync();
  const single = [{ ...EFFECT_ROWS[0], targets: [3] }];
  const view = openComposerViaSheet(sync, { fetchImpl: fetchStubFor(single) });
  await waitFor(() => assert.match(view.container.textContent, /Bless/));
  assert.equal(
    view.queryByRole('button', { name: 'Remove Josh from Bless' }),
    null,
    'one target left — the ✕ is not rendered (an empty set is refused; End is the operation)',
  );
  assert.ok(view.getByRole('button', { name: 'End Bless' }), 'End still offered');
  cleanup();

  const two = [{ ...EFFECT_ROWS[0] }];
  const view2 = openComposerViaSheet(sync, { fetchImpl: fetchStubFor(two) });
  await waitFor(() => assert.match(view2.container.textContent, /Bless/));
  assert.ok(
    view2.getByRole('button', { name: 'Remove Josh from Bless' }),
    'two targets — the remove control exists (threshold minus one vs threshold)',
  );
});

// Finding 6 — Math.round made 1.6 valid and shipped value: 2. The number
// that lands must be the number shown.
test('a fractional modifier value never arms Apply — the number shown ships', async () => {
  const sync = fakeSessionSync();
  const view = openComposerViaSheet(sync);
  await tick();

  fireEvent.input(view.getByLabelText('Effect name'), { target: { value: 'Bless' } });
  fireEvent.change(view.getByLabelText('Stat'), { target: { value: 'attack' } });
  fireEvent.click(view.getByRole('checkbox', { name: 'Josh' }));
  fireEvent.input(view.getByLabelText('Value'), { target: { value: '1.6' } });
  const apply = view.getByRole('button', { name: 'Apply effect' });
  assert.equal(apply.disabled, true, '1.6 is not an integer — Apply stays dark');
  fireEvent.click(apply);
  await tick();
  assert.equal(sync.writes.length, 0, 'nothing shipped');

  fireEvent.input(view.getByLabelText('Value'), { target: { value: '2' } });
  assert.equal(apply.disabled, false, 'the whole number arms Apply');
  fireEvent.click(apply);
  assert.equal(sync.writes.length, 1);
  assert.equal(sync.writes[0].value.modifiers[0].value, 2, 'the shipped number is the shown number');
});

// Finding 7 — the others block rendered inside “Your active effects”.
test('another player\u2019s effects render under their own heading, not \u201cYour active effects\u201d', async () => {
  const sync = fakeSessionSync();
  const rows = [
    EFFECT_ROWS[0],
    { ...EFFECT_ROWS[0], effect_id: 77, name: 'Inspire Courage', source_character_id: 3, version: 900 },
  ];
  const view = openComposerViaSheet(sync, { fetchImpl: fetchStubFor(rows) });
  await waitFor(() => assert.match(view.container.textContent, /Inspire Courage/));
  const headings = [...view.container.querySelectorAll('h4')].map((h) => h.textContent?.trim());
  assert.deepEqual(headings, ['Your active effects', 'Party effects']);
  const text = view.container.textContent ?? '';
  assert.ok(
    text.indexOf('Party effects') < text.indexOf('Inspire Courage'),
    'the other player\u2019s row renders after the party heading, not under yours',
  );
});
