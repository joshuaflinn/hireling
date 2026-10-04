import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';
import { afterEach, test, vi } from 'vitest';
import { tick } from 'svelte';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (extractor + compute over the same export — the swap's byte-identity
 * fixture). */
import engine from '../data/engine_output_reference.json';

import EmptyState from '../../src/lib/sheet/components/EmptyState.svelte';
import ErrorState from '../../src/lib/sheet/components/ErrorState.svelte';
import Skeleton from '../../src/lib/sheet/components/Skeleton.svelte';
import SyncIndicator from '../../src/lib/sheet/components/SyncIndicator.svelte';
import CharacterHeader from '../../src/lib/sheet/components/CharacterHeader.svelte';
import StatsPane from '../../src/lib/sheet/components/StatsPane.svelte';
import HpBar from '../../src/lib/sheet/components/HpBar.svelte';
import PipRow from '../../src/lib/sheet/components/PipRow.svelte';
import Dialog from '../../src/lib/sheet/components/Dialog.svelte';

afterEach(cleanup);

test('EmptyState is a designed screen: crest, copy, and the import CTA', () => {
  render(EmptyState, { props: { onimport: () => {} } });
  screen.getByText(/No character yet/);
  screen.getByRole('button', { name: 'Import your character' });
});

test('ErrorState renders a calm message; the retry affordance only where a retry exists', () => {
  render(ErrorState, {
    props: { message: 'Boom.', detail: '503', onretry: () => {} },
  });
  screen.getByText(/Boom/);
  screen.getByRole('button', { name: 'Try again' });
  cleanup();
  render(ErrorState, { props: { message: 'Boom.' } });
  assert.equal(screen.queryByRole('button', { name: 'Try again' }), null);
});

test('Skeleton renders pane-shaped placeholders, never blank', () => {
  const { container } = render(Skeleton, { props: { panes: 3 } });
  const skeletons = container.innerHTML.match(/class="skeleton"/g) ?? [];
  assert.ok(skeletons.length >= 3, `expected at least 3 skeleton blocks, got ${skeletons.length}`);
  screen.getByRole('status', { name: 'Loading your sheet' });
});

test('SyncIndicator renders iff the queue is non-empty — connection state is irrelevant', () => {
  render(SyncIndicator, { props: { syncing: false } });
  assert.equal(screen.queryByRole('status'), null);
  cleanup();
  render(SyncIndicator, { props: { syncing: true } });
  assert.match(screen.getByRole('status').textContent, /syncing/);
});

test('CharacterHeader: editable shows the level control and New Day; view-only shows neither', () => {
  render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: 'Gnome · Wizard', level: 3, syncing: false },
  });
  screen.getByText(/Lorum Ipsum/);
  screen.getByRole('button', { name: 'Level 3' });
  screen.getByRole('button', { name: 'New Day' });

  cleanup();
  render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: '', level: 3, editable: false },
  });
  screen.getByText(/Level 3/);
  assert.equal(screen.queryByRole('button', { name: 'New Day' }), null);
});

test('StatsPane renders the fixture through the adapter: tiles, pips, skills, meta', () => {
  const view = engine;
  const noop = () => {};
  const { container } = render(StatsPane, {
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
  screen.getByText(/Armor Class/);
  assert.match(container.innerHTML, /\b16\b/, 'AC 16 from the fixture');
  screen.getByText(/Hero Points/);
  screen.getByText(/Wellspring Gnome/);
  screen.getByText(/Thievery/);
  screen.getByText(/Mror Holds History Lore/);
});

test('StatsPane is view-only without controls when editable is false', () => {
  const view = engine;
  const { container } = render(StatsPane, {
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
  // The affordance is the title attribute on the buttons, not their text —
  // queryByText cannot see it (MOR-77 review finding 1).
  assert.equal(screen.queryByTitle(/Restore to max/), null, 'no HP buttons');
  assert.equal(container.querySelectorAll('button').length, 0, 'view-only renders no controls at all');
});

// ---- MOR-48 review fixes: the production path owns the behaviour ----------

test('HpBar renders temp HP as markup, never as escaped text (finding 1)', () => {
  const withTemp = render(HpBar, {
    props: {
      hp: { value: 12, pending: false },
      temp: { value: 5, pending: false },
      max: 32,
    },
  }).container.innerHTML;
  assert.match(withTemp, /\+5 temp/, 'the temp segment renders');
  assert.doesNotMatch(withTemp, /&lt;span/, 'no literal tags in the readout');
  cleanup();
  const withoutTemp = render(HpBar, {
    props: { hp: { value: 12, pending: false }, temp: { value: 0, pending: false }, max: 32 },
  }).container.innerHTML;
  assert.doesNotMatch(withoutTemp, /temp</, 'nothing renders when the pool is empty');
});

test('PipRow: editable pips are buttons, view-only pips are bare spans (finding 9)', () => {
  const editable = render(PipRow, {
    props: { label: 'Focus', current: 1, max: 1, onset: () => {} },
  }).container;
  screen.getByRole('group', { name: 'Focus: 1 of 1' });
  assert.ok(editable.querySelectorAll('button').length > 0, 'editable renders controls');
  cleanup();
  const viewOnly = render(PipRow, {
    props: { label: 'Focus', current: 1, max: 1, editable: false },
  }).container;
  assert.equal(viewOnly.querySelectorAll('button').length, 0, 'view-only renders no controls');
  assert.match(viewOnly.innerHTML, /class="pip on"/);
});

test('a rejected write surfaces inline at its control (finding 5)', () => {
  const view = engine;
  const noop = () => {};
  render(StatsPane, {
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
  });
  assert.match(
    screen.getByRole('alert').textContent,
    /The party refused that write: stale version\./,
  );
  cleanup();
  render(StatsPane, {
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
  assert.equal(screen.queryByRole('alert'), null, 'no error surface without an error');
});

test('the header carries Import and Log out — there is a way off the sheet (finding 10)', () => {
  render(CharacterHeader, {
    props: {
      name: 'Lorum Ipsum',
      subline: '',
      level: 3,
      onimport: () => {},
      onlogout: () => {},
    },
  });
  screen.getByRole('button', { name: 'Log out' });
  screen.getByRole('button', { name: 'Import' });
  cleanup();
  render(CharacterHeader, {
    props: { name: 'Lorum Ipsum', subline: '', level: 3 },
  });
  assert.equal(
    screen.queryByRole('button', { name: 'Log out' }),
    null,
    'affordances appear only when wired',
  );
});

test('Dialog forwards oncommit to the keyboard discipline (finding 6)', async () => {
  // jsdom runs the $effect that installs trapKeys on the open dialog — the
  // wire finding 6 found missing (the prop existed in the util, never in
  // the dialog). A real keydown on the mounted dialog proves the
  // forwarding; the server-mode hook could only grep the source for it.
  const oncommit = vi.fn();
  const onclose = vi.fn();
  const { container } = render(Dialog, { props: { open: true, title: 'Confirm', onclose, oncommit } });
  await tick();
  const dialog = container.querySelector('dialog');
  assert.ok(dialog, 'the open dialog renders');
  fireEvent.keyDown(dialog, { key: 'Enter' });
  assert.equal(oncommit.mock.calls.length, 1, 'Enter commits through the wired prop');
  assert.equal(onclose.mock.calls.length, 0, 'Enter is not a cancel');
});
