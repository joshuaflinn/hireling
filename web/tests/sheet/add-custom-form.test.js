import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';
import { render, cleanup, screen, fireEvent } from '@testing-library/svelte';

import AddCustomForm from '../../src/lib/sheet/components/AddCustomForm.svelte';

// The shared minimal-fields form (E9 T7, FR-4): kind-parameterized, caps
// enforced inline with the field and bound named, and — the depth clause —
// no request when the client's own caps refuse: the submit spy stays cold.

afterEach(cleanup);

test('an over-cap name shows the inline error and never reaches onsubmit', async () => {
  let submissions = 0;
  const { container } = render(AddCustomForm, {
    props: { kind: 'item', onsubmit: () => { submissions += 1; } },
  });
  const name = /** @type {HTMLInputElement} */ (screen.getByLabelText('Name'));
  fireEvent.input(name, { target: { value: 'x'.repeat(65) } });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  const error = container.querySelector('[role="alert"]');
  assert.ok(error, 'the refusal renders inline');
  assert.match(/** @type {HTMLElement} */ (error).textContent, /name/i);
  assert.match(/** @type {HTMLElement} */ (error).textContent, /64/);
  assert.equal(submissions, 0, 'no request for a row the caps already refuse');
});

test('a blank name refuses too — required is a bound, not a formality', () => {
  let submissions = 0;
  const { container } = render(AddCustomForm, {
    props: { kind: 'item', onsubmit: () => { submissions += 1; } },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  assert.match(
    /** @type {HTMLElement} */ (container.querySelector('[role="alert"]')).textContent,
    /required/i,
  );
  assert.equal(submissions, 0);
});

test('a valid submit hands onsubmit the entered fields — the store owns the trim', async () => {
  /** @type {any[]} */ const seen = [];
  render(AddCustomForm, {
    props: {
      kind: 'item',
      onsubmit: (/** @type {any} */ fields) => {
        seen.push(fields);
      },
    },
  });
  fireEvent.input(screen.getByLabelText('Name'), { target: { value: '  Named Wagon  ' } });
  fireEvent.input(screen.getByLabelText('Description'), { target: { value: '  Looted  ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  assert.deepEqual(seen, [{ name: '  Named Wagon  ', description: '  Looted  ', value_or_rank: null }]);
});

test('the kind parameterizes the fields: spells take a rank, conditions an optional value, items neither', () => {
  const spell = render(AddCustomForm, { props: { kind: 'spell', onsubmit: () => {} } });
  assert.ok(screen.getByLabelText('Rank'), 'spell rank input');
  spell.unmount();

  const condition = render(AddCustomForm, { props: { kind: 'condition', onsubmit: () => {} } });
  assert.ok(screen.getByLabelText('Value (optional)'), 'condition value input');
  condition.unmount();

  render(AddCustomForm, { props: { kind: 'item', onsubmit: () => {} } });
  assert.throws(() => screen.getByLabelText('Rank'), 'items take no rank');
  assert.throws(() => screen.getByLabelText('Value (optional)'), 'items take no value');
});

test('spell rank out of bounds refuses inline: 0..10 named', () => {
  let submissions = 0;
  const { container } = render(AddCustomForm, {
    props: { kind: 'spell', onsubmit: () => { submissions += 1; } },
  });
  fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Conjure Toad Swarm' } });
  fireEvent.input(screen.getByLabelText('Rank'), { target: { value: '11' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  assert.match(
    /** @type {HTMLElement} */ (container.querySelector('[role="alert"]')).textContent,
    /0\.\.10/,
    'the bound is named',
  );
  assert.equal(submissions, 0);
});

test('a server refusal thrown from onsubmit lands inline with its field and reason', async () => {
  const { container } = render(AddCustomForm, {
    props: {
      kind: 'item',
      onsubmit: () => Promise.reject({ field: 'name', reason: 'too long' }),
    },
  });
  fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Wagon' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await Promise.resolve();
  await Promise.resolve();
  const error = container.querySelector('[role="alert"]');
  assert.ok(error, 'the server refusal renders');
  assert.match(/** @type {HTMLElement} */ (error).textContent, /too long/);
});

test('cancel calls oncancel', () => {
  let cancelled = 0;
  render(AddCustomForm, {
    props: { kind: 'item', onsubmit: () => {}, oncancel: () => { cancelled += 1; } },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  assert.equal(cancelled, 1);
});
