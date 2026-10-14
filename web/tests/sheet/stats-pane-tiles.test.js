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
