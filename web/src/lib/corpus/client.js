/* global fetch, URLSearchParams */
// Corpus reads for the Dave-parity surfaces (specs/015). One cache per page
// load: corpus data only changes on an import run.

/** @type {Map<string, Promise<any>>} */
const cache = new Map();

/**
 * @param {string} url
 * @param {typeof fetch} [fetchImpl]
 */
function cached(url, fetchImpl) {
  if (!cache.has(url)) {
    const doFetch = fetchImpl ?? ((u) => fetch(u));
    const p = doFetch(url).then((r) => {
      if (!r.ok) throw new Error(`corpus read ${r.status}`);
      return r.json();
    });
    p.catch(() => cache.delete(url));
    cache.set(url, p);
  }
  return /** @type {Promise<any>} */ (cache.get(url));
}

/**
 * Entries of one kind by exact name and/or upstream id.
 * @param {string} kind
 * @param {{names?: string[], ids?: string[]}} sel
 * @param {typeof fetch} [fetchImpl]
 * @returns {Promise<any[]>}
 */
export function lookup(kind, { names = [], ids = [] }, fetchImpl) {
  if (!names.length && !ids.length) return Promise.resolve([]);
  const q = new URLSearchParams();
  if (names.length) q.set('names', [...new Set(names)].sort().join(','));
  if (ids.length) q.set('ids', [...new Set(ids)].sort().join(','));
  return cached(`/api/corpus/${kind}?${q}`, fetchImpl);
}

/**
 * A filtered browse list.
 * @param {string} kind
 * @param {Record<string, string | number | undefined>} filters
 * @param {typeof fetch} [fetchImpl]
 * @returns {Promise<any[]>}
 */
export function list(kind, filters = {}, fetchImpl) {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(filters)) if (v !== undefined && v !== '') q.set(k, String(v));
  return cached(`/api/corpus/${kind}/list?${q}`, fetchImpl);
}

/**
 * A creature's embedded items (strikes, abilities, spells).
 * @param {number} id
 * @param {typeof fetch} [fetchImpl]
 */
export function creatureItems(id, fetchImpl) {
  return cached(`/api/corpus/creature/${id}/items`, fetchImpl);
}

/** Test seam. */
export function clearCorpusCache() {
  cache.clear();
}
