// The roster state layer (E10 design D3) — the read half of the sheet's
// state module, fanned out per party character. One sync, one fields map:
// every card store reads the SAME merged state the sheets read, so a
// roster number can never drift from its sheet's number.
//
// Cards have no write surface — writes belong to the sheet's state layer
// (`sheet/state.js`), reachable only through SheetView. `syncing` is the
// queue indicator and renders on the owner's own card only.

import { derived, writable } from 'svelte/store';

import { targetKey } from '../sync/store.js';

/** @param {number} characterId @param {string} field */
const vitalsTarget = (characterId, field) => ({
  kind: 'vitals',
  character_id: characterId,
  field,
});

/**
 * @typedef {object} RosterCardState
 * @property {{ id: number, name: string | null, level: number | null,
 *   class: string | null, owner: string }} character the roster summary
 * @property {import('svelte/store').Readable<{value: number, pending: boolean}>} hp
 * @property {import('svelte/store').Readable<{value: number, pending: boolean}>} tempHp
 * @property {import('svelte/store').Readable<number | null>} hpMax null until
 *   the wire delivers the engine output — the honest skeleton state
 * @property {import('svelte/store').Readable<Array<object>>} effects
 * @property {import('svelte/store').Readable<boolean>} syncing
 */

/**
 * @param {{
 *   sync: import('../sync/index.js').Sync,
 *   roster: { characters: Array<*> },
 * }} setup
 * @returns {{ cards: RosterCardState[], destroy: () => void }}
 */
export function createRosterState({ sync, roster }) {
  // The shared read surface: one fields map, refreshed on the same events
  // the sheet state consumes — no second merge, no polling.
  const fields = writable(sync.state());
  const syncing = writable(sync.isSyncing());
  /** Per-character engine output, verbatim from the seam (`sync.derived`),
   *  refreshed on fields+derived events. Map of id → output | null. */
  const views = writable(refreshViews());

  function refreshViews() {
    const next = new Map();
    for (const character of roster.characters) {
      next.set(character.character.id, sync.derived(character.character.id));
    }
    return next;
  }

  const unsubscribe = sync.subscribe((event) => {
    if (event.type === 'fields') {
      fields.set(sync.state());
      syncing.set(sync.isSyncing());
      views.set(refreshViews());
    } else if (event.type === 'derived') {
      views.set(refreshViews());
    } else if (event.type === 'queue') {
      syncing.set(sync.isSyncing());
    }
  });

  const cards = roster.characters.map((entry) => {
    const characterId = entry.character.id;
    const summary = entry.character;

    /** One vitals number: live when the store has carried it (version > 0),
     *  bootstrap vitals otherwise. Cards never write, so pending is false —
     *  optimistic echoes live in the sheet's own state layer.
     *  @param {string} field @param {*} fallback */
    function liveVitals(field, fallback) {
      const key = targetKey(vitalsTarget(characterId, field));
      return derived(fields, ($fields) => {
        const item = $fields[key];
        return item && item.version > 0 ? item.value : fallback;
      });
    }

    const view = derived(views, ($views) => $views.get(characterId) ?? null);
    const rawHp = liveVitals('hp', entry.vitals?.hp ?? null);
    const rawTemp = liveVitals('temp_hp', entry.vitals?.temp_hp ?? null);

    return {
      character: summary,
      hp: derived(rawHp, ($value) => ({ value: $value, pending: false })),
      tempHp: derived(rawTemp, ($value) => ({ value: $value, pending: false })),
      // The ceiling comes from the engine output (render_base, E8's
      // contract): null until the wire speaks — skeleton bar, no invented
      // number (the same honest gap the sheet renders).
      hpMax: derived(view, ($view) => $view?.render_base?.hp_max ?? null),
      effects: derived(view, ($view) => $view?.effects ?? []),
      syncing,
    };
  });

  return { cards, destroy: unsubscribe };
}
