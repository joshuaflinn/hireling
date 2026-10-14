import { render, screen, cleanup } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';
/** The reference EngineOutput, verbatim as the wire carries it. */
import engine from '../data/engine_output_reference.json';

import StatsPane from '../../src/lib/sheet/components/StatsPane.svelte';

// StatsPane tile-alignment guards (E14 design D5/D6, spec FR-1): the
// spell-DC selection and the class-DC tile's absence rule, pinned at render
// level through the mounted pane. The unit suites (engine/caster.test.js,
// party/stat-block.test.js) pin the rule; this file pins the sheet.

afterEach(cleanup);

/** The pane mounted on a (cloned) engine output — the production mount the
 * sheet tests already use. @param {any} view */
function mountPane(view) {
  const noop = () => {};
  return render(StatsPane, {
    props: {
      view,
      baseSheet: fixture,
      hp: { value: 20, pending: false },
      temp: { value: 0, pending: false },
      focusCurrent: { value: 0, pending: false },
      focusMax: 1,
      heroPoints: { value: 1, pending: false },
      heroMax: 3,
      ondamage: noop,
      onheal: noop,
      onfull: noop,
      ontemp: noop,
      onfocus: noop,
      onhero: noop,
    },
  });
}

/** The pane's spell-DC tile, for value/note assertions.
 * @param {ReturnType<typeof render>} mounted */
function spellDcTile(mounted) {
  const tile = mounted.container.querySelector('.t-spelldc');
  assert.ok(tile, 'the spell-DC tile renders');
  return /** @type {Element} */ (tile);
}

test('innate caster ordered first: the pane shows the non-innate block spell numbers', () => {
  // The render-level twin of caster.test.js's discriminating case: on the
  // reference fixture's ordering the two rules agree, so the fixture is
  // reordered [innate, non-innate] here — a degraded extraction (plain
  // `casters[0]`) would render the innate block's 18 / attack +8.
  const view = structuredClone(engine);
  view.derived.casters.reverse();
  assert.equal(view.derived.casters[0].innate, true, 'fixture precondition: innate first');
  const mounted = mountPane(view);
  const tile = spellDcTile(mounted);
  assert.match(
    /** @type {HTMLElement} */ (tile.querySelector('.v'))?.textContent ?? '',
    /^\s*19\s*$/,
    'the non-innate block spell DC, not the innate 18',
  );
  assert.match(
    /** @type {HTMLElement} */ (tile.querySelector('.x'))?.textContent ?? '',
    /attack \+9/,
    'the non-innate block attack note, not the innate +8',
  );
  screen.getByText('Spell DC');
});

test('a character with a class DC renders the tile with its value and note', () => {
  // The non-null side of the boundary (E14 task 1b): the reference fixture's
  // Wizard carries class_dc base 19 — the tile renders as before.
  const mounted = mountPane(structuredClone(engine));
  const tile = mounted.container.querySelector('.t-classdc');
  assert.ok(tile, 'the class-DC tile renders for a non-null base');
  assert.match(
    /** @type {HTMLElement} */ (tile.querySelector('.v'))?.textContent ?? '',
    /^\s*19\s*$/,
  );
  assert.match(
    /** @type {HTMLElement} */ (tile.querySelector('.x'))?.textContent ?? '',
    /INT/,
    'the key-ability note renders as before',
  );
});

test('class_dc.base null: no Class DC tile — absence, never a dash', () => {
  // The null side (E14 task 1b): contract §1 — the slot's output is null,
  // nothing is invented. The old unconditional tile rendered a bare '—'
  // with no provenance and no information; it must be gone, not dashed.
  const view = structuredClone(engine);
  view.derived.class_dc.base = null;
  view.derived.class_dc.total = null;
  const mounted = mountPane(view);
  assert.equal(
    mounted.container.querySelector('.t-classdc'),
    null,
    'no class-DC tile on the sheet, matching the card',
  );
  assert.equal(
    mounted.container.textContent?.includes('Class DC'),
    false,
    'no Class DC label anywhere on the pane',
  );
});
