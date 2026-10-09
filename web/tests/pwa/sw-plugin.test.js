/* global process */

import { test } from 'vitest';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { composeSw, default as swPlugin } from '../../plugins/sw-plugin.mjs';
// The real strategy rules are what gets inlined — vite's ?raw gives us the
// exact source text the build will inline, so these tests pin the actual
// emitted worker.
import strategySource from '../../src/lib/pwa/strategy.js?raw';

const FAKE_FILES = ['/index.html', '/assets/index-a1b2c3.js', '/manifest.webmanifest'];

function composed({ buildId = 'feedfacefeedface', precache = FAKE_FILES } = {}) {
  return composeSw({ buildId, precache, strategySource });
}

test('the emitted worker carries the build id and the whole precache list', () => {
  const source = composed({ buildId: 'b01d5' });
  assert.match(source, /const BUILD_ID = "b01d5"/);
  for (const file of FAKE_FILES) {
    assert.ok(source.includes(JSON.stringify(file)), `precache entry ${file}`);
  }
});

test('the strategy is inlined, not linked', () => {
  const source = composed();
  assert.ok(source.includes('function classifyRequest('), 'classifyRequest body inlined');
  assert.ok(source.includes('function shellCacheName('), 'shellCacheName body inlined');
  assert.ok(source.includes('function obsoleteCaches('), 'obsoleteCaches body inlined');
  assert.ok(
    !source.includes('../src/lib/pwa/strategy.js'),
    'no import link to the strategy module',
  );
  // The inlined rules are the real ones: a known rule appears verbatim.
  assert.ok(source.includes("pathname.startsWith('/assets/')"));
});

test('the fetch handler returns before any respondWith on the bypass path', () => {
  const source = composed();
  const bypassGuard = source.indexOf("if (kind === 'bypass') return;");
  const firstRespondWith = source.indexOf('.respondWith(');
  assert.ok(bypassGuard >= 0, 'bypass guard present');
  assert.ok(firstRespondWith > bypassGuard, 'bypass returns before respondWith');
  assert.ok(!/if \(kind === 'bypass'\)[\s\S]*respondWith[\s\S]*if \(kind === 'bypass'\)/.test(source));
});

test('the emitted worker parses as a script', () => {
  const source = composed();
  // The Function constructor is the point: parsing the emitted worker is
  // this test's whole subject (and `node --check` on disk backs it up).
  // eslint-disable-next-line no-new-func
  new Function(source); // throws on syntax error
});

test('install precaches, activate claims, fetch routes by class', () => {
  const source = composed();
  assert.ok(/addEventListener\('install'/.test(source));
  assert.ok(/addEventListener\('activate'/.test(source));
  assert.ok(/addEventListener\('fetch'/.test(source));
  assert.ok(source.includes('skipWaiting()'), 'new release takes over at once');
  assert.ok(source.includes('clients.claim()'), 'the open page adopts the new worker');
  assert.ok(source.includes('cacheFirst('), 'asset rule');
  assert.ok(source.includes('networkFirstShell('), 'navigation rule');
  assert.ok(source.includes("caches.match('/index.html')"), 'offline boot fallback');
});

// ---- plugin smoke: closeBundle writes sw.js and nothing else -----------

function writeDist(root, files) {
  for (const file of files) {
    const target = path.join(root, 'dist', file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, `built: ${file}\n`);
  }
}

function distFiles(root) {
  const dist = path.join(root, 'dist');
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const rel = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(rel);
      else out.push(path.relative(dist, rel));
    }
  };
  if (fs.existsSync(dist)) walk(dist);
  return out.sort();
}

test('closeBundle emits a self-contained sw.js into dist and touches nothing else', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hireling-sw-plugin-'));
  try {
    const built = ['index.html', 'assets/index-a1b2c3.js', 'manifest.webmanifest'];
    writeDist(root, built);
    fs.mkdirSync(path.join(root, 'src', 'lib', 'pwa'), { recursive: true });
    fs.writeFileSync(
      path.join(root, 'src', 'lib', 'pwa', 'strategy.js'),
      strategySource,
    );

    const before = distFiles(root);
    const plugin = swPlugin({ root });
    assert.equal(typeof plugin.closeBundle, 'function', 'plugin exposes closeBundle');
    plugin.closeBundle();

    const after = distFiles(root);
    assert.deepEqual(after, [...before, 'sw.js'].sort(), 'only sw.js was added');

    const source = fs.readFileSync(path.join(root, 'dist', 'sw.js'), 'utf8');
    assert.match(source, /const BUILD_ID = "[0-9a-f]+"/, 'content-hash build id');
    assert.ok(source.includes('/assets/index-a1b2c3.js'), 'hashed asset precached');
    assert.ok(source.includes('/manifest.webmanifest'));
    assert.ok(!source.includes('favicon.ico'), 'non-shell files never precached');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('closeBundle over an empty dist emits nothing', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hireling-sw-plugin-'));
  try {
    fs.mkdirSync(path.join(root, 'dist'), { recursive: true });
    fs.mkdirSync(path.join(root, 'src', 'lib', 'pwa'), { recursive: true });
    fs.writeFileSync(
      path.join(root, 'src', 'lib', 'pwa', 'strategy.js'),
      strategySource,
    );
    swPlugin({ root }).closeBundle();
    assert.deepEqual(distFiles(root), [], 'no sw.js without a build');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('the same dist hashes to the same build id (release identity is deterministic)', () => {
  const buildIdOf = (root) => {
    fs.mkdirSync(path.join(root, 'src', 'lib', 'pwa'), { recursive: true });
    fs.writeFileSync(path.join(root, 'src', 'lib', 'pwa', 'strategy.js'), strategySource);
    writeDist(root, ['index.html', 'assets/index-x1y2z3.js']);
    swPlugin({ root }).closeBundle();
    return fs.readFileSync(path.join(root, 'dist', 'sw.js'), 'utf8').match(/BUILD_ID = "([0-9a-f]+)"/)[1];
  };
  const a = fs.mkdtempSync(path.join(os.tmpdir(), 'hireling-sw-id-a-'));
  const b = fs.mkdtempSync(path.join(os.tmpdir(), 'hireling-sw-id-b-'));
  try {
    assert.equal(buildIdOf(a), buildIdOf(b), 'identical builds, identical id');
  } finally {
    fs.rmSync(a, { recursive: true, force: true });
    fs.rmSync(b, { recursive: true, force: true });
  }
});

// The emitted worker is generated; this guards the pipeline that produces it
// (node --check proves the file on disk parses, belt and braces for the
// `new Function` check above on the composed string).
test('node itself accepts the emitted worker syntax', () => {
  const source = composed();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hireling-sw-syntax-'));
  try {
    const file = path.join(root, 'sw.js');
    fs.writeFileSync(file, source);
    execFileSync(process.execPath, ['--check', file]);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
