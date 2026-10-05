import { render, cleanup } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (extractor + compute over the same export — the swap's byte-identity
 * fixture). */
import view from '../data/engine_output_reference.json';

import CompanionsPanel from '../../src/lib/sheet/components/CompanionsPanel.svelte';

afterEach(cleanup);

test('the familiar card derives from the owner: HP 5×level, owner AC/saves, skill lines', () => {
  const { container } = render(CompanionsPanel, {
    props: { baseSheet: fixture, view },
  });
  assert.match(container.innerHTML, /Pippin/, 'the export names the familiar');
  assert.match(container.innerHTML, /HP<b[^>]*>15<\/b>/, '5 × level 3');
  assert.match(container.innerHTML, /AC<b[^>]*>16<\/b>/, "the owner's AC");
  assert.match(container.innerHTML, /Fort<b[^>]*>\+7<\/b>/, "the owner's saves");
  assert.match(container.innerHTML, /Perception<\/b> \+6/, '3 + level skill line');
  assert.match(container.innerHTML, /Fast Movement \(Land\)/, 'abilities render');
  assert.match(container.innerHTML, /Command an Animal/, 'the export takes the Pet feat, so the pet note');
  assert.match(container.innerHTML, /Pet feat: HP 5 per level/, 'the pet source line');
});

test('an absent companion renders a clean empty note, not a blank', () => {
  const bare = { ...fixture, companions: [] };
  const { container } = render(CompanionsPanel, {
    props: { baseSheet: bare, view },
  });
  assert.match(container.innerHTML, /No pet or familiar/);
});
