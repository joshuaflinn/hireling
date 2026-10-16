/* D5/FR-6's book-value read — the client half of
 * `GET /api/parties/{party_id}/items/book-value?name=<item>` (`src/book_value.rs`).
 *
 * The sell prompt pre-fills with the corpus price and must show "no book
 * value" when the corpus has none (design D5: that absence is a state the user
 * sees, never a silent zero that quietly becomes the asking price). The route
 * answers `{found: true, price}` or `{found: false}`; `price` is the imported
 * corpus coin object, so it may carry `pp`, and a missing price on a
 * `found: true` is treated as the corpus not knowing rather than as zero.
 *
 * A failed read resolves to "unknown" like any other no-match. It must not
 * throw into the sell dialog's open path — that is the same rule
 * `rules/custom-store.js` applies to its own reads, and the route's server half
 * is tested in `src/tests/book_value.rs`.
 */

/** @typedef {import('./stash.js').BankBalance} BankBalance */

/**
 * @param {{partyId: number, fetchImpl?: typeof fetch}} setup
 * @returns {(name: string) => Promise<BankBalance | null>}
 */
export function createBookValueLookup({
  partyId,
  fetchImpl = (/** @type {RequestInfo|URL|string} */ url) => globalThis.fetch(url),
}) {
  return async function lookUpBookValue(name) {
    try {
      const response = await fetchImpl(
        `/api/parties/${partyId}/items/book-value?name=${encodeURIComponent(name)}`,
      );
      if (!response.ok) return null;
      const body = /** @type {{found?: boolean, price?: BankBalance}} */ (await response.json());
      return body.found === true && body.price ? body.price : null;
    } catch {
      return null;
    }
  };
}
