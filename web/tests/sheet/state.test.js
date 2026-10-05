import { test } from 'vitest';
import assert from 'node:assert/strict';

import { get } from 'svelte/store';

import { createSync } from '../../src/lib/sync/index.js';
import { fakeClock, mockSockets, fakeStorage } from '../sync/fakes.js';
import { createSheetState } from '../../src/lib/sheet/state.js';

import fixture from '../data/base_sheet_reference.json';
/** The reference character's EngineOutput, verbatim as the wire carries it
 * (generated from the extractor + compute over the same export — the swap's
 * byte-identity fixture). */
import engineOutput from '../data/engine_output_reference.json';

const CHARACTER_ID = 7;

/** The bootstrap payload shape /api/characters/me returns. */
function bootstrap() {
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
  return {
    character: { id: CHARACTER_ID, name: fixture.identity.name },
    base_sheet: fixture,
    vitals: {
      hp: 20,
      temp_hp: 0,
      money_pp: 0,
      money_gp: 24,
      money_sp: 2,
      money_cp: 4,
      level_adjust: 0,
      focus_current: 0,
      hero_points: 1,
      daily: { staff_charge_rank: 3, staff_spent: 0, drain_used: false },
    },
    slots,
    inventory: [{ name: 'Chalk', qty_delta: -2 }],
    item_bulk: { Chalk: null },
  };
}

/** A sync instance on mocks, plus the sheet state over it. */
function setup() {
  const clock = fakeClock();
  const mocks = mockSockets();
  let opSeq = 0;
  const sync = createSync({
    url: 'ws://test/party/1',
    storage: fakeStorage(),
    accountSub: 'sub-a',
    socketFactory: mocks.factory,
    rng: () => 1,
    now: clock.now,
    timers: clock.timers,
    idFactory: () => `op-${++opSeq}`,
  });
  const state = createSheetState({ sync, character: bootstrap() });
  return { clock, mocks, sync, state };
}

/** Open the socket and merge a snapshot the way the server sends it —
 * versioned fields plus the roster's derived array (design D6). */
function handshake(socket, fields = [], derived = [engineOutput]) {
  socket.open();
  socket.message(
    JSON.stringify({
      t: 'snapshot',
      fields: fields.map(({ target, value, version }) => ({
        field: target,
        value,
        version,
      })),
      derived,
      snapshot_bytes: 42,
    }),
  );
}

const hp = () => ({ kind: 'vitals', character_id: CHARACTER_ID, field: 'hp' });
const focus = () => ({ kind: 'vitals', character_id: CHARACTER_ID, field: 'focus_current' });
const daily = () => ({ kind: 'vitals', character_id: CHARACTER_ID, field: 'daily' });
const slot = (rank, index, key = 'Wizard') => ({
  kind: 'slot',
  character_id: CHARACTER_ID,
  caster_key: key,
  rank,
  slot_index: index,
});

test('before the wire: view is null, vitals render, no invented numbers', () => {
  const { state } = setup();
  assert.equal(get(state.view), null, 'derived is never stored — null until the wire');
  assert.equal(get(state.hpMax), null, 'the ceiling waits for render_base');
  assert.equal(get(state.hp).value, 20, 'bootstrap vitals render before the socket');
  assert.equal(get(state.heroPoints).value, 1);
  assert.deepEqual(get(state.daily).value, {
    staff_charge_rank: 3,
    staff_spent: 0,
    drain_used: false,
  });
  assert.equal(get(state.slots).length, 14, '6+4+3 wizard slots + 1 innate cantrip');
  assert.equal(get(state.money).value.gp, 24);
});

test('a write whose clamp needs the max no-ops until the engine output arrives', () => {
  const { mocks, state } = setup();
  state.writeHp(30);
  assert.deepEqual(state.queue(), [], 'no hpMax on the wire yet — nothing rides');
  assert.equal(get(state.hp).value, 20, 'no optimistic echo was invented');

  state.connect();
  handshake(mocks.sockets[0]); // derived arrives: hpMax 32
  state.writeHp(30);
  assert.equal(get(state.hp).value, 30, 'the echo paints once the ceiling exists');
  assert.equal(get(state.hp).pending, true, 'the echo is tagged pending');
});

test('the snapshot delivers the view: render_base and derived, wire-verbatim', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], []);
  const view = get(state.view);
  assert.equal(view.schema, 'hireling.engine.output.v1', 'the contract shape, verbatim');
  assert.equal(view.render_base.level, 3);
  assert.equal(view.render_base.hp_max, 32);
  assert.equal(view.derived.ac.total, 16);
  assert.equal(get(state.hpMax), 32);
  assert.equal(get(state.heroMax), 3);
  assert.equal(get(state.focusMax), 1);
});

test('a write echoes pending, then an applied ack settles it', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0]);
  state.writeHp(30);
  assert.equal(get(state.hp).value, 30, 'the echo paints immediately');
  assert.equal(get(state.hp).pending, true, 'the echo is tagged pending');

  const sent = mocks.sockets[0].sent.map((raw) => JSON.parse(raw));
  const write = sent.find((frame) => frame.t === 'write' && frame.target.field === 'hp');
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: write.op_id, outcome: 'applied', version: 6 }),
  );
  assert.equal(get(state.hp).value, 30);
  assert.equal(get(state.hp).pending, false, 'the ack settles the echo');
});

test('superseded reverts silently — server truth, no error surfaced', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], [{ target: hp(), value: 25, version: 5 }]);
  assert.equal(get(state.hp).value, 25, 'server truth renders');

  state.writeHp(30);
  const sent = mocks.sockets[0].sent.map((raw) => JSON.parse(raw));
  const write = sent.find((frame) => frame.t === 'write');
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: write.op_id, outcome: 'superseded', winning_version: 9 }),
  );
  assert.equal(get(state.hp).value, 25, 'reverted to server truth');
  assert.deepEqual(get(state.opErrors), [], 'nothing is ever shown for superseded');
});

test('rejected surfaces the op record inline; the next applied ack clears it', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0]);

  state.writeHp(30);
  let write = mocks.sockets[0].sent.map((r) => JSON.parse(r)).find((f) => f.t === 'write');
  mocks.sockets[0].message(
    JSON.stringify({
      t: 'ack',
      op_id: write.op_id,
      outcome: 'rejected',
      reason: 'hp must be an integer',
    }),
  );
  assert.deepEqual(
    get(state.opErrors).map((error) => ({
      key: error.key,
      outcome: error.outcome,
      reason: error.reason,
    })),
    [{ key: '{"character_id":7,"field":"hp","kind":"vitals"}', outcome: 'rejected', reason: 'hp must be an integer' }],
  );

  state.writeHp(31);
  write = [...mocks.sockets[0].sent]
    .reverse()
    .map((r) => JSON.parse(r))
    .find((f) => f.t === 'write');
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: write.op_id, outcome: 'applied', version: 6 }),
  );
  assert.deepEqual(get(state.opErrors), [], 'the win on the field clears the error');
});

test('offline flips the affordance store; reads stay live', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], [{ target: hp(), value: 25, version: 5 }]);
  assert.equal(get(state.offline), false);

  state.disconnect(); // the page's browser-offline path
  assert.equal(get(state.offline), true, 'the affordance store flips');
  assert.equal(get(state.hp).value, 25, 'reads stay');
  state.writeHp(30); // queued, not lost
  assert.deepEqual(state.queue().length, 1);
});

test('client-side bounds: invalid input never leaves the component layer', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0]); // ceilings arrive: hp 32, focus 1, hero 3
  state.writeHp(-5); // clamped to 0
  state.writeHp(9999); // clamped to max 32
  state.writeTempHp(-1); // clamped to 0
  state.writeLevelAdjust(50); // clamped to the 1..20 window
  state.writeFocus(99); // clamped to focus_max (1 on the fixture)
  state.writeHeroPoints(7); // clamped to hero_max (3)
  const queued = state.queue();
  assert.deepEqual(
    queued.map((op) => op.value),
    [0, 32, 0, 17, 1, 3],
    'clamped values are what rides the queue',
  );
});

test('slot writes carry the whole-slot value with current prepared spell', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], []);
  state.writeSlot('Wizard', 1, 0, { used: true });
  const write = mocks.sockets[0].sent.map((r) => JSON.parse(r)).find((f) => f.t === 'write');
  assert.deepEqual(write.target, slot(1, 0));
  assert.deepEqual(write.value, { used: true, prepared: '500 Toads' }, 'prepared rides along');
});

test('qty writes are deltas against base, clamped at zero', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], []);
  // Chalk: 10 in base, −2 stored delta → effective 8.
  assert.equal(get(state.itemQty('Chalk')).qty, 8);
  state.writeItemQty('Chalk', 5); // → delta −5
  state.writeItemQty('Chalk', 0); // → delta −10 (qty 0 = removed)
  state.writeItemQty('Bedroll', 3); // → delta +2
  const writes = mocks.sockets[0].sent
    .map((r) => JSON.parse(r))
    .filter((f) => f.t === 'write');
  assert.deepEqual(
    writes.map((write) => [write.target.item_name, write.value.qty_delta]),
    [
      ['Chalk', -5],
      ['Chalk', -10],
      ['Bedroll', 2],
    ],
  );
});

test('New Day enqueues the whole reset burst FIFO: slots, focus, daily', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], [
    { target: slot(1, 0), value: { used: true, prepared: '500 Toads' }, version: 2 },
    { target: focus(), value: 1, version: 3 },
    {
      target: daily(),
      value: { staff_charge_rank: 3, staff_spent: 1, drain_used: true },
      version: 4,
    },
  ]);

  state.newDay();
  const queued = state.queue();
  const targets = queued.map((op) => op.target);
  assert.deepEqual(targets[0], slot(1, 0), 'the used slot goes first');
  assert.deepEqual(queued[0].value, { used: false, prepared: '500 Toads' });
  assert.deepEqual(targets.at(-2), focus(), 'focus second-to-last');
  // Refill, not empty (review finding 2): the pool regains its points —
  // the fixture character's focus max is 1.
  assert.equal(queued.at(-2).value, 1, 'focus refills to the character max');
  assert.deepEqual(targets.at(-1), daily(), 'daily last');
  assert.deepEqual(queued.at(-1).value, {
    staff_charge_rank: 0,
    staff_spent: 0,
    drain_used: false,
  });
  const ids = new Set(queued.map((op) => op.op_id));
  assert.equal(ids.size, queued.length, 'distinct ops');
});

test('New Day before the wire is a no-op — no silent partial burst (review F3)', () => {
  const { mocks, state } = setup();
  assert.equal(get(state.view), null, 'cold boot: no engine output yet');
  state.newDay();
  assert.deepEqual(
    state.queue(),
    [],
    'slots and daily do not fire while the focus refill has no source — two of three promises in one silent stroke is the bug, not the fix',
  );

  // Once the wire speaks, the burst rides in full.
  state.connect();
  handshake(mocks.sockets[0], [
    { target: slot(1, 0), value: { used: true, prepared: '500 Toads' }, version: 2 },
  ]);
  state.newDay();
  const queued = state.queue();
  assert.ok(queued.length >= 3, 'slot + focus + daily all present');
  assert.deepEqual(queued.map((op) => op.target.kind), ['slot', 'vitals', 'vitals']);
  assert.equal(queued[1].target.field, 'focus_current');
  assert.equal(queued[2].target.field, 'daily');
});

test('the syncing store reads the queue, nothing else', () => {
  const { mocks, state } = setup();
  assert.equal(get(state.syncing), false, 'empty queue + dead link = idle');
  state.connect();
  handshake(mocks.sockets[0]);
  state.writeHp(30);
  assert.equal(get(state.syncing), true, 'queued write = syncing');
  const write = mocks.sockets[0].sent.map((r) => JSON.parse(r)).find((f) => f.t === 'write');
  mocks.sockets[0].message(
    JSON.stringify({ t: 'ack', op_id: write.op_id, outcome: 'applied', version: 6 }),
  );
  assert.equal(get(state.syncing), false, 'settled = idle');
});

test("the client never re-derives: a level_adjust write moves nothing until the wire's derived frame lands", () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0]);
  assert.equal(get(state.view).render_base.hp_max, 32);

  state.writeLevelAdjust(4); // desired level 4 → adjust +1
  assert.equal(get(state.view).render_base.hp_max, 32, 'no client math — the view waits');
  assert.equal(get(state.hpMax), 32);

  // The server's recompute comes back as a `derived` frame (the fan-out
  // the write triggers, D4/D6) — THAT is what moves the view.
  const recomputed = {
    ...engineOutput,
    render_base: { ...engineOutput.render_base, level: 4, hp_max: 40 },
    derived: {
      ...engineOutput.derived,
      fort: { ...engineOutput.derived.fort, base: 8, total: 8 },
    },
  };
  mocks.sockets[0].message(JSON.stringify({ t: 'derived', character_id: CHARACTER_ID, output: recomputed }));
  assert.equal(get(state.view).render_base.hp_max, 40, 'the wire moves the view');
  assert.equal(get(state.hpMax), 40);
});

// ---- MOR-48 review fixes: the production path owns the behaviour ----------

test('the hp readout clamps to the live max — a level-down never shows 32 / 16 (finding 11)', () => {
  const { mocks, state } = setup();
  state.connect();
  handshake(mocks.sockets[0], [{ target: hp(), value: 32, version: 2 }]);
  assert.equal(get(state.hp).value, 32, 'full HP at level 3 (max 32)');

  // The −2 adjust lands as server truth, then the recomputed engine output
  // arrives (hp_max 16 at eff_level 1): the readout must clamp to it —
  // the write clamp alone left "32 / 16".
  const derated = {
    ...engineOutput,
    render_base: { ...engineOutput.render_base, level: 1, hp_max: 16 },
  };
  handshake(mocks.sockets[0], [
    {
      target: { kind: 'vitals', character_id: CHARACTER_ID, field: 'level_adjust' },
      value: -2,
      version: 3,
    },
  ], [derated]);
  assert.equal(get(state.hpMax), 16, 'max re-derives — on the server, by the wire');
  assert.equal(get(state.hp).value, 16, 'the readout clamps to the live max');
});
