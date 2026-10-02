// Harness smoke proof (gh#41): a .svelte component from main mounts in jsdom
// and its spec'd affordances are asserted against the real DOM. The path is
// the point — tests/harness/ is invisible to the old `node --test` glob, and
// this file is what proves the replacement runner sees it.
import { render, screen, cleanup, fireEvent } from '@testing-library/svelte';
import { afterEach, test, vi } from 'vitest';
import assert from 'node:assert/strict';

import ImportPage from '../../src/lib/import/ImportPage.svelte';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

test('the import page renders its affordances into the real DOM', () => {
  render(ImportPage);

  screen.getByRole('heading', { name: 'Import a character' });
  screen.getByPlaceholderText('{"success":true,"build":{…}}');
  screen.getByText('Upload a .json export…');

  // Nothing pasted yet → the import affordance is not offered (E5 FR-18).
  const button = screen.getByRole('button', { name: 'Import' });
  assert.equal(button.disabled, true);
});

// The PR-#35 defect class: behaviour that lives in the component's DOM and
// event wiring — unreachable from a pushed-down .js twin. A real paste, a
// real click, and the visible result the spec promises (E5 FR-18).
test('a paste and a click import through the component and render the diff', async () => {
  const payload = {
    character: { name: 'Lorum Ipsum', level: 3, first_import: true },
    diff: {},
    advisory: { skipped_fields: 0 },
  };
  // No async/await in the stub: `submit` only awaits the returned objects,
  // and resolved promises serve those exactly (and keep eslint's
  // require-await quiet). The mock is kept in a named binding — the bare
  // global `fetch` after stubGlobal is runtime-real but undeclared to the
  // linter (no-undef).
  const fetchMock = vi.fn(() => ({
    ok: true,
    status: 200,
    json: () => Promise.resolve(payload),
  }));
  vi.stubGlobal('fetch', fetchMock);

  render(ImportPage);

  fireEvent.input(screen.getByPlaceholderText('{"success":true,"build":{…}}'), {
    target: { value: '{"success":true,"build":{"name":"Lorum Ipsum"}}' },
  });

  const button = screen.getByRole('button', { name: 'Import' });
  assert.equal(button.disabled, false, 'a paste enables the import affordance');
  fireEvent.click(button);

  const line = await screen.findByText(
    'Imported Lorum Ipsum (level 3) — welcome to the party.',
  );
  assert.equal(line.tagName, 'LI');
  assert.equal(fetchMock.mock.calls[0][0], '/api/characters/import');
});
