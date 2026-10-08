import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  classifyRequest,
  shellCacheName,
  precacheList,
  obsoleteCaches,
} from '../../src/lib/pwa/strategy.js';

// Contract: contracts/sw-shell-cache.md §3. The SW owns the shell and
// nothing else — every /api/* request (and every WS) is bypassed, whatever
// its mode. Rule order matters: an API navigation is still API.
test('api paths are bypass for every mode — navigations included', () => {
  for (const mode of ['navigate', 'cors', 'same-origin', 'no-cors', 'websocket']) {
    assert.equal(classifyRequest('/api/party/roster', mode), 'bypass', `roster ${mode}`);
    assert.equal(classifyRequest('/api/ws/party/1', mode), 'bypass', `ws ${mode}`);
  }
  assert.equal(classifyRequest('/api/me', 'navigate'), 'bypass');
});

test('built assets, icons and the manifest are assets', () => {
  assert.equal(classifyRequest('/assets/index-a1b2c3.js', 'same-origin'), 'asset');
  assert.equal(classifyRequest('/assets/index-9f8e7d.css', 'cors'), 'asset');
  assert.equal(classifyRequest('/icon-192.png', 'no-cors'), 'asset');
  assert.equal(classifyRequest('/icon-512.png', 'same-origin'), 'asset');
  assert.equal(classifyRequest('/icon-maskable-512.png', 'same-origin'), 'asset');
  assert.equal(classifyRequest('/manifest.webmanifest', 'same-origin'), 'asset');
});

test('navigations that are not API and not assets are the shell', () => {
  assert.equal(classifyRequest('/', 'navigate'), 'navigation');
  assert.equal(classifyRequest('/party', 'navigate'), 'navigation');
});

test('everything else is bypass — the SW adds no fetch theatre', () => {
  assert.equal(classifyRequest('/whatever.txt', 'same-origin'), 'bypass');
  assert.equal(classifyRequest('/robots.txt', 'cors'), 'bypass');
  // The API carve-out is the exact /api/ prefix; anything else navigated
  // is the shell (network-first, cached index.html behind it).
  assert.equal(classifyRequest('/api-not-API/x', 'navigate'), 'navigation');
});

test('the cache name carries the build id', () => {
  assert.equal(shellCacheName('abc123'), 'hireling-shell-abc123');
});

test('precacheList keeps the shell files and nothing else', () => {
  const dist = [
    'index.html',
    'assets/index-a1b2c3.js',
    'assets/index-9f8e7d.css',
    'manifest.webmanifest',
    'icon-192.png',
    'icon-512.png',
    'icon-maskable-512.png',
    'favicon.ico', // not shell-owned: never precached
    'notes.txt',
  ];
  assert.deepEqual(precacheList(dist), [
    '/index.html',
    '/assets/index-a1b2c3.js',
    '/assets/index-9f8e7d.css',
    '/manifest.webmanifest',
    '/icon-192.png',
    '/icon-512.png',
    '/icon-maskable-512.png',
  ]);
});

test('obsoleteCaches keeps the current shell cache and foreign caches alone', () => {
  const caches = [
    'hireling-shell-old1',
    'hireling-shell-abc123',
    'hireling-shell-old2',
    'some-other-app-cache',
  ];
  assert.deepEqual(obsoleteCaches(caches, 'abc123'), [
    'hireling-shell-old1',
    'hireling-shell-old2',
  ]);
});

test('obsoleteCaches with nothing to drop drops nothing', () => {
  assert.deepEqual(obsoleteCaches(['hireling-shell-abc123'], 'abc123'), []);
});
