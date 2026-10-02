import { test } from 'vitest';
import assert from 'node:assert/strict';

import { logoutAction } from '../src/lib/logout.js';

test('a confirmed logout shows the signed-out screen', () => {
  assert.equal(logoutAction(204), 'signed-out');
});

test('an unconfirmed logout keeps the signed-in view for retry', () => {
  // 500 = the server could not confirm invalidation; the cookie — and so
  // the session — is still live, so claiming signed-out would be a lie.
  assert.equal(logoutAction(500), 'retry');
  assert.equal(logoutAction(502), 'retry');
  assert.equal(logoutAction(503), 'retry');
});

test('a logout request that never completes keeps the signed-in view', () => {
  assert.equal(logoutAction(null), 'retry');
});

test('no status short of a confirmed 204 reads as signed-out', () => {
  // In particular an odd auth failure from the logout leg itself must not
  // flip the screen: only the server's confirmed answer may.
  assert.equal(logoutAction(401), 'retry');
  assert.equal(logoutAction(403), 'retry');
  assert.equal(logoutAction(404), 'retry');
  assert.equal(logoutAction(200), 'retry');
});
