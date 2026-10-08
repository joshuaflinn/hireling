import { test } from 'vitest';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import { registerPwa } from '../../src/lib/pwa/register.js';

// vitest runs with the package dir as cwd (`npm --prefix web test`), so the
// committed statics resolve the same way the gate and CI read them.
const ROOT = process.cwd();

// ---- registration (the update policy's page half) ----------------------

function fakeReg({ controller = null } = {}) {
  const listeners = {};
  const calls = { register: [] };
  return {
    controller,
    calls,
    addEventListener(type, fn) {
      (listeners[type] ??= []).push(fn);
    },
    emit(type) {
      for (const fn of listeners[type] ?? []) fn();
    },
    register(relPath, opts) {
      calls.register.push([relPath, opts]);
      return Promise.resolve({});
    },
  };
}

test('PROD registers /sw.js at scope /', () => {
  const reg = fakeReg();
  registerPwa({ PROD: true }, reg);
  assert.deepEqual(reg.calls.register, [['/sw.js', { scope: '/' }]]);
});

test('DEV never touches the service worker — the dev server stays SW-free', () => {
  // Two fixtures on env (AGENTS.md imported-field rule): same reg, PROD
  // flipped off, and the registration surface is never used.
  const reg = fakeReg();
  registerPwa({ PROD: false }, reg);
  assert.deepEqual(reg.calls.register, []);
  let fired = false;
  reg.addEventListener = () => {
    fired = true;
  };
  registerPwa({ PROD: false }, reg);
  assert.equal(fired, false, 'not even a listener is attached in dev');
});

test('a release takeover reloads exactly once when a controller existed', () => {
  const reg = fakeReg({ controller: {} }); // the page was already SW-controlled
  let reloads = 0;
  registerPwa({ PROD: true }, reg, () => {
    reloads += 1;
  });
  reg.emit('controllerchange');
  reg.emit('controllerchange'); // a second takeover must not reload again
  assert.equal(reloads, 1);
});

test('a first install never reloads — no controller existed at register time', () => {
  const reg = fakeReg({ controller: null });
  let reloads = 0;
  registerPwa({ PROD: true }, reg, () => {
    reloads += 1;
  });
  reg.emit('controllerchange');
  assert.equal(reloads, 0);
});

test('registerPwa with the production defaults wires the real navigator', () => {
  // jsdom has no serviceWorker: the default-parameter path must bail
  // cleanly rather than throw inside mount-time code.
  assert.doesNotThrow(() => registerPwa({ PROD: true }));
});

// ---- statics contract (committed files, read node-side) ----------------

const manifest = JSON.parse(
  fs.readFileSync(path.join(ROOT, 'public', 'manifest.webmanifest'), 'utf8'),
);

test('manifest parses with the installability fields', () => {
  assert.equal(manifest.name, 'Hireling');
  assert.equal(manifest.short_name, 'Hireling');
  assert.equal(manifest.start_url, '/');
  assert.equal(manifest.scope, '/');
  assert.equal(manifest.display, 'standalone');
  assert.ok(manifest.background_color, 'background color present');
  assert.ok(manifest.theme_color, 'theme color present');
  assert.ok(Array.isArray(manifest.icons));
});

test('manifest lists 192, 512 and maskable icons', () => {
  const bySrc = new Map(manifest.icons.map((icon) => [icon.src, icon]));
  assert.deepEqual(bySrc.get('/icon-192.png'), {
    src: '/icon-192.png',
    sizes: '192x192',
    type: 'image/png',
  });
  assert.deepEqual(bySrc.get('/icon-512.png'), {
    src: '/icon-512.png',
    sizes: '512x512',
    type: 'image/png',
  });
  assert.deepEqual(bySrc.get('/icon-maskable-512.png'), {
    src: '/icon-maskable-512.png',
    sizes: '512x512',
    type: 'image/png',
    purpose: 'maskable',
  });
});

test('every listed icon file exists and is a real PNG of its claimed size', () => {
  for (const icon of manifest.icons) {
    const bytes = fs.readFileSync(path.join(ROOT, 'public', icon.src));
    assert.ok(bytes.length > 0, `${icon.src} non-empty`);
    assert.deepEqual(
      [...bytes.subarray(0, 8)],
      [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
      `${icon.src} starts with the PNG magic bytes`,
    );
    // IHDR: width at byte 16, height at 20 (big-endian). The generator
    // cannot lie about sizes — the pixels are checked, not the manifest.
    const [width, height] = icon.sizes.split('x');
    assert.equal(bytes.readUInt32BE(16), Number(width), `${icon.src} width`);
    assert.equal(bytes.readUInt32BE(20), Number(height), `${icon.src} height`);
  }
});

test('manifest colors come from the app palette (:root tokens)', () => {
  // The plan's hexes were from memory; the binding instruction is to take
  // them from web/src/app.css. Panel is the app chrome color.
  const css = fs.readFileSync(path.join(ROOT, 'src', 'app.css'), 'utf8');
  const panel = css.match(/--panel:\s*(#[0-9a-f]{6})/)?.[1];
  assert.ok(panel, 'app.css defines --panel');
  assert.equal(manifest.theme_color, panel);
  assert.equal(manifest.background_color, panel);
});

test('index.html links the manifest, an icon, and carries theme-color', () => {
  const html = fs.readFileSync(path.join(ROOT, 'index.html'), 'utf8');
  assert.match(html, /<link\s+rel="manifest"\s+href="\/manifest.webmanifest"\s*\/?>/);
  assert.match(html, /<meta\s+name="theme-color"\s+content="#[0-9a-f]{6}"\s*\/?>/);
  assert.match(html, /<link\s+rel="icon"[^>]*href="\/icon-192\.png"/);
});
