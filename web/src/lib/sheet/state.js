// Sheet state layer (E6 design §2) — the ONLY module that talks to
// `createSync`. Components get stores; they never see the socket, the
// queue, or a wire target.
//
// Read path: the sync store's fields (strictly-newer merged, optimistic
// echo applied) fall back to the bootstrap vitals for anything the socket
// has not delivered yet — the sheet renders instantly on cold boot, never
// a spinner over nothing (design §7).
//
// Write path: component → client-side bounds validation (invalid input
// never leaves this layer; the server re-validates anyway) → `sync.write`
// → optimistic echo tagged `pending` → ack settles it. `superseded`
// reverts silently (the store's rule); `rejected`/`forbidden` surface on
// `opErrors` for inline display at the control and auto-clear on the
// field's next applied ack (spec §6).
//
// New Day (Q1 ruling): one FIFO burst — every slot row → used=false,
// focus → 0, daily → zeroed. CAS per field, no cross-field transaction;
// a partial replay converges because every write is idempotent and
// absolutely-valued (design §3).

import { writable, derived, get } from 'svelte/store';

import { derive } from '../engine/index.js';
import { targetKey } from '../sync/store.js';

/** @param {number} characterId @param {string} field */
const vitalsTarget = (characterId, field) => ({
  kind: 'vitals',
  character_id: characterId,
  field,
});

/** @param {number} characterId @param {{caster_key: string, rank: number, slot_index: number}} row */
const slotTarget = (characterId, { caster_key, rank, slot_index }) => ({
  kind: 'slot',
  character_id: characterId,
  caster_key,
  rank,
  slot_index,
});

/** @param {number} characterId @param {string} itemName */
const invTarget = (characterId, itemName) => ({
  kind: 'inv',
  character_id: characterId,
  item_name: itemName,
});

/** @param {number} value @param {number} min @param {number} max */
const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

/** The `/api/characters/me` payload this layer consumes.
 *
 * @typedef {object} Bootstrap
 * @property {{ id: number, name: string | null }} character
 * @property {*} base_sheet the normalized sheet (the Rust transform's shape)
 * @property {{ hp: number, temp_hp: number, money_pp: number, money_gp: number,
 *   money_sp: number, money_cp: number, level_adjust: number,
 *   focus_current: number, hero_points: number,
 *   daily: { staff_charge_rank: number, staff_spent: number, drain_used: boolean } }} vitals
 * @property {Array<{ caster_key: string, rank: number, slot_index: number,
 *   used: boolean, prepared_spell: string | null }>} slots
 * @property {Array<{ name: string, qty_delta: number }>} inventory
 * @property {Record<string, number | null>} [item_bulk]
 */

/** One field entry in the sync store's merged state.
 *
 * @typedef {{ target: Record<string, *>, value: *, version: number }} FieldEntry
 */

/** One rejected/forbidden op awaiting inline display.
 *
 * @typedef {{ key: string | null, target: Record<string, *> | null,
 *   op_id: string, outcome: string, reason: string }} OpError
 */

/**
 * The sheet state handle — svelte stores plus the write surface. Exported
 * as a typedef so components can annotate the handle explicitly (svelte2tsx
 * will not infer store-ness through a call's return type).
 *
 * @typedef {object} SheetState
 * @property {import('svelte/store').Readable<*>} view
 * @property {import('svelte/store').Readable<number>} hpMax
 * @property {import('svelte/store').Readable<{value: number, pending: boolean}>} hp
 * @property {import('svelte/store').Readable<{value: *, pending: boolean}>} tempHp
 * @property {import('svelte/store').Readable<{value: *, pending: boolean}>} money
 * @property {import('svelte/store').Readable<{value: *, pending: boolean}>} focusCurrent
 * @property {import('svelte/store').Readable<number>} focusMax
 * @property {import('svelte/store').Readable<{value: *, pending: boolean}>} heroPoints
 * @property {import('svelte/store').Readable<number>} heroMax
 * @property {import('svelte/store').Readable<{value: *, pending: boolean}>} daily
 * @property {import('svelte/store').Readable<Array<object>>} slots
 * @property {import('svelte/store').Readable<boolean>} syncing
 * @property {import('svelte/store').Readable<boolean>} offline
 * @property {import('svelte/store').Readable<OpError[]>} opErrors
 * @property {(itemName: string) => import('svelte/store').Readable<{qty: number, pending: boolean}>} itemQty
 * @property {() => void} connect
 * @property {() => void} disconnect
 * @property {() => Array<object>} queue
 * @property {(value: number) => void} writeHp
 * @property {(value: number) => void} writeTempHp
 * @property {(value: {pp: number, gp: number, sp: number, cp: number}) => void} writeMoney
 * @property {(value: number) => void} writeLevelAdjust
 * @property {(value: number) => void} writeFocus
 * @property {(value: number) => void} writeHeroPoints
 * @property {(value: {staff_charge_rank: number, staff_spent: number, drain_used: boolean}) => void} writeDaily
 * @property {(casterKey: string, rank: number, index: number, patch?: {used?: boolean, prepared?: string | null}) => void} writeSlot
 * @property {(itemName: string, quantity: number) => void} writeItemQty
 * @property {() => void} newDay
 * @property {() => void} destroy
 */

/**
 * @param {{
 *   sync: import('../sync/index.js').Sync,
 *   character: Bootstrap,
 * }} setup
 * @returns {SheetState}
 */
export function createSheetState({ sync, character }) {
  const characterId = character.character.id;
  const baseSheet = character.base_sheet;

  // ---- affordance stores -------------------------------------------------
  const syncing = writable(sync.isSyncing());
  const connectionState = writable(sync.connectionState());
  const offline = derived(connectionState, ($state) => $state === 'offline');
  /** Rejected/forbidden ops awaiting inline display, keyed by field. */
  const opErrors = writable(/** @type {OpError[]} */ ([]));

  // ---- the live field map + engine view ----------------------------------
  const fields = writable(/** @type {Record<string, FieldEntry>} */ (sync.state()));
  const pendingKeys = writable(/** @type {Set<string>} */ (new Set()));

  /** The engine view: base sheet + live level + zero effects. */
  const view = derived([fields], ([$fields]) => {
    const adjustField = $fields[targetKey(vitalsTarget(characterId, 'level_adjust'))];
    // Any live entry — a pending echo or server truth — outranks the
    // bootstrap vitals; nothing recorded yet means "render the export".
    const adjust = adjustField ? adjustField.value : character.vitals.level_adjust;
    return derive({ id: characterId, base_sheet: baseSheet }, { level_adjust: adjust, effects: [] });
  });

  /**
   * One vitals field as `{value, pending}`: live value when the socket has
   * delivered it (or an echo is pending), bootstrap vitals otherwise.
   * @param {string} field @param {*} fallback
   */
  function vitalsStore(field, fallback) {
    const target = vitalsTarget(characterId, field);
    return derived([fields, pendingKeys], ([$fields, $pending]) => {
      const entry = $fields[targetKey(target)];
      const pending = $pending.has(targetKey(target));
      if (pending || (entry && entry.version > 0)) {
        return { value: entry ? entry.value : fallback, pending };
      }
      return { value: fallback, pending: false };
    });
  }

  const hpMax = derived(view, ($view) => $view.hp_max.total);
  const hp = derived([vitalsStore('hp', character.vitals.hp), hpMax], ([$hp]) => ({
    value: clamp($hp.value, 0, Number.MAX_SAFE_INTEGER),
    pending: $hp.pending,
  }));
  const tempHp = vitalsStore('temp_hp', character.vitals.temp_hp);
  const money = vitalsStore('money', {
    pp: character.vitals.money_pp,
    gp: character.vitals.money_gp,
    sp: character.vitals.money_sp,
    cp: character.vitals.money_cp,
  });
  const focusCurrent = vitalsStore('focus_current', character.vitals.focus_current);
  const heroPoints = vitalsStore('hero_points', character.vitals.hero_points);
  const daily = vitalsStore('daily', character.vitals.daily);

  /** The focus pip ceiling: the character's focus max (adapter). */
  const focusMax = derived(view, ($view) => $view.focus_max);
  const heroMax = derived(view, ($view) => $view.hero_max);

  // ---- slots ---------------------------------------------------------------
  /** The slot layout with live used/prepared/pending per row. */
  const slots = derived([fields, pendingKeys], ([$fields, $pending]) =>
    character.slots.map((row) => {
      const target = slotTarget(characterId, row);
      const key = targetKey(target);
      const entry = $fields[key];
      const pending = $pending.has(key);
      const value = pending || (entry && entry.version > 0) ? entry.value : row;
      return {
        ...row,
        used: value ? value.used : row.used,
        prepared_spell: value ? (value.prepared ?? row.prepared_spell) : row.prepared_spell,
        pending,
      };
    }),
  );

  /** Effective quantity per item name: base + delta, delta from live state. */
  const baseQty = new Map();
  for (const item of baseSheet.equipment ?? []) {
    if (!baseQty.has(item.name)) baseQty.set(item.name, item.qty);
  }
  const storedDelta = new Map(
    (character.inventory ?? []).map((item) => [item.name, item.qty_delta]),
  );
  const itemQtyStores = new Map();

  /**
   * @param {string} itemName
   * @returns {import('svelte/store').Readable<{qty: number, pending: boolean}>}
   */
  function itemQty(itemName) {
    let store = itemQtyStores.get(itemName);
    if (!store) {
      const target = invTarget(characterId, itemName);
      store = derived([fields, pendingKeys], ([$fields, $pending]) => {
        const key = targetKey(target);
        const entry = $fields[key];
        const pending = $pending.has(key);
        const delta =
          pending || (entry && entry.version > 0)
            ? (entry ? entry.value.qty_delta : 0)
            : (storedDelta.get(itemName) ?? 0);
        return { qty: Math.max(0, (baseQty.get(itemName) ?? 0) + delta), pending };
      });
      itemQtyStores.set(itemName, store);
    }
    return store;
  }

  // ---- sync event wiring ---------------------------------------------------
  /** @param {Record<string, *>} event */
  function handleSyncEvent(event) {
    if (event.type === 'queue') {
      syncing.set(sync.isSyncing());
      const pending = new Set(sync.queue().map((op) => targetKey(op.target)));
      pendingKeys.set(pending);
      fields.set(sync.state());
    } else if (event.type === 'connection') {
      connectionState.set(event.state);
    } else if (event.type === 'fields') {
      fields.set(sync.state());
    } else if (event.type === 'applied' && event.key) {
      // A win on a field clears its inline error (auto-clear on next ack).
      opErrors.update((errors) => errors.filter((error) => error.key !== event.key));
    } else if (event.type === 'op_exposed') {
      const key = event.op ? targetKey(event.op.target) : null;
      opErrors.update((errors) => [
        ...errors.filter((error) => error.key !== key),
        {
          key,
          target: event.op ? event.op.target : null,
          op_id: /** @type {string} */ (event.op_id),
          outcome: /** @type {string} */ (event.outcome),
          reason: event.reason ?? '',
        },
      ]);
    }
  }
  const unsubscribe = sync.subscribe(handleSyncEvent);

  // ---- write surface (client-side bounds live here) ------------------------
  /** @param {Record<string, *>} target @param {*} value */
  function write(target, value) {
    // A retry on a field clears its error immediately — the user acted.
    const key = targetKey(target);
    opErrors.update((errors) => errors.filter((error) => error.key !== key));
    sync.write(target, value);
  }

  /** @param {number} value */
  function writeHp(value) {
    const max = get(hpMax);
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    write(vitalsTarget(characterId, 'hp'), clamp(Math.round(parsed), 0, max));
  }

  /** @param {number} value */
  function writeTempHp(value) {
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    write(vitalsTarget(characterId, 'temp_hp'), Math.max(0, Math.round(parsed)));
  }

  /** @param {{pp: number, gp: number, sp: number, cp: number}} value */
  function writeMoney(value) {
    /** @param {*} amount */
    const whole = (amount) => Math.max(0, Math.round(Number(amount) || 0));
    write(vitalsTarget(characterId, 'money'), {
      pp: whole(value.pp),
      gp: whole(value.gp),
      sp: whole(value.sp),
      cp: whole(value.cp),
    });
  }

  /** @param {number} value the desired effective level 1..20 */
  function writeLevelAdjust(value) {
    const exportLevel = baseSheet.identity.level;
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    const level = clamp(Math.round(parsed), 1, 20);
    write(vitalsTarget(characterId, 'level_adjust'), level - exportLevel);
  }

  /** @param {number} value */
  function writeFocus(value) {
    const max = get(focusMax);
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    write(vitalsTarget(characterId, 'focus_current'), clamp(Math.round(parsed), 0, max));
  }

  /** @param {number} value */
  function writeHeroPoints(value) {
    const max = get(heroMax);
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    write(vitalsTarget(characterId, 'hero_points'), clamp(Math.round(parsed), 0, max));
  }

  /** @param {{staff_charge_rank: number, staff_spent: number, drain_used: boolean}} value */
  function writeDaily(value) {
    write(vitalsTarget(characterId, 'daily'), {
      staff_charge_rank: clamp(Math.round(Number(value.staff_charge_rank) || 0), 0, 10),
      staff_spent: Math.max(0, Math.round(Number(value.staff_spent) || 0)),
      drain_used: Boolean(value.drain_used),
    });
  }

  /**
   * Whole-slot write (unmentioned fields reset, so `prepared` always rides).
   * @param {string} casterKey @param {number} rank @param {number} index
   * @param {{used?: boolean, prepared?: string | null}} patch
   */
  function writeSlot(casterKey, rank, index, { used, prepared } = {}) {
    const row = character.slots.find(
      (candidate) =>
        candidate.caster_key === casterKey &&
        candidate.rank === rank &&
        candidate.slot_index === index,
    );
    const current = get(slots).find(
      (candidate) =>
        candidate.caster_key === casterKey &&
        candidate.rank === rank &&
        candidate.slot_index === index,
    );
    write(slotTarget(characterId, { caster_key: casterKey, rank, slot_index: index }), {
      used: Boolean(used ?? current?.used ?? false),
      prepared:
        prepared !== undefined
          ? prepared
          : (current?.prepared_spell ?? row?.prepared_spell ?? null),
    });
  }

  /**
   * Set an item's effective quantity (qty 0 = removed, design §11.6); the
   * wire value is the signed delta against the base sheet.
   * @param {string} itemName @param {number} quantity
   */
  function writeItemQty(itemName, quantity) {
    const parsed = Number(quantity);
    if (Number.isNaN(parsed)) return;
    const effective = Math.max(0, Math.round(parsed));
    const delta = effective - (baseQty.get(itemName) ?? 0);
    write(invTarget(characterId, itemName), { qty_delta: delta });
  }

  /** The New Day burst: every slot's used flag, then focus, then daily. */
  function newDay() {
    for (const row of get(slots)) {
      if (row.used || row.pending) {
        writeSlot(row.caster_key, row.rank, row.slot_index, { used: false });
      }
    }
    write(vitalsTarget(characterId, 'focus_current'), 0);
    write(vitalsTarget(characterId, 'daily'), {
      staff_charge_rank: 0,
      staff_spent: 0,
      drain_used: false,
    });
  }

  return {
    view,
    hpMax,
    hp,
    tempHp,
    money,
    focusCurrent,
    focusMax,
    heroPoints,
    heroMax,
    daily,
    slots,
    syncing,
    offline,
    opErrors,

    itemQty,
    connect: () => sync.connect(),
    disconnect: () => sync.disconnect(),
    queue: () => sync.queue(),
    writeHp,
    writeTempHp,
    writeMoney,
    writeLevelAdjust,
    writeFocus,
    writeHeroPoints,
    writeDaily,
    writeSlot,
    writeItemQty,
    newDay,
    destroy: () => unsubscribe(),
  };
}
