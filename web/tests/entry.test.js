import test from 'node:test';
import assert from 'node:assert/strict';

import { entryAction } from '../src/lib/entry.js';

test('an unauthenticated visitor is sent to the login leg', () => {
  assert.equal(entryAction(401), 'enter');
});

test('a forbidden probe reads as not-signed-in and enters', () => {
  assert.equal(entryAction(403), 'enter');
});

test('a broken backend keeps the visitor on the page', () => {
  assert.equal(entryAction(500), 'offline');
  assert.equal(entryAction(502), 'offline');
  assert.equal(entryAction(503), 'offline');
});

test('anything unrecognised does not send the visitor away', () => {
  assert.equal(entryAction(302), 'offline');
  assert.equal(entryAction(404), 'offline');
  assert.equal(entryAction(0), 'offline');
});
