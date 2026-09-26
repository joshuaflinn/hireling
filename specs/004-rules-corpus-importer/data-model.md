# Data Model: Rules Corpus Importer (E4) — row contract for E2

**Status**: Draft for human review · **Created**: 2026-09-20

> **OWNERSHIP LAW**: E2 owns ALL DDL (its migration branch). E4 writes ROWS ONLY.
> This document is E4's column-level *request* — the interface contract between
> the importer and E2's schema. Names and types here are a proposal; E2's
> merged migrations are authoritative and E4 adapts to them. Anything E4
> specifically needs is marked **[E4-needs]**.

## Entities

### `rules_corpus` — one row per imported game element

Shared shape for conditions and items (one table, `kind` discriminates).
Rationale for one table: both categories are "a raw doc + provenance +
lane"; the only condition-specific fields (`tier`, `is_valued`) are null
for items. Two tables would double the upsert code for no integrity gain.

| Column | Type (proposal) | Notes |
|---|---|---|
| `id` | uuid PK | Surrogate key; internal FK target. |
| `kind` | text | `'condition'` or `'item'` at POC. CHECK constraint. New categories = new importer version (spec assumption). |
| `upstream_id` | text | Foundry `_id` (e.g. `TBSHQspnbcqxsmjL`). The stable identity (FR-15). |
| `slug` | text | URL-ish identifier where derivable (e.g. rule-element slug / filename); carried for human readability and downstream name-matching aid. |
| `name` | text | Display name as of this release. Renames update in place. |
| `lane` | text | `'core' \| 'imported' \| 'custom'`. **Importer only ever writes `'imported'`.** |
| `tier` | text NULL | `'engine_math' \| 'display_only'`; conditions only, never null for `kind='condition'`, always null for items (FR-8). |
| `is_valued` | boolean | Conditions only; from the seed mapping (frightened=yes). Items: false. |
| `content` | jsonb | The raw upstream document, verbatim. Consumers (E9 display, E12 book value) read from this; structure lives here so the importer doesn't fabricate columns for fields no consumer needs yet. |
| `content_hash` | text | sha256 (hex) of the raw document bytes. The change detector; same hash + same importer version → no write (FR-3). |
| `publication_license` | text | From `system.publication.license` (e.g. `'ORC'`). Feeds the license verdict (FR-18). |
| `publication_title` | text | e.g. `'Pathfinder Player Core'`. |
| `remaster` | boolean | From `system.publication.remaster`. POC imports remaster content; the field keeps a future legacy-pack import a data decision (spec assumption). |
| `pack_release` | text | Pinned tag, e.g. `'pf2e-8.5.1'` (FR-7). |
| `imported_at` | timestamptz | This row's last actual write (FR-7). Skipped rows keep their stamp — no provenance churn. |
| `importer_version` | integer | `IMPORTER_VERSION` at write time (FR-13). |
| `created_at` / `updated_at` | timestamptz | E2 house standard (EPICS.md E2 guardrail). |

**[E4-needs] constraints/indexes:**

1. `CHECK (lane IN ('core','imported','custom'))` — backstop for lane
   discipline.
2. `CHECK (kind IN ('condition','item'))` — POC categories.
3. `UNIQUE (kind, upstream_id) WHERE lane = 'imported'` — **the load-bearing
   one**: makes the importer's upsert race-free and exact, and leaves
   `custom`/`core` rows outside it so a custom row named like an upstream
   row collides harmlessly (spec edge case).
4. Index `(kind, lane)` — the pickers' read path (E8 condition picker, E12
   item name-match).
5. Conditions-tier invariant, if E2 tolerates a CHECK:
   `CHECK (kind <> 'condition' OR tier IS NOT NULL)` and
   `CHECK (kind = 'condition' OR tier IS NULL)`.

### `condition_modifier_mappings` — engine-math rows for conditions

One row per modifier a tier-`engine_math` condition carries. Display-only
conditions have **zero rows here** (FR-10); the absence is meaningful, not
a join failure.

| Column | Type (proposal) | Notes |
|---|---|---|
| `id` | uuid PK | |
| `corpus_id` | uuid FK → `rules_corpus.id` | Only rows with `kind='condition' AND tier='engine_math'` are referenced. |
| `modifier_type` | text | `'circumstance' \| 'status' \| 'item' \| 'untyped'` (CHECK). The PRD's four types. |
| `stat` | text | Closed vocabulary: single stats (`ac`, `fort`, …, `skill:<name>`) or blanket targets (`all_checks`, `all_dcs`, `all_checks_and_dcs`). Validated app-side at seed load; **[E4-needs]** no DB enum — the vocabulary is E8's and lives in code. |
| `value_kind` | text | `'constant' \| 'condition_value'`. Valued conditions use `condition_value` — ONE mapping row serves frightened 1…4 (FR-9). |
| `value` | integer NULL | Set iff `value_kind='constant'`. Negative = penalty. |

**[E4-needs]:** `UNIQUE (corpus_id, modifier_type, stat)` — duplicate
mappings for a condition are a seed bug; the constraint fails the import
loudly. Rows in this table are wholesale replaced when their parent
condition's content hash changes (delete-and-reinsert inside the category
transaction — simpler than diffing, and FR-3 still holds because unchanged
conditions are skipped entirely).

### `import_runs` — one row per importer execution

The run log (FR-12, SC-8). E4 is its only writer.

| Column | Type (proposal) | Notes |
|---|---|---|
| `id` | uuid PK | |
| `pack_release` | text | The pinned tag. |
| `importer_version` | integer | |
| `started_at` / `finished_at` | timestamptz | |
| `outcome` | text | `'success' \| 'failed'`. |
| `counts` | jsonb | Per-table `{inserted, updated, skipped, stale}` — the per-table counts FR-12 requires logging. |
| `stale_rows` | jsonb | List of `{kind, upstream_id, name}` kept-but-absent-upstream (FR-15). |
| `unmapped_conditions` | jsonb | New upstream conditions with no seed entry → display-only; listed for the next seed PR. |
| `notes` | text NULL | e.g. "freshness regression: re-imported older release". |
| `error` | text NULL | Failure summary when `outcome='failed'`. |

## State transitions

- Corpus row lifecycle: `inserted → (content change) updated → (upstream
  removal) kept-forever, listed stale`. **There is no importer-driven
  delete.** Reconciliation of stale rows is a human act, outside this epic.
- Run lifecycle: `running → success | failed`. A failed run writes its
  `import_runs` row in a *separate* transaction after the category
  transactions roll back — the failure must itself be logged (SC-8).

## Validation rules (enforced app-side, pre-write)

- Every doc validates against its category's declared shape (contracts
  doc) before any DB write (FR-13).
- Every mapping row's `stat`/`modifier_type` comes from the closed
  vocabulary, checked when the seed file loads — a bad seed fails at
  startup of the import run, not mid-transaction.
- Tier/mapping consistency: `tier='engine_math'` ⇒ ≥1 mapping row;
  `tier='display_only'` ⇒ 0 mapping rows. Asserted in the transform and
  re-verified by integration test (SC-4's "no condition in both or
  neither").

## Explicit non-requests (what E4 does NOT ask E2 for)

- No staging tables (in-memory transform — design.md).
- No tables/flags for spells, feats, bestiary (FR-1 deferral).
- No prose/curation columns (E9/E15's layer; `content` jsonb covers display).
- No soft-delete columns on corpus rows (nothing is ever deleted).
