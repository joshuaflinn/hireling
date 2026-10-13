/* global structuredClone */
import { test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

import SheetView from '../../src/lib/sheet/SheetView.svelte';
import { targetKey } from '../../src/lib/sync/store.js';
import baseSheet from '../data/base_sheet_reference.json';
import engine from '../data/engine_output_reference.json';

// gh#82 acceptance 4: the Raise-a-Shield affordance is proven through the
// mounted component — fireEvent.click, then the RENDERED AC — not on the
// store alone. The scripted sync replays the composed client's real event
// order for an applied write (queue echo → applied → fields → derived), so
// the chain under test is click → StatsPane → state.writeShieldRaised →
// sync.write → settle → derived frame → the AC tile's number.

const CHARACTER_ID = 7;

/** The reference sheet with a +2 shield in its `ac` section. */
const shieldedSheet = (() => {
  const sheet = structuredClone(baseSheet);
  sheet.ac.shieldBonus = 2;
  return sheet;
})();

/** The raised engine output: the lowered fixture plus exactly the bonus. */
const raisedOutput = (() => {
  const output = structuredClone(engine);
  output.derived.ac.base = 18;
  output.derived.ac.total = 18;
  return output;
})();

/** The me-shaped bootstrap; `shield_raised` null = never toggled live. */
function character(sheet) {
  return {
    character: { id: CHARACTER_ID, name: sheet.identity.name, owner: 'dev-sub-josh' },
    base_sheet: sheet,
    vitals: {
      hp: 20,
      temp_hp: 0,
      money_pp: 0,
      money_gp: 5,
      money_sp: 0,
      money_cp: 0,
      level_adjust: 0,
      focus_current: 1,
      hero_points: 1,
      daily: { staff_charge_rank: 0, staff_spent: 0, drain_used: false },
      shield_raised: null,
    },
    slots: [],
    inventory: [],
    item_bulk: {},
    item_traits: {},
  };
}

/** A sync double that APPLIES every write: the optimistic echo, the
 * settled field, and the derived consequence — the same event sequence
 * the real composed sync emits for an applied write. Pass a connection
 * state to simulate degraded mode (`'live'` by default). */
function scriptedSync(connection = 'live') {
  const listeners = new Set();
  const settled = new Map();
  const pending = new Map();
  const outputs = new Map([[CHARACTER_ID, engine]]);
  let version = 0;
  const emit = (event) => {
    for (const cb of listeners) cb(event);
  };
  return {
    connect() {},
    disconnect() {},
    /** @param {{character_id: number}} target @param {*} value */
    write(target, value) {
      const key = targetKey(target);
      pending.set(key, { target, value });
      emit({ type: 'queue', length: 1 });
      pending.delete(key);
      settled.set(key, { target, value, version: (version += 1) });
      emit({ type: 'queue', length: 0 });
      emit({ type: 'applied', key });
      emit({ type: 'fields' });
      outputs.set(target.character_id, value ? raisedOutput : engine);
      emit({ type: 'derived', character_id: target.character_id });
    },
    state: () => {
      const out = {};
      for (const [key, entry] of settled) {
        out[key] = { target: entry.target, value: entry.value, version: entry.version };
      }
      for (const [key, op] of pending) {
        out[key] = { target: op.target, value: op.value, version: 0 };
      }
      return out;
    },
    derived: (id) => outputs.get(id) ?? null,
    isSyncing: () => pending.size > 0,
    snapshotForBoot: () => '{"fields":[]}',
    queue: () => [...pending.values()],
    connectionState: () => connection,
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

const acTile = (container) => container.querySelector('.t-ac .v')?.textContent ?? '';

test('raise, then lower, through the mounted sheet: the AC moves by exactly the shield bonus', async () => {
  const { container, getByRole, unmount } = render(SheetView, {
    props: {
      character: character(shieldedSheet),
      accountSub: 'dev-sub-josh',
      sync: scriptedSync(),
      editable: true,
    },
  });
  await tick();
  assert.match(acTile(container), /16/, 'lowered baseline renders');
  const toggle = getByRole('button', { name: 'Raise shield' });
  assert.equal(toggle.getAttribute('aria-pressed'), 'false', 'default is lowered');

  fireEvent.click(getByRole('button', { name: 'Raise shield' }));
  await tick();
  assert.match(acTile(container), /18/, 'raised: +2 exactly, on the rendered tile');
  assert.equal(
    getByRole('button', { name: 'Lower shield' }).getAttribute('aria-pressed'),
    'true',
    'the toggle shows its raised state',
  );

  fireEvent.click(getByRole('button', { name: 'Lower shield' }));
  await tick();
  assert.match(acTile(container), /16/, 'lowered: back to baseline — both sides live');
  unmount();
  cleanup();
});

test('offline, the toggle disables like every other control in the pane', async () => {
  const { getByRole, unmount } = render(SheetView, {
    props: {
      character: character(shieldedSheet),
      accountSub: 'dev-sub-josh',
      sync: scriptedSync('offline'),
      editable: true,
    },
  });
  await tick();
  assert.equal(
    getByRole('button', { name: 'Raise shield' }).disabled,
    true,
    'degraded-mode.md:32 — edit affordances disable offline; a control that'
      + ' lies once and then locks (the enqueued write keeps pending true) is'
      + ' worse than one never offered',
  );
  unmount();
  cleanup();
});

test('a view-only (GM) sheet sees the shield state without the control', async () => {
  const { container, unmount } = render(SheetView, {
    props: {
      character: character(shieldedSheet),
      accountSub: 'dev-sub-josh',
      sync: scriptedSync(),
      editable: false,
    },
  });
  await tick();
  assert.equal(
    container.querySelector('.shield-toggle'),
    null,
    'no control on a view-only sheet',
  );
  const state = container.querySelector('.shield-state');
  assert.ok(state, 'the raised/lowered text is present');
  assert.match(state.textContent, /lowered/, 'the reference sheet carries none — lowered');
  assert.equal(
    state.getAttribute('aria-hidden'),
    null,
    'on a view-only sheet this span is the only state signal — screen readers get it',
  );
  unmount();
  cleanup();
});

test('a character without a shield gets no affordance', async () => {
  const { container, unmount } = render(SheetView, {
    props: {
      character: character(baseSheet),
      accountSub: 'dev-sub-josh',
      sync: scriptedSync(),
      editable: true,
    },
  });
  await tick();
  assert.equal(
    container.querySelector('.shield-toggle'),
    null,
    'no shieldBonus in the sheet — no Raise shield button',
  );
  unmount();
  cleanup();
});
