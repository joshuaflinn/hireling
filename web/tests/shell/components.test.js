import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import EmptyState from '../../src/lib/sheet/components/EmptyState.svelte';
import ErrorState from '../../src/lib/sheet/components/ErrorState.svelte';
import Skeleton from '../../src/lib/sheet/components/Skeleton.svelte';
import SyncIndicator from '../../src/lib/sheet/components/SyncIndicator.svelte';
import CharacterHeader from '../../src/lib/sheet/components/CharacterHeader.svelte';
import StatsPane from '../../src/lib/sheet/components/StatsPane.svelte';
import HpBar from '../../src/lib/sheet/components/HpBar.svelte';
import PipRow from '../../src/lib/sheet/components/PipRow.svelte';

import { derive } from '../../src/lib/engine/index.js';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));

test('EmptyState is a designed screen: crest, copy, and the import CTA', () => {
  const { body } = render(EmptyState, { props: { onimport: () => {} } });
  assert.match(body, /No character yet/);
  assert.match(body, /Import your character/);
});

test('ErrorState renders a calm message; the retry affordance only where a retry exists', () => {
  const withRetry = render(ErrorState, {
    props: { message: 'Boom.', detail: '503', onretry: () => {} },
  }).body;
  assert.match(withRetry, /Boom/);
  assert.match(withRetry, /Try again/);
  const withoutRetry = render(ErrorState, { props: { message: 'Boom.' } }).body;
  assert.doesNotMatch(withoutRetry, /Try again/);
});

test('Skeleton renders pane-shaped placeholders, never blank', () => {
  const { body } = render(Skeleton, { props: { panes: 3 } });
  const skeletons = body.match(/class="skeleton"/g) ?? [];
  assert.ok(skeletons.length >= 3, `expected at least 3 skeleton blocks, got ${skeletons.length}`);
  assert.match(body, /Loading your sheet/);
});

test('SyncIndicator renders iff the queue is non-empty — connection state is irrelevant', () => {
  assert.doesNotMatch(render(SyncIndicator, { props: { syncing: false } }).body, /syncing/);
  const syncing = render(SyncIndicator, { props: { syncing: true } }).body;
  assert.match(syncing, /syncing/);
});

test('CharacterHeader: editable shows the level control and New Day; view-only shows neither', () => {
  const editable = render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: 'Gnome · Wizard', level: 3, syncing: false },
  }).body;
  assert.match(editable, /Lorum Ipsum/);
  assert.match(editable, /Level 3/);
  assert.match(editable, /New Day/);

  const viewOnly = render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: '', level: 3, editable: false },
  }).body;
  assert.match(viewOnly, /Level 3/);
  assert.doesNotMatch(viewOnly, /New Day/);
});

test('StatsPane renders the fixture through the adapter: tiles, pips, skills, meta', () => {
  const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });
  const noop = () => {};
  const { body } = render(StatsPane, {
    props: {
      view,
      baseSheet: fixture,
      hp: { value: 20, pending: false },
      temp: { value: 0, pending: false },
      money: { value: { pp: 0, gp: 24, sp: 2, cp: 4 }, pending: false },
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
  assert.match(body, /Armor Class/);
  assert.match(body, /\b16\b/, 'AC 16 from the fixture');
  assert.match(body, /Hero Points/);
  assert.match(body, /Wellspring Gnome/, 'heritage rides the meta lines');
  assert.match(body, /Thievery/, 'the skill grid renders');
  assert.match(body, /Mror Holds History Lore/, 'lores render wide');
});

test('StatsPane is view-only without controls when editable is false', () => {
  const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });
  const { body } = render(StatsPane, {
    props: {
      view,
      baseSheet: fixture,
      hp: { value: 20, pending: false },
      temp: { value: 0, pending: false },
      focusCurrent: { value: 0, pending: false },
      focusMax: 1,
      heroPoints: { value: 1, pending: false },
      heroMax: 3,
      editable: false,
    },
  });
  assert.doesNotMatch(body, /Restore to max/, 'no HP buttons');
  const buttons = body.match(/<button/g) ?? [];
  assert.equal(buttons.length, 0, 'view-only renders no controls at all');
});

// ---- MOR-48 review fixes: the production path owns the behaviour ----------

test('HpBar renders temp HP as markup, never as escaped text (finding 1)', () => {
  const withTemp = render(HpBar, {
    props: {
      hp: { value: 12, pending: false },
      temp: { value: 5, pending: false },
      max: 32,
    },
  }).body;
  assert.match(withTemp, /\+5 temp/, 'the temp segment renders');
  assert.doesNotMatch(withTemp, /&lt;span/, 'no literal tags in the readout');
  const withoutTemp = render(HpBar, {
    props: { hp: { value: 12, pending: false }, temp: { value: 0, pending: false }, max: 32 },
  }).body;
  assert.doesNotMatch(withoutTemp, /temp</, 'nothing renders when the pool is empty');
});

test('PipRow: editable pips are buttons, view-only pips are bare spans (finding 9)', () => {
  const editable = render(PipRow, {
    props: { label: 'Focus', current: 1, max: 1, onset: () => {} },
  }).body;
  assert.match(editable, /<button/, 'editable renders controls');
  assert.match(editable, /aria-label="Focus: 1 of 1"/);
  const viewOnly = render(PipRow, {
    props: { label: 'Focus', current: 1, max: 1, editable: false },
  }).body;
  assert.doesNotMatch(viewOnly, /<button/, 'view-only renders no controls');
  assert.match(viewOnly, /class="pip on"/);
});

test('a rejected write surfaces inline at its control (finding 5)', () => {
  const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });
  const noop = () => {};
  const rejected = render(StatsPane, {
    props: {
      view,
      baseSheet: fixture,
      hp: { value: 20, pending: false },
      temp: { value: 0, pending: false },
      focusCurrent: { value: 0, pending: false },
      focusMax: 1,
      heroPoints: { value: 1, pending: false },
      heroMax: 3,
      opErrors: [
        {
          key: 'vitals:7:hp',
          target: { kind: 'vitals', character_id: 7, field: 'hp' },
          op_id: 'op-1',
          outcome: 'rejected',
          reason: 'The party refused that write: stale version.',
        },
      ],
      ondamage: noop,
      onheal: noop,
      onfull: noop,
      ontemp: noop,
      onfocus: noop,
      onhero: noop,
    },
  }).body;
  assert.match(rejected, /The party refused that write: stale version\./);
  assert.match(rejected, /role="alert"/);
  const calm = render(StatsPane, {
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
  }).body;
  assert.doesNotMatch(calm, /role="alert"/, 'no error surface without an error');
});

test('the header carries Import and Log out — there is a way off the sheet (finding 10)', () => {
  const header = render(CharacterHeader, {
    props: {
      name: 'Lorum Ipsum',
      subline: '',
      level: 3,
      onimport: () => {},
      onlogout: () => {},
    },
  }).body;
  assert.match(header, /Log out/);
  assert.match(header, /Import/);
  const bare = render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: '', level: 3 },
  }).body;
  assert.doesNotMatch(bare, /Log out/, 'affordances appear only when wired');
});

test('Dialog forwards oncommit to the keyboard discipline (finding 6)', async () => {
  // SSR cannot synthesize a keydown; the seam this pins is that Dialog
  // passes its oncommit prop into trapKeys — the exact wire finding 6
  // found missing (the prop existed in the util, never in the dialog).
  const source = await readFile(
    new URL('../../src/lib/sheet/components/Dialog.svelte', import.meta.url),
    'utf8',
  );
  assert.match(source, /onCommit: oncommit/, 'Enter commits is wired, not claimed');
});
