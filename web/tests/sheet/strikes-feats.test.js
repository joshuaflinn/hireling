/* global URL */
import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import StrikesPane from '../../src/lib/sheet/components/StrikesPane.svelte';
import FeatsPanel from '../../src/lib/sheet/components/FeatsPanel.svelte';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));
const ENGINE_PATH = fileURLToPath(new URL('../data/engine_output_reference.json', import.meta.url));
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (extractor + compute over the same export — the swap's byte-identity
 * fixture). */
const view = JSON.parse(await readFile(ENGINE_PATH, 'utf8'));

test('strikes render at the fixture: attack, MAP row, damage, trait names', () => {
  const { body } = render(StrikesPane, { props: { view } });
  assert.match(body, /Staff/);
  assert.match(body, /atk \+4/, "the export's own attack number");
  assert.match(body, /MAP −5 \/ −10/, 'not agile');
  assert.match(body, /d4−1/, 'damage dice with the STR bonus');
  assert.match(body, /Fist/, 'unarmed rides along');
  assert.match(body, /Agile/, 'trait names render');
  assert.doesNotMatch(body, /<button/, 'strikes are read-only');
});

test('strikes render whatever the wire says — no client math, ever', () => {
  // Post-swap the re-derivation is the server's (D4/D6): the pane renders
  // the payload verbatim. A different payload renders different numbers —
  // here, the +1 adjust's output the extractor's golden pins (atk +5 at
  // eff_level 4), fed straight through.
  const adjusted = {
    ...view,
    derived: {
      ...view.derived,
      strikes: view.derived.strikes.map((strike) =>
        strike.key === 'Staff'
          ? { ...strike, attack: { ...strike.attack, base: 5, total: 5 } }
          : strike,
      ),
    },
  };
  const { body } = render(StrikesPane, { props: { view: adjusted } });
  assert.match(body, /atk \+5/, 'the wire\u2019s number, not a local derivation');
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
