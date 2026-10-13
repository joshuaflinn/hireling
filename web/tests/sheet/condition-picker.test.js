import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

/** The fetch's promise chain plus Svelte's render queue — flush them all. */
const flush = async () => {
  for (let i = 0; i < 5; i += 1) await tick();
};

import ConditionPicker from '../../src/lib/sheet/components/ConditionPicker.svelte';

// The condition picker surface (E9 T9, FR-7): a dialog over E8's existing
// `GET /api/parties/{id}/conditions` — search, tier/lane/valued badges, a
// value input for valued conditions, apply through the existing
// effect-create write shape, and the Add-custom-condition affordance. The
// rows fixture carries all three shapes: engine_math/valued, display_only,
// and a custom row.

/** @returns {typeof fetch} */
function conditionsRoute(rows) {
  return /** @type {typeof fetch} */ (
    /** @param {string} url @returns {Promise<any>} */ (url) => {
      if (String(url).includes('/conditions')) {
        return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(rows) });
      }
      return Promise.resolve({ ok: false, status: 404, json: () => Promise.resolve({}) });
    }
  );
}

const ROWS = [
  { corpus_entry_id: 76, name: 'Frightened', tier: 'engine_math', lane: 'core', valued: true },
  { corpus_entry_id: 59, name: 'Blinded', tier: 'display_only', lane: 'imported', valued: false },
  { corpus_entry_id: 99, name: 'Sunlit', tier: 'display_only', lane: 'custom', valued: false },
];

/** @returns {{applies: any[], apply}} */
function applySpy() {
  /** @type {any[]} */ const applies = [];
  return {
    applies,
    apply: (/** @type {any} */ create) => applies.push(create),
  };
}

afterEach(cleanup);

test('picker rows render from the fetched list, badged by tier and lane', async () => {
  const { container } = render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
    },
  });
  await flush();
  const dialog = /** @type {HTMLElement} */ (screen.getByRole('dialog'));
  const text = dialog.textContent;
  assert.match(text, /Frightened/);
  assert.match(text, /engine_math/, 'the tier badge names the engine tier');
  assert.match(text, /display_only/);
  assert.match(text, /custom/, 'custom rows list alongside corpus rows — same list, no separate section');
  assert.match(container.innerHTML, /Blinded/);
});

test('a valued condition carries a value input; display-only and custom rows do not', async () => {
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
    },
  });
  await flush();
  assert.ok(screen.getByLabelText('Value of Frightened'), 'the valued row takes a value');
  assert.throws(() => screen.getByLabelText('Value of Blinded'), 'display-only applies without value');
});

test('applying a valued condition issues the existing effect-create shape', async () => {
  const spy = applySpy();
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 3,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: spy.apply,
    },
  });
  await flush();
  fireEvent.input(screen.getByLabelText('Value of Frightened'), { target: { value: '2' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply Frightened' }));
  assert.equal(spy.applies.length, 1);
  assert.deepEqual(spy.applies[0], {
    name: 'Frightened',
    source_character_id: 3,
    targets: [7],
    modifiers: [],
    duration_note: '',
    corpus_entry_id: 76,
    condition_value: 2,
  });
});

test('a display-only condition applies without a value — condition_value null', async () => {
  const spy = applySpy(7);
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 3,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: spy.apply,
    },
  });
  await flush();
  fireEvent.click(screen.getByRole('button', { name: 'Apply Blinded' }));
  assert.equal(spy.applies.length, 1);
  assert.equal(spy.applies[0].corpus_entry_id, 59);
  assert.equal(spy.applies[0].condition_value, null);
});

test('a valued condition with an out-of-bounds value refuses inline and never applies', async () => {
  const spy = applySpy(7);
  const { container } = render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 3,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: spy.apply,
    },
  });
  await flush();
  fireEvent.input(screen.getByLabelText('Value of Frightened'), { target: { value: '21' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply Frightened' }));
  const error = container.querySelector('[role="alert"]');
  assert.ok(error, 'the refusal is inline');
  assert.match(/** @type {HTMLElement} */ (error).textContent, /1\.\.20/);
  assert.equal(spy.applies.length, 0);
});

test('the search narrows the list', async () => {
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
    },
  });
  await flush();
  fireEvent.input(screen.getByLabelText('Search conditions'), { target: { value: 'blind' } });
  assert.match(screen.getByRole('dialog').textContent, /Blinded/);
  assert.doesNotMatch(screen.getByRole('dialog').textContent, /Frightened/);
});

test('a custom row shows its optional value as a display note and its description', async () => {
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
      customRows: [
        {
          corpus_entry_id: 99,
          kind: 'condition',
          name: 'Sunlit',
          lane: 'custom',
          description: 'Standing in the sun',
          value_or_rank: 2,
          created_by_sub: 'dev-sub-bear',
        },
      ],
    },
  });
  await flush();
  const dialog = screen.getByRole('dialog');
  assert.match(dialog.textContent, /Standing in the sun/, 'the creator description');
  assert.match(dialog.textContent, /Value: 2/, 'the value is a display note — never an apply input');
});

test('picker rows are tooltip triggers — Frightened opens the curated tip', async () => {
  const { container } = render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
    },
  });
  await flush();
  fireEvent.mouseEnter(screen.getByRole('button', { name: 'Frightened' }));
  assert.match(
    /** @type {HTMLElement} */ (container.querySelector('.pop')).innerHTML,
    /Status penalty equal to the value/,
  );
});

test('Add custom condition opens the inline form; its submit rides onsubmitcustom', async () => {
  /** @type {any[]} */ const submissions = [];
  render(ConditionPicker, {
    props: {
      partyId: 1,
      targetId: 7,
      sourceId: 7,
      open: true,
      fetchImpl: conditionsRoute(ROWS),
      onapply: () => {},
      onsubmitcustom: (/** @type {any} */ fields) => submissions.push(fields),
    },
  });
  await flush();
  fireEvent.click(screen.getByRole('button', { name: 'Add custom condition' }));
  fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Sunlit' } });
  fireEvent.input(screen.getByLabelText('Description'), { target: { value: 'Standing in the sun' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  assert.deepEqual(submissions, [{ name: 'Sunlit', description: 'Standing in the sun', value_or_rank: null }]);
});
