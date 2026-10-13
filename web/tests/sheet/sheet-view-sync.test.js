import { test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup, fireEvent } from '@testing-library/svelte';
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

// ---- E9 T9: the owner-gated "Add condition" affordance near the strip ----

test('an editable sheet shows the Add condition affordance; a view-only sheet does not', async () => {
  const owned = render(SheetView, {
    props: { character: CHARACTER, accountSub: 'dev-sub-josh', sync: fakeSessionSync(), editable: true },
  });
  await tick();
  assert.ok(owned.getByRole('button', { name: 'Add condition' }), 'the owner sees the affordance');
  owned.unmount();

  const viewer = render(SheetView, {
    props: { character: CHARACTER, accountSub: 'dev-sub-josh', sync: fakeSessionSync(), editable: false },
  });
  await tick();
  assert.throws(() => viewer.getByRole('button', { name: 'Add condition' }), 'GM/cross-member sees none');
});

test('the affordance opens the picker, which renders the party conditions (E8 REST)', async () => {
  // jsdom's window.fetch is the picker's fetch; the standalone path's
  // custom-store fetches ride it too — answer them all, loudly recorded.
  /** @type {string[]} */ const requested = [];
  const rows = [
    { corpus_entry_id: 76, name: 'Frightened', tier: 'engine_math', lane: 'core', valued: true },
    { corpus_entry_id: 99, name: 'Sunlit', tier: 'display_only', lane: 'custom', valued: false },
  ];
  const original = globalThis.fetch;
  globalThis.fetch = /** @type {typeof fetch} */ (
    /** @param {string} url @returns {Promise<any>} */ (url) => {
      requested.push(String(url));
      if (String(url).includes('/custom?kind=')) {
        return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
      }
      if (String(url).includes('/conditions')) {
        return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(rows) });
      }
      return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({}) });
    }
  );
  try {
    const { getByRole } = render(SheetView, {
      props: {
        character: CHARACTER,
        accountSub: 'dev-sub-josh',
        sync: fakeSessionSync(),
        editable: true,
        partyId: 4,
      },
    });
    await tick();
    fireEvent.click(getByRole('button', { name: 'Add condition' }));
    for (let i = 0; i < 5; i += 1) await tick();
    const dialog = /** @type {HTMLElement} */ (getByRole('dialog'));
    assert.match(dialog.textContent, /Frightened/, 'the picker lists the party conditions');
    assert.match(dialog.textContent, /Sunlit/, 'custom rows list inline');
    assert.ok(
      requested.some((url) => url.includes('/api/parties/4/conditions')),
      'the picker fetches the passed party id',
    );
  } finally {
    globalThis.fetch = original;
  }
});
