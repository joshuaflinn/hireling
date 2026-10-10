# Data Model: Rules Tooltips + Custom Content Entry (E9)

**Status**: signed with design.md (Korrin, 2026-10-20).

**Zero migrations.** E9 writes rows into tables E2 already owns and adds
two checked-in data files. Nothing here requests DDL.

## 1. `corpus_entries` — custom rows (writer contract E9, per E2's comment)

E2's migration 4 names the deal: "`data` … shape per kind is the writer's
contract (E4 for imported, E9 for custom/core)". This is E9's side.

| Column | Custom-row value |
|---|---|
| `kind` | `'item'` \| `'spell'` \| `'condition'` |
| `name` | user input, 1..64 chars trimmed |
| `lane` | `'custom'` (CHECK already allows) |
| `data` | `{"custom": {"description": str ≤280, "value_or_rank": int?}}` — **no `import` block** (picker NULL-tier default yields `display_only`; Clarify Q8) |
| `modifiers` | `NULL` (⇒ picker `valued: false`; apply path ⇒ `tracked_manually`, zero math) |
| `source_id` | `NULL` — the structural importer-isolation guarantee (FR-8) |
| `pack_version`, `imported_at` | `NULL` |
| `created_by_sub` | creating account (FK `accounts`; E2 built the column for this epic) |

Notes:

- Custom conditions are tier-`display_only` **by absence**, not by
  writing importer-namespace fields — E8's read logic supplies both
  defaults. No E8 contract change.
- The `custom` block's `description` is plain text (never markup —
  `contracts/inert-html.md` §5); `value_or_rank` is a display note for
  conditions, the preparable rank for spells.
- `character_inventory_live` gains rows only via custom **item** creates
  (one per create, owned by the creator's character); qty changes
  thereafter are ordinary `inv` writes. No shape change.

## 2. Audit events — existing enum, new emissions

`forbidden_custom_write` (E2 migration 7) — emitted by the PATCH path on
non-creator edits, GM writes, and lane violations; `target` carries
`custom/{corpus_entry_id}`, `outcome='denied'`. Successful creates/edits
are **not** audit events (the table's charter is rejections + logins).

## 3. Checked-in data files (new)

### `web/src/lib/rules/condition-prose.json` — the curated seed

Ported **verbatim** from the frozen prototype's `CONDITIONS` map (42
entries, merge `cb0f397`); build-time import (design D1):

```jsonc
{
  "_readme": ["human-reviewed, PR-reviewed curation data (E4's",
    "condition-tiers.json pattern). Paraphrase, page cite, and AoN id",
    "for every Player Core condition as of the prototype freeze.",
    "Changes by PR; in-app editing is E15."],
  "frightened": { "page": 444, "aonId": 76, "text": "Status penalty equal to the value on all checks and DCs. …" },
  "persistent damage": { "page": 445, "aonId": 86, "text": "…" },
  "broken":    { "page": 442, "aonId": 60, "link": false, "text": "…" }
}
```

- Key: lowercase condition name; joined to corpus/effect names by
  **case-insensitive exact match** (client-side, `rules/prose.js`).
- `link: false` — the prototype's `nl` flag (objects-only/meta
  conditions excluded from linkification).
- `page` — Player Core page cite; `aonId` — Archives of Nethys
  condition id (`https://2e.aonprd.com/Conditions.aspx?ID={aonId}`).
- Coverage test (plan Task 2): every seed key matches a condition name
  in the pinned corpus fixture; two-fixture rule — a matching fixture
  and a deliberately renamed one prove the join, not a constant.

### `NOTICE.md` — extended (not new)

Adds the curated-paraphrase lane under the Paizo Community Use Policy
(consistent with archived ORC/OGL), citing the prototype freeze as
provenance. Rendered in-app by `AboutView` through the inert setter.

## 4. State that does NOT change

- Wire protocol: no new frames; custom-condition apply rides the
  existing `effect_new` corpus create; spell prep rides existing slot
  writes; item qty rides existing `inv` writes.
- Engine: untouched — no vocabulary changes, no apply changes (NULL
  modifiers already resolve to tracked-manually).
- `GET /api/parties/{id}/conditions`: response shape unchanged; custom
  rows appear through the existing query.
- Service worker: no contract change; the prose seed rides the cached
  shell because it is a build-time module.
