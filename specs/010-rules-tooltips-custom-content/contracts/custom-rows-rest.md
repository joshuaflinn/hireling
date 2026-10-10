# Contract: Custom Rows REST (E9)

**Status**: binding for E9 implementation · the only new server surface ·
routes register in E3's declarative `API_ROUTES` matrix (`src/http.rs`),
deny-by-default, session-gated.

## 1. Routes

### `POST /api/parties/{party_id}/custom`

Create one `custom`-lane row. Body:

```jsonc
{
  "kind": "item" | "spell" | "condition",
  "name": "Conjure Toad Swarm",        // 1..64 chars, trimmed, non-blank
  "description": "…",                  // 0..280 chars, trimmed
  "value_or_rank": 3                   // optional int
                                      //   spell: 0..10 (rank, required)
                                      //   condition: 1..20 (optional, display note —
                                      //     renders in the condition's tip body and
                                      //     picker row; never an apply input, FR-6)
                                      //   item: rejected (400)
}
```

Behavior:

- **Authz**: session account owns a character in `{party_id}`. GM → 403
  (no create; E3 read-only seat). No session → 401 by middleware.
- **Caps** (server re-validates; client validates first): name
  1..64 chars after trim, description 0..280 chars after trim,
  `value_or_rank` bounds above. Violation → `400` with
  `{"error": "validation", "field": "<name>", "reason": "<bound>"}`.
- **Write** (one transaction): `INSERT INTO corpus_entries (kind, name,
  lane, data, created_by_sub)` with `lane='custom'`, `data = {"custom":
  {"description": …, "value_or_rank": …}}`, `source_id = NULL`,
  `modifiers = NULL`. For `kind='item'`, also `INSERT INTO
  character_inventory_live (character_id, item_name)` — one row owned by
  the creating account's character (their first character in the party;
  POC: one character per account) — **`qty_delta = 1` explicitly**, not
  the column default: a custom item has no Pathbuilder anchor base
  (base_qty 0) and quantity renders as `base_qty + delta`
  (`src/pbimport/anchor.rs`), so a row inserted at the default `0`
  renders **qty 0** and fails US-3 AC-1 (qty 1).
- **Response `201`**: the created row as the client renders it —
  `{ "corpus_entry_id": …, "kind": …, "name": …, "lane": "custom",
  "description": …, "value_or_rank": …, "created_by_sub": … }`.
  The creating client uses this to surface the row immediately (design
  D3).

### `PATCH /api/parties/{party_id}/custom/{corpus_entry_id}`

Edit one custom row. Body: any subset of `{ "name": …,
"description": …, "value_or_rank": … }` (same caps).

- **Authz**: session account is the row's `created_by_sub` (sole
  writer, FR-5). Anyone else — including other character owners and the
  GM — → **403** AND an audit row: `event='forbidden_custom_write'`,
  `actor_sub=<session>`, `target='custom/{corpus_entry_id}'`,
  `outcome='denied'` (enum value already exists; E2 migration 7).
- Non-custom lane (`imported`/`core`) → 403 same audit path (curation
  editing is E15; the route structurally cannot edit what it does not
  own).
- **Response `200`**: the updated row (same shape as create).
- Renames: allowed (creator's call); live references that keyed by old
  name (prepared slots, inventory rows) keep their names — documented
  consequence, same as any item rename today.

## 2. Reads (no new read route)

- **Conditions picker** — existing `GET /api/parties/{party_id}/conditions`
  (E8) already returns every corpus condition with `lane`; custom
  conditions appear automatically (`tier` NULL ⇒ `"display_only"`,
  `modifiers` NULL ⇒ `valued: false` — E8's read logic, unchanged).
- **Custom spells/items/conditions for composer, inventory merge, and
  chips** — `GET /api/parties/{party_id}/custom?kind=spell|item|condition`
  returns the client-side list shape `[{corpus_entry_id, name,
  description, value_or_rank, created_by_sub}]`. Custom-condition rows
  carry their `description` so the chip popup can show the creator's
  text (US-1 AC-5); the chips themselves stay name-only on the wire —
  the description join is client-side (design D6). Party-readable
  (member or GM) — reads gate nothing (ownership gates writes).
- Tooltips carry no fetch (design D1/D6).

## 3. Semantics settled by this contract

- **No delete** (spec non-goal). **No WS broadcast** on create/edit
  (design D3): party-wide visibility is the picker/composer query truth;
  clients refresh on open/boot.
- **Importer isolation (corpus importer)**: custom rows carry
  `source_id = NULL` ⇒ outside `corpus_entries_kind_source_id_key`;
  **E4 corpus-importer** re-runs cannot touch them (asserted by test,
  FR-8). The **Pathbuilder importer** is a different path and never
  writes corpus rows — but a custom item is absent from every
  Pathbuilder export by construction, so it surfaces as a kept-delta
  notice (`KeptEntry::item`, `src/pbimport/anchor.rs`) in every
  re-import diff, forever. Accepted: the diff is a review surface,
  never a mutation; the corpus row and inventory row are untouched
  (spec Edge Cases). Suppressing the notice is an explicit non-goal.
- **Idempotency**: creates are not idempotent (no client op ledger —
  they are not sync writes); the client disables submit while a create
  is in flight. Duplicate names are allowed (two creators may share a
  name; each row is distinct).
- **Rate/abuse**: caps + 280-char description are the abuse bound at
  POC (six trusted accounts); no rate limiting ships.

## 4. Test obligations (production-path, through the real router)

- 201 create (each kind) → row present via picker/custom read; custom
  item also produces the inventory row.
- 400 at every cap boundary (empty name, 65-char name, 281-char
  description, rank 11, item with value) with field+reason.
- 403 + audit: GM create; member edit of another creator's row; edit of
  an imported row. Audit rows asserted in `audit_events`.
- FR-8: run the **E4 corpus importer** against a party holding custom
  rows; assert custom rows byte-identical before/after. (The pbimport
  kept-delta notice is expected behavior, not a failure — §3.)
