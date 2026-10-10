import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import { createCustomStore, validateCustom } from '../../src/lib/rules/custom-store.js';

// The custom-rows client (E9 T7, contracts/custom-rows-rest.md): the only
// module that talks to the custom REST. Client caps mirror the server's
// (defense in depth, FR-4 AC-3): an invalid row never leaves the client.

const PARTY = 1;

/** @param {*} body @param {number} [status] */
function jsonRoute(body, status = 200) {
  return /** @type {typeof fetch} */ (
    /** @returns {Promise<Response>} */ async () =>
      /** @type {any} */ ({
        ok: status >= 200 && status < 300,
        status,
        json: async () => body,
      })
  );
}

/** Records calls, answers from a queue. @param {Array<{body: any, status: number}>} answers */
function fetchSpy(answers) {
  /** @type {Array<{url: string, init: any}>} */
  const calls = [];
  const impl = /** @type {typeof fetch} */ (
    /** @param {string} url @param {any} init */ async (url, init) => {
      calls.push({ url, init });
      const answer = answers.shift() ?? { body: [], status: 200 };
      return /** @type {any} */ ({
        ok: answer.status >= 200 && answer.status < 300,
        status: answer.status,
        json: async () => answer.body,
      });
    }
  );
  return { calls, impl };
}

afterEach(async () => {
  const { cleanup } = await import('@testing-library/svelte');
  cleanup();
});

test('validateCustom mirrors the server caps: name 1..64, description 0..280', () => {
  assert.deepEqual(validateCustom('item', { name: '   ' }).field, 'name');
  assert.equal(validateCustom('item', { name: '' }).reason, 'required (1..64 characters)');
  assert.equal(validateCustom('item', { name: 'x'.repeat(65) }).field, 'name');
  assert.equal(validateCustom('item', { name: 'x'.repeat(64) }), null, 'the cap itself passes');
  assert.equal(validateCustom('item', { name: 'Wagon', description: 'y'.repeat(281) }).field, 'description');
  assert.equal(validateCustom('item', { name: 'Wagon', description: 'y'.repeat(280) }), null);
});

test('validateCustom bounds rank 0..10 for spells and value 1..20 for conditions', () => {
  assert.equal(validateCustom('spell', { name: 'Toads', value_or_rank: -1 }).field, 'value_or_rank');
  assert.equal(validateCustom('spell', { name: 'Toads', value_or_rank: 11 }).field, 'value_or_rank');
  assert.equal(validateCustom('spell', { name: 'Toads', value_or_rank: 10 }), null);
  assert.equal(validateCustom('spell', { name: 'Toads', value_or_rank: 0 }), null);
  assert.equal(validateCustom('condition', { name: 'Sunlit', value_or_rank: 0 }).field, 'value_or_rank');
  assert.equal(validateCustom('condition', { name: 'Sunlit', value_or_rank: 21 }).field, 'value_or_rank');
  assert.equal(validateCustom('condition', { name: 'Sunlit', value_or_rank: null }), null, 'optional');
  assert.equal(validateCustom('item', { name: 'Wagon', value_or_rank: 3 }).field, 'value_or_rank', 'items take no value');
});

test('create posts the row and inserts it optimistically — the store answers the surface', async () => {
  const spy = fetchSpy([
    {
      status: 201,
      body: {
        corpus_entry_id: 91,
        kind: 'spell',
        name: 'Conjure Toad Swarm',
        lane: 'custom',
        description: 'So many toads',
        value_or_rank: 3,
        created_by_sub: 'dev-sub-josh',
      },
    },
  ]);
  const store = createCustomStore({ partyId: PARTY, fetchImpl: spy.impl });
  const row = await store.create('spell', { name: '  Conjure Toad Swarm  ', description: 'So many toads', value_or_rank: 3 });
  assert.equal(row.corpus_entry_id, 91);
  assert.equal(spy.calls.length, 1, 'one request');
  assert.equal(spy.calls[0].url, '/api/parties/1/custom');
  const sent = JSON.parse(spy.calls[0].init.body);
  assert.equal(sent.kind, 'spell');
  assert.equal(sent.name, 'Conjure Toad Swarm', 'trimmed before the wire');
  const spells = await new Promise((resolve) => store.spells.subscribe(resolve)());
  assert.equal(spells.length, 1, 'the optimistic insert surfaced');
  assert.equal(spells[0].name, 'Conjure Toad Swarm');
});

test('an invalid row never leaves the client — no request, the field and reason come back', async () => {
  const spy = fetchSpy([]);
  const store = createCustomStore({ partyId: PARTY, fetchImpl: spy.impl });
  await assert.rejects(
    store.create('item', { name: 'x'.repeat(65), description: '' }),
    /** @param {any} error */ (error) => error.field === 'name' && /64/.test(error.reason),
  );
  assert.equal(spy.calls.length, 0, 'no request for a row the caps already refuse');
});

test('a 400 from the server surfaces its field and reason', async () => {
  const spy = fetchSpy([
    { status: 400, body: { error: 'validation', field: 'name', reason: 'too long' } },
  ]);
  const store = createCustomStore({ partyId: PARTY, fetchImpl: spy.impl });
  await assert.rejects(
    store.create('item', { name: 'Wagon', description: '' }),
    /** @param {any} error */ (error) => error.field === 'name' && error.reason === 'too long',
  );
});

test('refresh reads a kind into its store — the party-wide pick-up path (design D3)', async () => {
  const spy = fetchSpy([
    {
      status: 200,
      body: [
        {
          corpus_entry_id: 90,
          kind: 'item',
          name: 'Named Wagon',
          lane: 'custom',
          description: 'Looted',
          value_or_rank: null,
          created_by_sub: 'dev-sub-bear',
        },
      ],
    },
  ]);
  const store = createCustomStore({ partyId: PARTY, fetchImpl: spy.impl });
  await store.refresh('item');
  assert.equal(spy.calls[0].url, '/api/parties/1/custom?kind=item');
  const items = await new Promise((resolve) => store.items.subscribe(resolve)());
  assert.equal(items.length, 1);
  assert.equal(items[0].name, 'Named Wagon');
});
