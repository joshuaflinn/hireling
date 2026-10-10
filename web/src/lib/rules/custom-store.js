// The custom-rows client (E9 T7, contracts/custom-rows-rest.md): the ONLY
// module that talks to the custom REST. Reads are party-scoped GETs; writes
// are POSTs into the `custom` lane. Visibility is D3's: the creating client
// renders its row immediately (optimistic insert here), everyone else picks
// the row up on their next fetch (picker open, composer open, sheet boot).
//
// The client re-validates the server's caps before any request leaves
// (defense in depth, FR-4 AC-3) — an invalid row never leaves the client,
// and the server's 400 body surfaces with the same {field, reason} shape.

/* global fetch */
import { writable } from 'svelte/store';

/** The caps (contract §1) — one table, the form and the store both read it. */
export const CAPS = {
  nameMax: 64,
  descriptionMax: 280,
  rankMin: 0,
  rankMax: 10,
  valueMin: 1,
  valueMax: 20,
};

/**
 * Client-side mirror of the server's validation. Returns null when the row
 * is creatable, else `{field, reason}`.
 *
 * @param {'item' | 'spell' | 'condition'} kind
 * @param {{name: string, description?: string, value_or_rank?: number | string | null}} fields
 * @returns {{field: string, reason: string} | null}
 */
export function validateCustom(kind, { name, description = '', value_or_rank = null }) {
  const trimmedName = String(name ?? '').trim();
  if (!trimmedName) return { field: 'name', reason: 'required (1..64 characters)' };
  if (trimmedName.length > CAPS.nameMax) {
    return { field: 'name', reason: `over the ${CAPS.nameMax}-character cap` };
  }
  const trimmedDescription = String(description ?? '').trim();
  if (trimmedDescription.length > CAPS.descriptionMax) {
    return { field: 'description', reason: `over the ${CAPS.descriptionMax}-character cap` };
  }
  if (kind === 'spell') {
    const rank = Number(value_or_rank);
    if (!Number.isInteger(rank) || rank < CAPS.rankMin || rank > CAPS.rankMax) {
      return { field: 'value_or_rank', reason: `rank must be ${CAPS.rankMin}..${CAPS.rankMax}` };
    }
  } else if (kind === 'condition') {
    if (value_or_rank !== null && value_or_rank !== undefined && value_or_rank !== '') {
      const value = Number(value_or_rank);
      if (!Number.isInteger(value) || value < CAPS.valueMin || value > CAPS.valueMax) {
        return {
          field: 'value_or_rank',
          reason: `value must be ${CAPS.valueMin}..${CAPS.valueMax}`,
        };
      }
    }
  } else if (value_or_rank !== null && value_or_rank !== undefined && value_or_rank !== '') {
    return { field: 'value_or_rank', reason: 'items take no value' };
  }
  return null;
}

/** @typedef {{corpus_entry_id: number, kind: string, name: string,
  lane: 'custom', description: string, value_or_rank: number | null,
  created_by_sub: string}} CustomRow */

/**
 * One party's custom-row store. `spells`/`items`/`conditions` hold the rows
 * as the REST returns them; `conditions` also feeds the sheet's chip join
 * and the picker's value notes.
 *
 * @param {{partyId: number, fetchImpl?: typeof fetch}} setup
 */
export function createCustomStore({ partyId, fetchImpl = fetch }) {
  const spells = writable(/** @type {CustomRow[]} */ ([]));
  const items = writable(/** @type {CustomRow[]} */ ([]));
  const conditions = writable(/** @type {CustomRow[]} */ ([]));
  /** The last failed exchange, for surfaces that show it inline. */
  const lastError = writable(/** @type {string | null} */ (null));

  const listStore = (/** @type {string} */ kind) =>
    kind === 'spell' ? spells : kind === 'item' ? items : conditions;

  /**
   * Read one kind from the party's custom rows (design D3's party-wide
   * pick-up path — call on boot and on surface open).
   * @param {'item' | 'spell' | 'condition'} kind
   */
  async function refresh(kind) {
    try {
      const response = await fetchImpl(`/api/parties/${partyId}/custom?kind=${kind}`);
      if (!response.ok) throw new Error(`the server answered ${response.status}`);
      const rows = /** @type {CustomRow[]} */ (await response.json());
      listStore(kind).set(rows);
      lastError.set(null);
    } catch (error) {
      // Reads degrade to the last fetched state (design D3) — the error is
      // recorded, never thrown into the surface's render path.
      lastError.set(error instanceof Error ? error.message : 'the custom read failed');
    }
  }

  /**
   * Create one custom row. Validates first (no invalid request leaves);
   * a 201 lands the row optimistically in its store and returns it; a 400
   * throws the server's {field, reason}.
   *
   * @param {'item' | 'spell' | 'condition'} kind
   * @param {{name: string, description?: string, value_or_rank?: number | null}} fields
   * @returns {Promise<CustomRow>}
   */
  async function create(kind, fields) {
    const invalid = validateCustom(kind, fields);
    if (invalid) throw invalid;
    // A blank value stays `null` on the wire — `Number(null)` is 0, and the
    // server reads 0 as an out-of-range condition value (1..20) and a spell
    // rank of 0 (cantrip) where "required for spells" was meant (MOR-115 F3).
    const raw = fields.value_or_rank;
    const blank = raw === null || raw === undefined || String(raw) === '';
    const body = {
      kind,
      name: String(fields.name).trim(),
      description: String(fields.description ?? '').trim(),
      value_or_rank: blank || kind === 'item' ? null : Number(raw),
    };
    const response = await fetchImpl(`/api/parties/${partyId}/custom`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!response.ok) {
      let problem = { field: 'name', reason: `the server answered ${response.status}` };
      try {
        const parsed = await response.json();
        if (parsed && parsed.field) problem = { field: parsed.field, reason: parsed.reason };
      } catch {
        // keep the fallback problem
      }
      throw problem;
    }
    const row = /** @type {CustomRow} */ (await response.json());
    listStore(kind).update((rows) => [row, ...rows]);
    return row;
  }

  return { spells, items, conditions, lastError, refresh, create };
}
