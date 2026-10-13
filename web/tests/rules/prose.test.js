import { test } from 'vitest';
import assert from 'node:assert/strict';

import seed from '../../src/lib/rules/condition-prose.json';
import frozen from './data/condition-names-frozen.json';
import { lookup } from '../../src/lib/rules/prose.js';

// The curated seed (E9 FR-2/FR-10): a build-time static module joined to
// corpus/effect names by case-insensitive exact match. A name with no entry
// gets the fallback shape — never invented prose (design D9.2).

test('lookup finds by case-insensitive exact name and carries cite + AoN id', () => {
  const frightened = lookup('Frightened');
  assert.equal(frightened.found, true);
  assert.equal(frightened.page, 444);
  assert.equal(frightened.aonId, 76);
  assert.match(frightened.text, /Status penalty equal to the value/);

  // Casing and spacing are the join, nothing else ("off-guard" is stored
  // lowercase-hyphenated; "Persistent Damage" display-cased with a space).
  assert.equal(lookup('off-guard').found, true);
  assert.equal(lookup('OFF-GUARD').aonId, 58);
  assert.equal(lookup('persistent  damage').found, true, 'internal whitespace collapses');
  assert.equal(lookup('  Frightened ').found, true, 'trimmed');
});

test('a name with no curated prose gets the fallback shape, never invented text', () => {
  const miss = lookup('Sunlit');
  assert.equal(miss.found, false);
  assert.equal(miss.text, '');
  assert.equal(miss.page, null);
  assert.equal(miss.aonId, null);
  // The same shape for a renamed freeze condition — the two-fixture rule:
  // the matching fixture above asserts the join, this one asserts the miss.
  assert.equal(lookup('Sluggish').found, false);
});

test('link:false entries are found by lookup but flagged out of linkification', () => {
  assert.equal(lookup('Broken').found, true);
  assert.equal(lookup('Broken').link, false);
  assert.equal(lookup('Controlled').link, false);
  assert.equal(lookup('Frightened').link, true);
});

test('coverage: every seed key joins a pinned freeze name, both directions', () => {
  const keys = Object.keys(seed).filter((key) => key !== '_readme');
  const names = /** @type {string[]} */ (frozen.names);
  assert.equal(keys.length, 42, 'the freeze carries 42 conditions');
  assert.equal(names.length, 42, 'the fixture is the same freeze');
  const normalize = /** @param {string} s */ (s) => s.toLowerCase().replace(/\s+/g, '-');
  const normalizedNames = new Set(names.map(normalize));
  for (const key of keys) {
    assert.ok(
      normalizedNames.has(normalize(key)),
      `seed key "${key}" has no freeze-list condition name — curation PR needed`,
    );
    const entry = /** @type {any} */ (seed)[key];
    assert.equal(typeof entry.text, 'string', `${key} carries prose`);
    assert.equal(typeof entry.page, 'number', `${key} carries a page cite`);
    assert.equal(typeof entry.aonId, 'number', `${key} carries an AoN id`);
  }
  // And the freeze list carries nothing the seed lacks — a renamed upstream
  // condition orphans its prose entry; this direction is what surfaces it.
  const normalizedKeys = new Set(keys.map(normalize));
  for (const name of names) {
    assert.ok(
      normalizedKeys.has(normalize(name)),
      `freeze name "${name}" has no seed entry — curation PR needed`,
    );
  }
});
