// Sheet state layer (E6 design §2) — the ONLY module that talks to
// `createSync`. Components get stores; they never see the socket, the
// queue, or a wire target.
//
// Read path: the engine's derived output (forwarded verbatim off the
// socket, never stored durably — design D6) drives the number panes.
// Until the first snapshot arrives they render skeletons (design §7's
// loading state — an honest gap, no client math); bootstrap vitals (hp,
// money, slots, daily) stay live and writable from first paint.
//
// Write path: component → client-side bounds validation (invalid input
// never leaves this layer; the server re-validates anyway) → `sync.write`
// → optimistic echo tagged `pending` → ack settles it. `superseded`
// reverts silently (the store's rule); `rejected`/`forbidden` surface on
// `opErrors` for inline display at the control and auto-clear on the
// field's next applied ack (spec §6).
//
// New Day (Q1 ruling): one FIFO burst — every slot row → used=false,
// focus → the character's focus max (the pool refills), daily → zeroed.
// CAS per field, no cross-field transaction; a partial replay converges
// because every write is idempotent and absolutely-valued (design §3).

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

/** Effect CREATE (E8's frame): the row does not exist yet; the target is
 *  the party it lands in and the server mints the id (specs/010 FR-C4). */
/** @param {number} partyId */
const effectNewTarget = (partyId) => ({ kind: 'effect_new', party_id: partyId });

/** Effect retarget/end: whole-row CAS on the effect's version (FR-14). */
/** @param {number} effectId */
const effectTarget = (effectId) => ({ kind: 'effect', effect_id: effectId });

/** The closed modifier types — `engine/src/vocab.rs` MODIFIER_TYPES; the
 *  server re-validates and its rejection surfaces (spec FR-C3). */
const MODIFIER_TYPES = ['circumstance', 'status', 'item', 'untyped'];

/**
 * Client-side bounds for a freeform create — the server's table
 * (`src/sync/write.rs` `validate_effect_value(.., "create")`) mirrored so
 * invalid input never leaves the layer. Returns the wire-shaped value, or
 * null (bounds are the caller's no-op).
 *
 * @param {{name: string, sourceCharacterId: number, targets: number[], modifiers: Array<{type: string, stat: string, value: number}>, durationNote: string}} form
 * @returns {{op: string, name: string, source_character_id: number, targets: number[], modifiers: Array<{type: string, stat: string, value: number}>, duration_note: string} | null}
 */
export function effectCreateValue(form) {
  const name = String(form.name ?? '').trim();
  if (name.length < 1 || name.length > 120) return null;
  const targets = [...new Set(form.targets)].filter((id) => Number.isInteger(id) && id > 0);
  if (targets.length < 1) return null;
  if (!Array.isArray(form.modifiers) || form.modifiers.length < 1 || form.modifiers.length > 16) {
    return null;
  }
  const modifiers = [];
  for (const row of form.modifiers) {
    if (!MODIFIER_TYPES.includes(row.type)) return null;
    if (typeof row.stat !== 'string' || row.stat === '') return null;
    const value = Number(row.value);
    if (!Number.isInteger(value) || value < -50 || value > 50) return null;
    modifiers.push({ type: row.type, stat: row.stat, value });
  }
  return {
    op: 'create',
    name,
    source_character_id: form.sourceCharacterId,
    targets,
    modifiers,
    duration_note: String(form.durationNote ?? ''),
  };
}

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

// ---- the effect composer's store shapes (specs/010) -----------------------

/**
 * The inline error for one control, if any (spec §6): match the op's
 * target by kind plus identifying keys. Pure — components call it per
 * render with the `opErrors` store's value.
 *
 * @param {OpError[]} opErrors
 * @param {string} kind the target kind ('vitals' | 'slot' | 'inv')
 * @param {Record<string, *>} [match] extra target keys that must be equal
 * @returns {OpError | null}
 */
export function findOpError(opErrors, kind, match = {}) {
  return (
    opErrors.find(
      (error) =>
        error.target?.kind === kind &&
        Object.entries(match).every(
          ([key, value]) => error.target?.[key] === value,
        ),
    ) ?? null
  );
}

/**
 * The sheet state handle — svelte stores plus the write surface. Exported
 * as a typedef so components can annotate the handle explicitly (svelte2tsx
 * will not infer store-ness through a call's return type).
 *
 * @typedef {object} SheetState
 * @property {import('svelte/store').Readable<Record<string, *> | null>} view
 *   the E8 EngineOutput contract shape, verbatim from the wire — null until
 *   the snapshot or a `derived` frame delivers it (the sheet's loading state)
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
 * @property {import('svelte/store').Readable<Record<string, {qty: number, pending: boolean}>>} qtyMap
 * @property {import('svelte/store').Readable<boolean>} syncing
 * @property {import('svelte/store').Readable<boolean>} offline
 * @property {import('svelte/store').Readable<OpError[]>} opErrors
 * @property {import('svelte/store').Readable<{phase: string, reason?: string} | null>} composerOp
 *   the in-flight composer op: `writing` → `applied` (dialog closes) or
 *   `denied` with the server's reason verbatim (spec FR-C5)
 * @property {import('svelte/store').Readable<{status: string, rows: Array<any>}>} partyEffects
 *   the manager's REST rows (GET /api/parties/{party_id}/effects — effect_id
 *   + version for CAS addressing)
 * @property {(partyId: number, fetchImpl?: typeof fetch) => Promise<void>} loadPartyEffects
 * @property {(payload: any) => void} createEffect
 * @property {(effectId: number, targets: number[], baseVersion: number) => void} retargetEffect
 * @property {(effectId: number, baseVersion: number) => void} endEffect
 * @property {() => void} clearComposerOp
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
 * @property {(casterKey: string) => void} resetPrep
 * @property {() => void} newDay
 * @property {() => void} destroy
 */

/**
 * @param {{
 *   sync: import('../sync/index.js').Sync,
 *   character: Bootstrap,
 *   fetchImpl?: typeof fetch,
 * }} setup
 * @returns {SheetState}
 */
export function createSheetState({ sync, character, fetchImpl }) {
  const characterId = character.character.id;
  const baseSheet = character.base_sheet;

  // ---- affordance stores -------------------------------------------------
  const syncing = writable(sync.isSyncing());
  const connectionState = writable(sync.connectionState());
  const offline = derived(connectionState, ($state) => $state === 'offline');
  /** Rejected/forbidden ops awaiting inline display, keyed by field. */
  const opErrors = writable(/** @type {OpError[]} */ ([]));

  // ---- the effect composer (specs/010) ------------------------------------
  const composerOp = writable(/** @type {{phase: string, reason?: string} | null} */ (null));
  /** targetKey of the op the composer is waiting on — acks and exposures
   *  settle by key, so the composer never needs the op id back. */
  let pendingEffectKey = /** @type {string | null} */ (null);
  const partyEffects = writable(/** @type {{status: string, rows: Array<any>}} */ ({
    status: 'idle',
    rows: [],
  }));

  /**
   * The manager's rows: GET /api/parties/{party_id}/effects — the merged
   * read face (engine_host::rest::effects), rows carrying effect_id + the
   * version the CAS ops address. Called when the composer opens; settled
   * ops broadcast diffs the store already merges.
   *
   * @param {number} partyId
   * @param {typeof fetch} [injectFetch]
   */
  async function loadPartyEffects(partyId, injectFetch) {
    partyEffects.set({ status: 'loading', rows: [] });
    const doFetch = injectFetch ?? fetchImpl ?? globalThis.fetch;
    try {
      const response = await doFetch(`/api/parties/${partyId}/effects`);
      if (!response.ok) throw new Error(`the server answered ${response.status}`);
      const rows = await response.json();
      partyEffects.set({ status: 'ready', rows: Array.isArray(rows) ? rows : [] });
    } catch {
      partyEffects.set({ status: 'error', rows: [] });
    }
  }

  /**
   * The CAS version for an effect mutation: the live store's field when a
   * diff has landed (newest truth, TCP-ordered), the REST face's row
   * otherwise (a cold client has no diff yet). The server owns the verdict
   * either way — a lost race comes back `superseded` (FR-14).
   *
   * @param {Record<string, *>} target @param {number} restVersion
   */
  function effectCasVersion(target, restVersion) {
    const stored = sync.state()[targetKey(target)];
    const storeVersion = stored ? stored.version : 0;
    return storeVersion > 0 ? storeVersion : restVersion;
  }

  /** @param {any} payload the composer's form: {partyId, sourceCharacterId,
   *   name, targets, modifiers, durationNote} */
  function createEffect(payload) {
    const value = effectCreateValue(payload);
    if (value === null) return; // bounds refuse; the form's own state shows why
    const target = effectNewTarget(payload.partyId);
    pendingEffectKey = targetKey(target);
    composerOp.set({ phase: 'writing' });
    write(target, value);
  }

  /** Whole-target-set retarget (FR-14 CAS on the row version).
   *  @param {number} effectId @param {number[]} targets @param {number} restVersion */
  function retargetEffect(effectId, targets, restVersion) {
    const distinct = [...new Set(targets)].filter((id) => Number.isInteger(id) && id > 0);
    if (distinct.length < 1) return;
    const target = effectTarget(effectId);
    pendingEffectKey = targetKey(target);
    composerOp.set({ phase: 'writing' });
    write(target, { op: 'update', targets: distinct }, effectCasVersion(target, restVersion));
  }

  /** @param {number} effectId @param {number} restVersion */
  function endEffect(effectId, restVersion) {
    const target = effectTarget(effectId);
    pendingEffectKey = targetKey(target);
    composerOp.set({ phase: 'writing' });
    write(target, { op: 'end' }, effectCasVersion(target, restVersion));
  }

  function clearComposerOp() {
    pendingEffectKey = null;
    composerOp.set(null);
  }

  // ---- the live field map + engine view ----------------------------------
  const fields = writable(/** @type {Record<string, FieldEntry>} */ (sync.state()));
  const pendingKeys = writable(/** @type {Set<string>} */ (new Set()));

  /**
   * The engine view: the character's EngineOutput, verbatim from the wire
   * (contract Q3 — the server computes; this layer only forwards through
   * the engine seam). Null until the snapshot or a `derived` frame
   * delivers it; `level_adjust` re-derives server-side, so the view moves
   * only when the wire moves — the write below never touches it.
   * @type {import('svelte/store').Writable<Record<string, *> | null>}
   */
  const view = writable(sync.derived(characterId));
  /** Pull the seam's current answer into the store (reference-guarded —
   * the same output object arriving twice must not re-render the sheet). */
  function refreshView() {
    const next = derive(sync, characterId);
    if (get(view) !== next) view.set(next);
  }

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

  // The ceilings come from `render_base` (contract §3): null until the
  // wire delivers — the coupled writes no-op and their controls render
  // disabled until then (design §7's loading state, no invented numbers).
  const hpMax = derived(view, ($view) => $view?.render_base?.hp_max ?? null);
  // The readout clamps to max too (review finding 11): a level-down must
  // never display "32 / 16" — the write clamp alone leaves stale values.
  const hp = derived([vitalsStore('hp', character.vitals.hp), hpMax], ([$hp, $max]) => ({
    value: $max === null ? $hp.value : clamp($hp.value, 0, $max),
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

  /** The focus pip ceiling: `render_base.focus_max` (null until the wire). */
  const focusMax = derived(view, ($view) => $view?.render_base?.focus_max ?? null);
  const heroMax = derived(view, ($view) => $view?.render_base?.hero_max ?? null);

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

  /**
   * Effective quantity per item name, one map — the inventory pane's input
   * (a plain object per render, SSR-safe; the pane takes it as a prop).
   */
  const qtyMap = derived([fields, pendingKeys], ([$fields, $pending]) => {
    /** @type {Record<string, {qty: number, pending: boolean}>} */
    const out = {};
    for (const [name, base] of baseQty) {
      const target = invTarget(characterId, name);
      const key = targetKey(target);
      const entry = $fields[key];
      const pending = $pending.has(key);
      const delta =
        pending || (entry && entry.version > 0)
          ? (entry ? entry.value.qty_delta : 0)
          : (storedDelta.get(name) ?? 0);
      out[name] = { qty: Math.max(0, base + delta), pending };
    }
    return out;
  });

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
      refreshView(); // the snapshot's derived array rides the fields event
    } else if (event.type === 'derived') {
      refreshView();
    } else if (event.type === 'applied' && event.key) {
      // A win on a field clears its inline error (auto-clear on next ack).
      opErrors.update((errors) => errors.filter((error) => error.key !== event.key));
      // The composer's win: the dialog closes (the parent owns the
      // transition), the store shows the settled echo (spec FR-C5).
      if (pendingEffectKey !== null && event.key === pendingEffectKey) {
        pendingEffectKey = null;
        composerOp.set({ phase: 'applied' });
      }
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
      // The composer's denial: the server's reason, verbatim, inline (the
      // dialog stays open — spec FR-C5). Never swallowed.
      if (pendingEffectKey !== null && key === pendingEffectKey) {
        pendingEffectKey = null;
        composerOp.set({
          phase: 'denied',
          reason: event.reason || 'The server rejected the change.',
        });
      }
    }
  }
  const unsubscribe = sync.subscribe(handleSyncEvent);

  // ---- write surface (client-side bounds live here) ------------------------
  /** @param {Record<string, *>} target @param {*} value @param {number} [baseVersion]
   *  an explicit CAS version (the effect mutations' whole-row version); the
   *  field writes keep the store-derived default. */
  function write(target, value, baseVersion) {
    // A retry on a field clears its error immediately — the user acted.
    const key = targetKey(target);
    opErrors.update((errors) => errors.filter((error) => error.key !== key));
    sync.write(target, value, baseVersion);
  }

  /** @param {number} value */
  function writeHp(value) {
    const max = get(hpMax);
    if (max === null) return; // engine output not on the wire yet
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
    if (max === null) return; // engine output not on the wire yet
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return;
    write(vitalsTarget(characterId, 'focus_current'), clamp(Math.round(parsed), 0, max));
  }

  /** @param {number} value */
  function writeHeroPoints(value) {
    const max = get(heroMax);
    if (max === null) return; // engine output not on the wire yet
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

  /**
   * Reset one caster's preparation to the export's list (spec §2.3): every
   * slot whose prepared spell drifted gets a whole-slot write back to the
   * bootstrap value (which is the export's seeding, FR-12). Used flags stay.
   * @param {string} casterKey
   */
  function resetPrep(casterKey) {
    const current = get(slots);
    for (const row of character.slots) {
      if (row.caster_key !== casterKey) continue;
      const live = current.find(
        (candidate) =>
          candidate.caster_key === casterKey &&
          candidate.rank === row.rank &&
          candidate.slot_index === row.slot_index,
      );
      if (live && live.prepared_spell !== row.prepared_spell) {
        writeSlot(casterKey, row.rank, row.slot_index, { prepared: row.prepared_spell });
      }
    }
  }

  /** The New Day burst (spec §3: clear cast slots, refill focus, reset
   * drain): every slot's used flag, focus back to the character's pool
   * (review finding 2 — the pool regains its points, it is not emptied),
   * then the daily whole-row.
   *
   * No-op before the wire speaks (review finding F3): the focus refill
   * has no source until `render_base` arrives, and firing the slot/daily
   * halves alone would reset two of the three things spec §3 promises in
   * one silent stroke. The header's New Day button is dark in the same
   * window — this guard is the state-layer backstop. */
  function newDay() {
    if (get(view) === null) return;
    for (const row of get(slots)) {
      if (row.used || row.pending) {
        writeSlot(row.caster_key, row.rank, row.slot_index, { used: false });
      }
    }
    const focus = get(focusMax);
    if (focus !== null) {
      write(vitalsTarget(characterId, 'focus_current'), focus);
    }
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
    qtyMap,
    syncing,
    offline,
    opErrors,
    composerOp,
    partyEffects,

    loadPartyEffects,
    createEffect,
    retargetEffect,
    endEffect,
    clearComposerOp,
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
    resetPrep,
    newDay,
    destroy: () => unsubscribe(),
  };
}
