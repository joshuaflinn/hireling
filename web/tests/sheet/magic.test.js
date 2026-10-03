/* global URL */
import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';

import CasterPanel from '../../src/lib/sheet/components/CasterPanel.svelte';
import MagicPane from '../../src/lib/sheet/components/MagicPane.svelte';
import StaffPanel from '../../src/lib/sheet/components/StaffPanel.svelte';
import SpellSlotRow from '../../src/lib/sheet/components/SpellSlotRow.svelte';
import { derive } from '../../src/lib/engine/index.js';

const FIXTURE_PATH = fileURLToPath(new URL('../data/base_sheet_reference.json', import.meta.url));
const fixture = JSON.parse(await readFile(FIXTURE_PATH, 'utf8'));
const view = derive({ id: 7, base_sheet: fixture }, { level_adjust: 0, effects: [] });

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

test('a slot row renders its spell, cast affordance, and spent state', () => {
  const used = render(SpellSlotRow, {
    props: {
      row: { rank: 1, slot_index: 0, used: true, prepared_spell: 'Fear', pending: false },
    },
  }).body;
  assert.match(used, /Fear/);

  const open = render(SpellSlotRow, {
    props: {
      row: { rank: 1, slot_index: 1, used: false, prepared_spell: null, pending: false },
      editable: true,
      oncast: noop,
      onprepare: noop,
    },
  }).body;
  assert.match(open, /open slot/);
  assert.match(open, /Prepare/);
});

test('CasterPanel renders header pills, rank groups, and heightened cantrip rank', () => {
  const caster = fixture.spellcasters[0];
  const { body } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.cantrip_rank,
      known: caster.known,
      oncast: noop,
      onprepare: noop,
      onreset: noop,
    },
  });
  assert.match(body, /Wizard/);
  assert.match(body, /arcane/, 'tradition pill');
  assert.match(body, /atk \+9 · DC 19/, 'adapter numbers on the header pill');
  assert.match(body, /heightened to rank 2/, 'cantrip heightened-rank display');
  assert.match(body, /Fear/, 'prepared slots render');
  assert.match(body, /Reset prep to export/);
});

test('CasterPanel view-only: no cast buttons, no prep affordances', () => {
  const caster = fixture.spellcasters[0];
  const { body } = render(CasterPanel, {
    props: {
      caster,
      slots: fixtureSlots().filter((slot) => slot.caster_key === caster.caster_key),
      numbers: view.derived,
      cantripRank: view.cantrip_rank,
      editable: false,
      known: caster.known,
    },
  });
  assert.match(body, /Fear/);
  assert.doesNotMatch(body, /Reset prep to export/);
  const buttons = body.match(/<button/g) ?? [];
  assert.equal(buttons.length, 0, 'view-only renders no controls at all');
});

test('the innate caster renders locked-open without prep affordances', () => {
  const innate = fixture.spellcasters[1];
  const { body } = render(CasterPanel, {
    props: {
      caster: innate,
      slots: fixtureSlots().filter((slot) => slot.caster_key === innate.caster_key),
      numbers: view.derived,
      cantripRank: view.cantrip_rank,
      known: innate.known,
      oncast: noop,
    },
  });
  assert.match(body, /Innate spells are always prepared/);
  assert.match(body, /Guidance/);
});

test('StaffPanel renders the charge select, pips, and drain toggle', () => {
  const { body } = render(StaffPanel, {
    props: {
      daily: { value: { staff_charge_rank: 3, staff_spent: 1, drain_used: false }, pending: false },
      editable: true,
      onchange: noop,
    },
  });
  assert.match(body, /Staff charge rank/);
  assert.match(body, /Drain Bonded Item used/);
  assert.match(body, /Staff charges: 2 of 3 left/);
});

test('MagicPane carries the two tabs and the staff section', () => {
  const { body } = render(MagicPane, {
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
  assert.match(body, /Spells/);
  assert.match(body, /Pet &amp; Minions|Pet & Minions/);
  assert.match(body, /Staff Nexus/);
});

// ---- MOR-48 review fixes: the production path owns the behaviour ----------

test('focus spells from the export render with their caster (finding 11)', () => {
  const { body } = render(MagicPane, {
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
  assert.match(body, /Focus spells/, 'the block exists');
  assert.match(body, /Charming Push/, 'the export lists it; it renders, not vanishes');
});

test('a rejected slot write surfaces inline at that row; a rejected daily write at the staff panel (finding 5)', () => {
  const { body } = render(MagicPane, {
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
  assert.match(body, /Not yours to cast — view-only seat\./);
  assert.match(body, /The party refused that daily row\./);
});
