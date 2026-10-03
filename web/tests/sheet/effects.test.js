/* global URL */
import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import EffectsStrip from '../../src/lib/sheet/components/EffectsStrip.svelte';
import Provenance from '../../src/lib/sheet/components/Provenance.svelte';
import StatTile from '../../src/lib/sheet/components/StatTile.svelte';

// The E8 affordances (spec §2.8 + design §5): chips render from the wire's
// effect rows; a provenance hover shows applied entries and suppressed
// entries with the engine's reason text — verbatim, no UI inference.

test('the effects strip renders a chip per active effect: name, source, duration', () => {
  const { body } = render(EffectsStrip, {
    props: {
      effects: [
        {
          effect_id: 41,
          name: 'Bless',
          source_name: 'Lorum Ipsum',
          duration_note: '10 rounds',
          active: true,
          tracked_manually: false,
        },
        {
          effect_id: 48,
          name: 'Fascinated',
          source_name: 'Corpus',
          duration_note: '',
          active: true,
          tracked_manually: true,
        },
      ],
    },
  });
  assert.match(body, /Bless/);
  assert.match(body, /Lorum Ipsum/, 'the source rides the chip');
  assert.match(body, /10 rounds/, 'the duration rides the title');
  assert.match(body, /Fascinated/);
  assert.match(body, /tracked/, 'a display-only condition badges itself');
});

test('the strip renders nothing when no effects target the character', () => {
  const { body } = render(EffectsStrip, { props: { effects: [] } });
  assert.doesNotMatch(body, /chip/, 'no placeholder theatre');
  const empty = render(EffectsStrip, { props: {} }).body;
  assert.doesNotMatch(empty, /chip/, 'a missing array is the honest empty');
});

test('a provenance hover shows the applied entry and the suppressed entry with its reason', () => {
  const { body } = render(Provenance, {
    props: {
      applied: [
        { type: 'status', value: 1, effect_id: 41, effect_name: 'Bless', source_character_id: 3 },
      ],
      suppressed: [
        {
          type: 'status',
          value: 1,
          effect_id: 48,
          effect_name: 'Inspire Courage',
          source_character_id: 5,
          reason: 'same-type-lower-bonus',
          suppressed_by_effect_id: 41,
        },
      ],
    },
  });
  assert.match(body, /\+1 status — Bless/, 'the applied entry, engine-verbatim');
  assert.match(body, /\+1 status — Inspire Courage · same-type-lower-bonus/, 'the suppressed entry with its reason text');
  assert.match(body, /Not stacked/, 'the two lists are named apart');
  assert.match(body, /tabindex="0"/, 'the hover is focusable (design §9)');
  assert.match(
    body,
    /aria-label="Applied: \+1 status — Bless\. Not stacked: \+1 status — Inspire Courage · same-type-lower-bonus\."/,
    'the full breakdown is announced to the tree',
  );
});

test('a provenance hover renders nothing for untouched numbers', () => {
  const { body } = render(Provenance, { props: { applied: [], suppressed: [] } });
  assert.doesNotMatch(body, /class="prov"/, 'no hover theatre on a plain number');
});

test('a stat tile carries the hover its stat deserves', () => {
  const { body } = render(StatTile, {
    props: {
      label: 'AC',
      value: 19,
      cls: 't-ac gold',
      provenance: {
        applied: [
          { type: 'status', value: 1, effect_id: 41, effect_name: 'Bless', source_character_id: 3 },
        ],
        suppressed: [],
      },
    },
  });
  assert.match(body, /19/);
  assert.match(body, /Bless/, 'the tile forwards its stat\u2019s provenance to the hover');
});
