import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import StrikesPane from '../../src/lib/sheet/components/StrikesPane.svelte';
import FeatsPanel from '../../src/lib/sheet/components/FeatsPanel.svelte';
import { derive } from '../../src/lib/engine/index.js';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));
const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });

test('strikes render at the fixture: attack, MAP row, damage, trait names', () => {
  const { body } = render(StrikesPane, { props: { view } });
  assert.match(body, /Staff/);
  assert.match(body, /atk \+4/, "the export's own attack number");
  assert.match(body, /MAP −5 \/ −10/, 'not agile');
  assert.match(body, /d4−1/, 'damage dice with the STR bonus');
  assert.match(body, /Fist/, 'unarmed rides along');
  assert.match(body, /Agile/, 'trait names render');
  assert.doesNotMatch(body, /<button/), 'strikes are read-only';
});

test('strikes re-derive with the level adjust', () => {
  const adjusted = derive({ id: 7, base_sheet: fixture }, { level_adjust: 2, effects: [] });
  const { body } = render(StrikesPane, { props: { view: adjusted } });
  assert.match(body, /atk \+6/, 'level 5 simple: STR −1 + trained 7');
});

test('feats render level-grouped with categories; features tab from specials', () => {
  const feats = render(FeatsPanel, { props: { baseSheet: fixture, editable: true } }).body;
  assert.match(feats, /Level 1/);
  assert.match(feats, /Charming Liar/);
  assert.match(feats, /Class Feat/, 'categories render as chips');
  assert.doesNotMatch(feats, /Arcane School/, 'features live on the other tab');

  const features = render(FeatsPanel, {
    props: { baseSheet: fixture, editable: false },
  }).body;
  assert.match(features, /Arcane School: School of Mentalism/, 'features render when view-only');
});

test('empty feats render a clean note', () => {
  const bare = { ...fixture, raw: { ...fixture.raw, feats: [], specials: [] } };
  const { body } = render(FeatsPanel, { props: { baseSheet: bare, editable: false } });
  assert.match(body, /No feats recorded/);
});
