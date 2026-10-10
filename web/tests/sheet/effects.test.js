import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import { render, cleanup, screen, fireEvent } from '@testing-library/svelte';

import EffectsStrip from '../../src/lib/sheet/components/EffectsStrip.svelte';
import Provenance from '../../src/lib/sheet/components/Provenance.svelte';
import StatTile from '../../src/lib/sheet/components/StatTile.svelte';

// The E8 affordances (spec §2.8 + design §5): chips render from the wire's
// effect rows; a provenance hover shows applied entries and suppressed
// entries with the engine's reason text — verbatim, no UI inference.

afterEach(cleanup);

test('the effects strip renders a chip per active effect: name, source, duration', () => {
  const body = render(EffectsStrip, {
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
  }).container.innerHTML;
  assert.match(body, /Bless/);
  assert.match(body, /Lorum Ipsum/, 'the source rides the chip');
  assert.match(body, /10 rounds/, 'the duration rides the title');
  assert.match(body, /Fascinated/);
  assert.match(body, /tracked/, 'a display-only condition badges itself');
});

test('the strip renders nothing when no effects target the character', () => {
  const body = render(EffectsStrip, { props: { effects: [] } }).container.innerHTML;
  assert.doesNotMatch(body, /chip/, 'no placeholder theatre');
  const empty = render(EffectsStrip, { props: {} }).container.innerHTML;
  assert.doesNotMatch(empty, /chip/, 'a missing array is the honest empty');
});

test('a provenance hover shows the applied entry and the suppressed entry with its reason', () => {
  const body = render(Provenance, {
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
  }).container.innerHTML;
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
  const body = render(Provenance, { props: { applied: [], suppressed: [] } }).container.innerHTML;
  assert.doesNotMatch(body, /class="prov"/, 'no hover theatre on a plain number');
});

test('a stat tile carries the hover its stat deserves', () => {
  const body = render(StatTile, {
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
  }).container.innerHTML;
  assert.match(body, /19/);
  assert.match(body, /Bless/, 'the tile forwards its stat\u2019s provenance to the hover');
});

// ---- E9 Task 4: chips and provenance entries are ConditionTip triggers ----


/** The tip popup inside a rendered surface, when open. @param {HTMLElement} container */
function tipPopup(container) {
  return /** @type {HTMLElement | null} */ (container.querySelector('.pop'));
}

test('a Frightened chip is a tooltip trigger: hover shows prose, cite, and the AoN href', () => {
  const { container } = render(EffectsStrip, {
    props: {
      effects: [
        {
          effect_id: 41,
          name: 'Frightened',
          source_name: 'Lorum Ipsum',
          duration_note: '',
          active: true,
          tracked_manually: false,
        },
      ],
    },
  });
  const chip = screen.getByRole('button', { name: /Frightened/ });
  fireEvent.mouseEnter(chip);
  const pop = /** @type {HTMLElement} */ (tipPopup(container));
  assert.match(pop.innerHTML, /Status penalty equal to the value/, 'curated prose via the strip');
  assert.match(pop.innerHTML, /Player Core p\. 444/, 'the cite');
  assert.match(
    pop.innerHTML,
    /Conditions\.aspx\?ID=76/,
    'the AoN anchor rides the same popup',
  );
  fireEvent.mouseLeave(chip);
  assert.equal(tipPopup(container), null, 'unpinned hover closes on leave');
});

test('two fixtures: a joined custom condition shows its description; without the join, the fallback', () => {
  const joined = render(EffectsStrip, {
    props: {
      effects: [
        {
          effect_id: 44,
          name: 'Sunlit',
          source_name: 'Lorum Ipsum',
          duration_note: '',
          active: true,
          tracked_manually: true,
        },
      ],
      customConditions: [{ name: 'Sunlit', description: 'Standing in the sun', value_or_rank: 2 }],
    },
  });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Sunlit/ }));
  const pop = /** @type {HTMLElement} */ (tipPopup(joined.container));
  assert.match(pop.innerHTML, /Standing in the sun/, 'the creator description');
  assert.match(pop.innerHTML, /custom/, 'the custom badge');
  assert.match(pop.innerHTML, /Value: 2/, 'the value display note');
  joined.unmount();

  const orphan = render(EffectsStrip, {
    props: {
      effects: [
        {
          effect_id: 45,
          name: 'Sunlit',
          source_name: 'Lorum Ipsum',
          duration_note: '',
          active: true,
          tracked_manually: true,
        },
      ],
    },
  });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Sunlit/ }));
  assert.match(
    /** @type {HTMLElement} */ (tipPopup(orphan.container)).innerHTML,
    /No paraphrase yet/i,
    'no join data — the honest fallback, never invented prose',
  );
});

test('a provenance entry named for a condition is a tooltip trigger too', () => {
  const { container } = render(Provenance, {
    props: {
      applied: [
        { type: 'status', value: 1, effect_id: 41, effect_name: 'Frightened', source_character_id: 3 },
      ],
      suppressed: [],
    },
  });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Frightened/ }));
  const pop = /** @type {HTMLElement} */ (tipPopup(container));
  assert.match(pop.innerHTML, /Status penalty equal to the value/, 'the breakdown entry opens the tip');
});
