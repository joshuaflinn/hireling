import { test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';

import SheetView from '../../src/lib/sheet/SheetView.svelte';
import baseSheet from '../data/base_sheet_reference.json';
import engine from '../data/engine_output_reference.json';

/** The me-shaped bootstrap payload over the reference sheet. */
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

/** A sync whose engine seam answers the reference output — the shell's
 *  session sync stands in; the sheet must consume IT, not build its own. */
function fakeSessionSync() {
  const listeners = new Set();
  return {
    connect() {},
    disconnect() {},
    write() {},
    state: () => ({}),
    derived: (characterId) => (characterId === 7 ? engine : null),
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

test('a provided sync is the one consumed: engine output renders, no skeletons', async () => {
  const { container } = render(SheetView, {
    props: { character: CHARACTER, accountSub: 'dev-sub-josh', sync: fakeSessionSync() },
  });
  await tick();
  // The engine seam's answer, verbatim: hp_max 32 lives in the HP readout,
  // level 3 in the header — numbers a self-built (socketless) sync could
  // never produce, since its derived() is honestly null.
  assert.match(container.textContent, /\/ 32/, 'the fixture hp_max renders from the provided sync');
  assert.equal(container.querySelector('.skeleton'), null, 'no loading state — the view arrived');
  cleanup();
});

test('no sync prop still renders: the standalone path self-constructs (skeletons)', async () => {
  const { container } = render(SheetView, {
    props: { character: CHARACTER, accountSub: 'dev-sub-josh' },
  });
  await tick();
  assert.match(container.textContent, /Lorum Ipsum/);
  assert.ok(
    container.querySelector('.skeleton'),
    'no wire yet on the standalone path — honest skeletons',
  );
  cleanup();
});
