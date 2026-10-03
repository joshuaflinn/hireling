/* global URL */
import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import CompanionsPanel from '../../src/lib/sheet/components/CompanionsPanel.svelte';
import { derive } from '../../src/lib/engine/index.js';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));
const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });

test('the familiar card derives from the owner: HP 5×level, owner AC/saves, skill lines', () => {
  const { body } = render(CompanionsPanel, {
    props: { baseSheet: fixture, view },
  });
  assert.match(body, /Pippin/, 'the export names the familiar');
  assert.match(body, /HP<b[^>]*>15<\/b>/, '5 × level 3');
  assert.match(body, /AC<b[^>]*>16<\/b>/, "the owner's AC");
  assert.match(body, /Fort<b[^>]*>\+7<\/b>/, "the owner's saves");
  assert.match(body, /Perception<\/b> \+6/, '3 + level skill line');
  assert.match(body, /Fast Movement \(Land\)/, 'abilities render');
  assert.match(body, /Command an Animal/, 'the export takes the Pet feat, so the pet note');
  assert.match(body, /Pet feat: HP 5 per level/, 'the pet source line');
});

test('an absent companion renders a clean empty note, not a blank', () => {
  const bare = { ...fixture, companions: [] };
  const { body } = render(CompanionsPanel, {
    props: { baseSheet: bare, view },
  });
  assert.match(body, /No pet or familiar/);
});
