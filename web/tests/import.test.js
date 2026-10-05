// Import-page logic tests (E5 FR-18): one POST body for paste and file,
// the error mapping, and a render line for every diff kind (data-model §5).
import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  buildBody,
  describeOutcome,
  errorFor,
  readFileToText,
  submit,
} from '../src/lib/import.js';

test('paste and file paths produce identical POST bodies', async () => {
  const pasted = '{"success":true,"build":{"name":"Lorum Ipsum"}}';
  const file = new globalThis.File([pasted], 'export.json', { type: 'application/json' });
  const fromFile = await readFileToText(file);
  assert.equal(buildBody(fromFile), buildBody(pasted), 'same body either way');
});

test('error render maps the server code to its message, with fallbacks', () => {
  assert.equal(
    errorFor({
      ok: false,
      status: 400,
      payload: {
        error: {
          code: 'invalid-json',
          message: 'That isn\u2019t valid JSON. Copy the whole export from Pathbuilder.',
        },
      },
    }),
    'That isn\u2019t valid JSON. Copy the whole export from Pathbuilder.',
    'the server\u2019s exact message wins',
  );
  assert.equal(
    errorFor({ ok: false, status: 413, payload: { error: { code: 'payload-too-large' } } }),
    'That file is too large to be a character export.',
    'known code without a message gets the fallback',
  );
  assert.equal(
    errorFor({ ok: false, status: 0, payload: null }),
    'The server is unreachable right now.',
    'a dead server says so',
  );
  assert.equal(
    errorFor({ ok: false, status: 500, payload: null }),
    'The import failed unexpectedly. Try again.',
    'anything else is honest fallback',
  );
});

test('diff render covers every data-model §5 kind', () => {
  const outcome = {
    character: { name: 'Lorum Ipsum', level: 3, first_import: false },
    advisory: { skipped_fields: 2 },
    diff: {
      first_import: false,
      kept_unmatched: [
        { kind: 'slot', caster_key: 'Wizard', rank: 2, slot_index: 2, used: true, prepared: 'Fear' },
        { kind: 'item', name: 'Silver Chunk', qty_delta: -1 },
      ],
      seeded: [{ kind: 'slot', caster_key: 'Wizard', rank: 3, slot_index: 0, prepared: null }],
      prep_divergence: [
        { caster_key: 'Wizard', rank: 1, slot_index: 0, live: 'Fear', export: 'Befuddle' },
      ],
      notices: [
        { kind: 'negative_quantity', name: 'Chalk', base_qty: 2, delta: -3 },
        { kind: 'max_hp_changed', old: 14, new: 22 },
        { kind: 'section_skipped', section: 'equipment' },
      ],
    },
  };
  const lines = describeOutcome(outcome).join('\n');
  assert.match(lines, /Re-imported Lorum Ipsum \(level 3\)/);
  assert.match(lines, /Kept slot Wizard rank 2 #3 \(used\)/, 'kept slot named');
  assert.match(lines, /Kept item Silver Chunk/, 'kept item named');
  assert.match(lines, /Seeded new slot Wizard rank 3 #1/, 'seed named');
  assert.match(lines, /table says "Fear", export said "Befuddle"/, 'prep divergence, live wins');
  assert.match(lines, /Chalk would show -1/, 'negative quantity surfaced');
  assert.match(lines, /Max HP changed 14 → 22/);
  assert.match(lines, /Section "equipment" didn't import/, 'section skip named');
  assert.match(lines, /Skipped 2 unrecognized fields/, 'the advisory count');
});

test('a clean first import renders one line and no diff', () => {
  const lines = describeOutcome({
    character: { name: 'Fresh Face', level: 1, first_import: true },
    advisory: { skipped_fields: 0 },
    diff: { first_import: true, kept_unmatched: [], seeded: [], prep_divergence: [], notices: [] },
  });
  assert.deepEqual(
    lines,
    ['Imported Fresh Face (level 1) — welcome to the party.'],
    'first-import responses carry no diff lines',
  );
});

test('submit posts to the import endpoint and reports ok status', async () => {
  const originalFetch = globalThis.fetch;
  let captured = null;
  globalThis.fetch = (url, options) => {
    captured = { url, options };
    return {
      ok: true,
      status: 200,
      json: () => ({
        character: { name: 'X', level: 1, first_import: true },
        diff: { first_import: true, kept_unmatched: [], seeded: [], prep_divergence: [], notices: [] },
        advisory: { skipped_fields: 0 },
      }),
    };
  };
  try {
    const result = await submit('{"success":true}');
    assert.equal(captured.url, '/api/characters/import');
    assert.equal(captured.options.method, 'POST');
    assert.equal(captured.options.body, '{"success":true}', 'the body is verbatim');
    assert.equal(result.ok, true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});
