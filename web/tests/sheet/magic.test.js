import { render, cleanup, screen, fireEvent } from '@testing-library/svelte';
import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import fixture from '../data/base_sheet_reference.json';
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (extractor + compute over the same export — the swap's byte-identity
 * fixture). */
import view from '../data/engine_output_reference.json';

import CasterPanel from '../../src/lib/sheet/components/CasterPanel.svelte';
import MagicPane from '../../src/lib/sheet/components/MagicPane.svelte';
import StaffPanel from '../../src/lib/sheet/components/StaffPanel.svelte';
import SpellSlotRow from '../../src/lib/sheet/components/SpellSlotRow.svelte';

const noop = () => {};

/** Bootstrap-shaped slot rows for the fixture's casters. */
function fixtureSlots() {
  const slots = [];
  for (const caster of fixture.spellcasters) {
    caster.per_day.forEach((count, rank) => {
      for (let index = 0; index < count; index += 1) {
        const prepared = caster.prepared.find((list) => list.rank === rank);
        slots.push({
          caster_key: caster.caster_key,
          rank,
          slot_index: index,
          used: false,
          prepared_spell: prepared?.spells[index] ?? null,
        });
      }
    });
  }
  return slots;
}

afterEach(cleanup);

test('a slot row renders its spell, cast affordance, and spent state', () => {
  const used = render(SpellSlotRow, {
    props: {
      row: { rank: 1, slot_index: 0, used: true, prepared_spell: 'Fear', pending: false },
    },
  }).container.innerHTML;
  assert.match(used, /Fear/);

  const open = render(SpellSlotRow, {
    props: {
      row: { rank: 1, slot_index: 1, used: false, prepared_spell: null, pending: false },
      editable: true,
      oncast: noop,
      onprepare: noop,
    },
  }).container.innerHTML;
  assert.match(open, /open slot/);
  assert.match(open, /Prepare/);
});

test('CasterPanel renders header pills, rank groups, and heightened cantrip rank', () => {
  const caster = fixture.spellcasters[0];
  const { container } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      known: caster.known,
      oncast: noop,
      onprepare: noop,
      onreset: noop,
    },
  });
  assert.match(container.innerHTML, /Wizard/);
  assert.match(container.innerHTML, /arcane/, 'tradition pill');
  assert.match(container.innerHTML, /atk \+9 · DC 19/, 'adapter numbers on the header pill');
  assert.match(container.innerHTML, /heightened to rank 2/, 'cantrip heightened-rank display');
  assert.match(container.innerHTML, /Fear/, 'prepared slots render');
  assert.match(container.innerHTML, /Reset prep to export/);
});

test('CasterPanel view-only: no cast buttons, no prep affordances', () => {
  const caster = fixture.spellcasters[0];
  const { container } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      editable: false,
      known: caster.known,
    },
  });
  assert.match(container.innerHTML, /Fear/);
  assert.doesNotMatch(container.innerHTML, /Reset prep to export/);
  assert.equal(container.querySelectorAll('button').length, 0, 'view-only renders no controls at all');
});

test('the innate caster renders locked-open without prep affordances', () => {
  const innate = fixture.spellcasters[1];
  const { container } = render(CasterPanel, {
    props: {
      caster: innate,
      slots: fixtureSlots().filter((slot) => slot.caster_key === innate.caster_key),
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      known: innate.known,
      oncast: noop,
    },
  });
  assert.match(container.innerHTML, /Innate spells are always prepared/);
  assert.match(container.innerHTML, /Guidance/);
});

test('StaffPanel renders the charge select, pips, and drain toggle', () => {
  const { container } = render(StaffPanel, {
    props: {
      daily: { value: { staff_charge_rank: 3, staff_spent: 1, drain_used: false }, pending: false },
      editable: true,
      onchange: noop,
    },
  });
  assert.match(container.innerHTML, /Staff charge rank/);
  assert.match(container.innerHTML, /Drain Bonded Item used/);
  assert.match(container.innerHTML, /Staff charges: 2 of 3 left/);
});

test('MagicPane carries the two tabs and the staff section', () => {
  const { container } = render(MagicPane, {
    props: {
      baseSheet: fixture,
      slots: fixtureSlots(),
      view,
      daily: { value: { staff_charge_rank: 0, staff_spent: 0, drain_used: false } },
      oncast: noop,
      onprepare: noop,
      onreset: noop,
      ondaily: noop,
    },
  });
  assert.match(container.innerHTML, /Spells/);
  assert.match(container.innerHTML, /Pet &amp; Minions|Pet & Minions/);
  assert.match(container.innerHTML, /Staff Nexus/);
});

// ---- the production path owns the behaviour ----------

test('focus spells from the export render with their caster', () => {
  const { container } = render(MagicPane, {
    props: {
      baseSheet: fixture,
      slots: fixtureSlots(),
      view,
      daily: { value: { staff_charge_rank: 0, staff_spent: 0, drain_used: false } },
      oncast: noop,
      onprepare: noop,
      onreset: noop,
      ondaily: noop,
    },
  });
  assert.match(container.innerHTML, /Focus spells/, 'the block exists');
  assert.match(container.innerHTML, /Charming Push/, 'the export lists it; it renders, not vanishes');
});

test('a rejected slot write surfaces inline at that row; a rejected daily write at the staff panel (finding 5)', () => {
  render(MagicPane, {
    props: {
      baseSheet: fixture,
      slots: fixtureSlots(),
      view,
      daily: { value: { staff_charge_rank: 2, staff_spent: 1, drain_used: false } },
      opErrors: [
        {
          key: 'slot:7:Wizard:1:0',
          target: { kind: 'slot', character_id: 7, caster_key: 'Wizard', rank: 1, slot_index: 0 },
          op_id: 'op-2',
          outcome: 'forbidden',
          reason: 'Not yours to cast — view-only seat.',
        },
        {
          key: 'vitals:7:daily',
          target: { kind: 'vitals', character_id: 7, field: 'daily' },
          op_id: 'op-3',
          outcome: 'rejected',
          reason: 'The party refused that daily row.',
        },
      ],
      oncast: noop,
      onprepare: noop,
      onreset: noop,
      ondaily: noop,
    },
  });
  // Both surfaces are role=alert (row alert in CasterPanel, staff-panel
  // alert in StaffPanel) — query the role, not the markup.
  const alerts = screen
    .getAllByRole('alert')
    .map((el) => el.textContent)
    .sort();
  assert.deepEqual(alerts, [
    'Not yours to cast — view-only seat.',
    'The party refused that daily row.',
  ]);

  cleanup();
  render(MagicPane, {
    props: {
      baseSheet: fixture,
      slots: fixtureSlots(),
      view,
      daily: { value: { staff_charge_rank: 2, staff_spent: 1, drain_used: false } },
      oncast: noop,
      onprepare: noop,
      onreset: noop,
      ondaily: noop,
    },
  });
  assert.equal(screen.queryByRole('alert'), null, 'no error surface without a refused write');
});

// ---- E9 T8: the custom-spell section in the caster's spellbook ----

const CUSTOM_SPELL = {
  corpus_entry_id: 91,
  kind: 'spell',
  name: 'Conjure Toad Swarm',
  lane: 'custom',
  description: 'So many toads',
  value_or_rank: 3,
  created_by_sub: 'dev-sub-bear',
};

test('a custom spell lists in the spellbook, badged, with a working Prepare', () => {
  /** @type {any[]} */ const prepared = [];
  const caster = fixture.spellcasters[0];
  const { container, getAllByRole } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      known: caster.known,
      editable: true,
      oncast: noop,
      onprepare: (/** @type {any} */ row, /** @type {string} */ spell) => prepared.push({ row, spell }),
      onreset: noop,
      customSpells: [CUSTOM_SPELL],
    },
  });
  assert.match(container.innerHTML, /Custom spells/, 'the section exists when rows do');
  assert.match(container.innerHTML, /Conjure Toad Swarm/, 'the custom spell lists');
  assert.match(container.innerHTML, /custom/, 'badged custom');
  const prepare = /** @type {HTMLButtonElement} */ (
    getAllByRole('button', { name: 'Prepare' }).find((button) =>
      /** @type {HTMLElement} */ (button.parentElement).textContent.includes('Conjure Toad Swarm'),
    )
  );
  assert.ok(prepare, 'the custom row carries a Prepare affordance');
  prepare.click();
  assert.equal(prepared.length, 1);
  assert.equal(prepared[0].spell, 'Conjure Toad Swarm', 'the existing onprepare callback, by name');
  assert.equal(prepared[0].row.rank, 3, 'at its rank');
});

test('without custom spells the section does not render (two fixtures, different results)', () => {
  const caster = fixture.spellcasters[0];
  const { container } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      known: caster.known,
      oncast: noop,
      onprepare: noop,
      onreset: noop,
    },
  });
  assert.doesNotMatch(container.innerHTML, /Custom spells/, 'no rows — no section, no theatre');
});

test('the prepare dialog lists custom spells at their rank', async () => {
  const caster = fixture.spellcasters[0];
  // An unprepared rank-1 slot opens the picker; the custom spell rides at
  // rank 1 so it joins that picker's candidates.
  const slots = fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key);
  const rank1Index = slots.findIndex((slot) => slot.rank === 1);
  slots[rank1Index] = { ...slots[rank1Index], prepared_spell: null };
  const { getByRole, findByRole } = render(CasterPanel, {
    props: {
      caster,
      slots,
      numbers: view.derived,
      cantripRank: view.render_base.cantrip_rank,
      known: caster.known,
      editable: true,
      oncast: noop,
      onprepare: noop,
      onreset: noop,
      customSpells: [{ ...CUSTOM_SPELL, value_or_rank: 1 }],
    },
  });
  fireEvent.click(getByRole('button', { name: 'Prepare…' }));
  const dialog = /** @type {HTMLElement} */ (await findByRole('dialog'));
  assert.match(
    dialog.textContent,
    /Conjure Toad Swarm/,
    'the custom spell is pickable where known spells are',
  );
  const pick = /** @type {HTMLButtonElement} */ (
    [...dialog.querySelectorAll('button')].find((button) => button.textContent?.includes('Conjure Toad Swarm'))
  );
  fireEvent.click(pick);
  assert.throws(() => getByRole('dialog'), 'the dialog closes on commit');
});
