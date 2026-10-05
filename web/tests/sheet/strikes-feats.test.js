import { render, cleanup } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (extractor + compute over the same export — the swap's byte-identity
 * fixture). */
import view from '../data/engine_output_reference.json';

import StrikesPane from '../../src/lib/sheet/components/StrikesPane.svelte';
import FeatsPanel from '../../src/lib/sheet/components/FeatsPanel.svelte';

afterEach(cleanup);

test('strikes render at the fixture: attack, MAP row, damage, trait names', () => {
  const { container } = render(StrikesPane, { props: { view } });
  assert.match(container.innerHTML, /Staff/);
  assert.match(container.innerHTML, /atk \+4/, "the export's own attack number");
  assert.match(container.innerHTML, /MAP −5 \/ −10/, 'not agile');
  assert.match(container.innerHTML, /d4−1/, 'damage dice with the STR bonus');
  assert.match(container.innerHTML, /Fist/, 'unarmed rides along');
  assert.match(container.innerHTML, /Agile/, 'trait names render');
  assert.equal(container.querySelectorAll('button').length, 0, 'strikes are read-only');
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
  const { container } = render(StrikesPane, { props: { view: adjusted } });
  assert.match(container.innerHTML, /atk \+5/, 'the wire\u2019s number, not a local derivation');
});

test('feats render level-grouped with categories; features tab from specials', () => {
  const feats = render(FeatsPanel, { props: { baseSheet: fixture, editable: true } }).container;
  assert.match(feats.innerHTML, /Level 1/);
  assert.match(feats.innerHTML, /Charming Liar/);
  assert.match(feats.innerHTML, /Class Feat/, 'categories render as chips');
  assert.doesNotMatch(feats.innerHTML, /Arcane School/, 'features live on the other tab');

  cleanup();
  const features = render(FeatsPanel, {
    props: { baseSheet: fixture, editable: false },
  }).container;
  assert.match(features.innerHTML, /Arcane School: School of Mentalism/, 'features render when view-only');
});

test('empty feats render a clean note', () => {
  const bare = { ...fixture, raw: { ...fixture.raw, feats: [], specials: [] } };
  const { container } = render(FeatsPanel, { props: { baseSheet: bare, editable: false } });
  assert.match(container.innerHTML, /No feats recorded/);
});
