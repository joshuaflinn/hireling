# Design: Database Schema (E2)

**Epic**: E2 — Phase 0, P0 · GitHub issue #4 · branch `issue-4-database-schema`
**Spec**: [`spec.md`](spec.md) (the law — WHAT and WHY; this document is the HOW)
**Status**: Approved (human ruling 2026-09-20: the spec's Assumptions are
settled law; the recommended brainstorm approach was adopted with its
documented defaults — global version sequence, app-boot embedded migrations,
lane-only corpus — plus one cross-epic amendment: E3's accounts-keying
contract, `accounts.sub TEXT PRIMARY KEY`, wins over a surrogate id; all
ownership bindings FK to `accounts.sub`)

> **Standing rule (from the specify round, load-bearing): E2 owns ALL DDL,
> including the corpus tables. E4 writes ROWS ONLY into `corpus_entries` and
> carries zero migration files.** This prevents a migration-ownership race
> between the parallel E2/E4 branches. It is restated in §3 and in
> data-model.md §5.

## 1. Architecture

E2 extends E1's scaffold shape; it invents no new patterns.

- **`src/db.rs` (thin shell, new):** builds a `sqlx::PgPool` from
  `Settings.database_url` (the env-var pattern from `config.rs` is unchanged —
  `HIRELING_DATABASE_URL` already exists and defaults at the compose
  throwaway database). Pool `max_connections` ≈ 10 — deliberate headroom under
  the production role's `CONNECTION LIMIT 20`.
- **Startup sequence in `lib.rs run()`:** settings → pool → **run embedded
  migrations** (`sqlx::migrate!("./migrations")`) → serve. A schema the app
  can't reach or can't apply fails the boot loudly, before traffic — never at
  first query. (Human-settled default: app-boot migrate, single binary, single
  replica.)
- **`/healthz` stays dependency-free** per E1's guardrail: it never touches
  the pool. Monitors must keep distinguishing "process dead" from "database
  dead".
- **No endpoints in E2.** The pool becomes axum state that E5/E7/E8 consume;
  E2 delivers persistence and its guarantees only.
- **Local dev loop:** the compose throwaway Postgres (already in
  `compose.yml`, no volume — `docker compose down` destroys everything). New
  `just` recipes in the justfile's convenience-over-requirement style
  (`db-migrate`, `db-revert`, `db-reset`), each documented in README as the
  raw `sqlx migrate` command. No documented step or default ever points at
  Asgard (FR-3).
- **Production provisioning:** a checked-in SQL artifact
  (`db/provision.sql`) creating database `hireling` and role `hireling` with
  `CONNECTION LIMIT 20` (the Langfuse hall precedent), plus comments recording
  what DDL cannot express: reachable only over the `asgard-net` bridge, no
  published ports, rides Asgard's existing backup rotation. Run once by the
  operator; expressible as artifacts, never console click-ops (FR-2).

## 2. Migration strategy

- **Tool:** `sqlx migrate`, migrations checked into `migrations/` at repo
  root. Every migration is a **reversible up/down pair** (FR-1). Down
  migrations are real — drops in reverse dependency order — and are exercised
  by the SC-1 test loop, not vestigial.
- **File grouping:** fine-grained, domain-ordered files (01 identity core →
  02 live state → 03 effects → 04 corpus → 05 stash/bank/claim → 06 machinery:
  version sequence, append-only trigger). Small files keep down-migrations
  honest and reviews readable. Exact file split is plan-stage.
- **Partial-failure behavior:** sqlx's migration bookkeeping makes a failed
  run resume-or-fail-loud on retry; no silent half-application (spec edge
  case).
- **E4 boundary:** E4's branch contains no migrations. Its importer is written
  against the `corpus_entries` contract (data-model §5) and inserts/upserts
  rows only.
- **Prod execution:** app-boot embedded migrations (§1). The runtime role
  therefore owns DDL rights in its own hall; this is acceptable because the
  hall is dedicated, the deployment is single-replica, and claim-history
  append-only is trigger-enforced (works for the table-owning role, which
  REVOKE would not — data-model §6).

## 3. Entity/table map (conceptual)

Full field-level detail lives in [`data-model.md`](data-model.md). The map:

- **Identity & scope:** `accounts` (**natural key: `sub` text PRIMARY KEY** —
  the provider's stable `sub` claim; plus `username`, `display_name`, and a
  `role` data column for E3's enforcement; upserted at login by E3 — the
  cross-epic keying contract, settled by the human), `parties` (carries the
  quartermaster designation as a nullable character reference — data,
  present now, activated by E12), `characters` (party-scoped, one per
  account, bound to its owner by FK to `accounts.sub`, holding
  `payload_raw` text + `base_sheet` jsonb).
- **Live state (anchor-keyed, survives re-import):** `character_vitals`
  (HP/temp-HP/money/level-adjust, 1:1), `character_spell_slots` (one row per
  individual slot: character × caster × rank × index), `character_inventory_live`
  (one row per item name).
- **Effects:** `effects` + `effect_targets` + `effect_modifiers` — fully
  relational, versioned as a whole.
- **Corpus:** `corpus_entries` — one open-kinded table, lane-constrained
  (`core | imported | custom`), provenance columns per lane, display-only
  rows distinguishable by `modifiers IS NULL`. **E2 owns this DDL; E4 writes
  rows only.**
- **P1 shared inventory (present, unused at P0):** `party_stash`,
  `claim_history` (append-only by trigger), `party_bank` (single balance row
  per party).
- **Machinery:** one global `field_version_seq` feeding every per-field
  `version` column.

**Deliberate exclusions** (stated so they stay excluded):

- No focus-pool live table — the spec's live-state enumeration omits it; E6's
  spec can add it later as a cheap additive migration if it demands one.
- No import history — latest payload only (spec assumption); additive later
  if ever wanted.
- No engine-derived stat totals anywhere (FR-16) — E8 recomputes from
  base + active effects.
- No soft delete anywhere (FR-15) — lifecycle end states are state
  (`effects.active = false`); claim history is append-only regardless.
- No promoted character columns (name/level/class) — they would drift against
  `base_sheet`; roster reads JSON-extract from 5 rows, which is free.

## 4. Versioning model

The spec deferred columns-vs-sidecar; this design settles it: **columns, fed
by one global sequence.**

- Every mutable live-state field owns a sibling `version bigint` column:
  `hp_version`, `temp_hp_version`, `money_version` (money versioned as one
  unit — it's live state per the spec assumptions but not in FR-14's
  granularity list, so one version covers the four denomination columns),
  `level_adjust_version`, and a `version` on each `character_spell_slots`
  row, each `character_inventory_live` row, each `effects` row, and
  `party_bank`.
- **All versions come from a single sequence** (`field_version_seq`).
  Rationale (human-settled default): a total order across all versioned
  fields, not just per-field monotonicity. FR-14 only requires per-field
  monotonic, but the total order is what makes E7's reconnect catch-up a
  single predicate (`WHERE version > $last_seen`) instead of a field-by-field
  diff. Per-field monotonicity falls out of sequence monotonicity for free.
- **Write shape:** one statement per field write —
  `UPDATE … SET hp = $1, hp_version = nextval('field_version_seq') WHERE …`.
  Concurrent writes to the same row serialize on the row lock in arrival
  order for microseconds; **no lock is ever held across a client
  interaction** (SC-6). The version is server-assigned at receipt; client
  clocks are untrusted (E7 constraint).
- **Effects** are versioned as a whole: any change to `effect_targets` or
  `effect_modifiers` bumps `effects.version` inside the same transaction.
- **Stash** joins the scheme when E12's spec defines its reconciliation
  granularity — additive version columns then, which the spec explicitly
  reserves to E12.

## 5. Reconciliation support (what the consumers get)

### E5 — import / re-import

- Re-import is one `UPDATE characters SET payload_raw = …, base_sheet = …`
  plus an anchor-reconciliation pass over the three live tables. Live rows
  hold **no FK into base-sheet contents** — that absence is the mechanism:
  wholesale base replacement physically cannot cascade-delete or
  silently orphan live state (SC-5 is structural, not disciplined).
- Anchors (spec-settled): HP/temp-HP by character identity (the
  `character_vitals` row itself); slot usage and preparation by
  (`caster_key`, `rank`, `slot_index`); inventory deltas by `item_name`.
- `caster_key` is the spellCasters block's `name` from the export
  (`"Wizard"`, `"Wellspring Gnome"`). Rank+index alone is ambiguous — the
  reference export carries two caster blocks with overlapping ranks.
  Matching blocks across re-imports is E5's job; a renamed block orphans its
  slot rows under the same keep-and-surface rule as renamed items.
- Orphans are kept, never dropped: renamed item, vanished slot, renamed
  caster block — the live row survives and E5 surfaces it in the post-import
  diff. The diff is computed at import time and not persisted (spec
  assumption).

### E7 — party sync

- Per-field versions + total order (§4): broadcast diffs carry
  (field, value, version); clients track last-seen version per field;
  superseded queued writes are detected by version comparison and silently
  dropped (PRD FG2).
- Every live/effect/stash table is party-scoped, so broadcast fan-out is
  `WHERE party_id = …`.
- **No LISTEN/NOTIFY, no outbox, no queue** (Constitution V: nothing
  alongside Postgres). Broadcast is the server process's concern on write
  success. If E7 ever wants DB-level fan-out that's its spec's conversation;
  the default is in-process.

### E4 — corpus importer

- Upsert key: partial unique index on `(kind, source_id)` where `source_id`
  is present — "same pack, same row" reconciles idempotently per re-run.
- Lane CHECK makes a fourth lane unstorable; `custom` rows carry no
  `source_id`, so they are structurally never in any pack's upsert scope —
  re-runs cannot touch them (spec edge case, by construction).
- `modifiers IS NULL` is the display-only test (US5.4): no prose parsing, no
  fabricated modifier rows.
- Per-row provenance (`pack_version`, `imported_at`) is the data-freshness
  indicator E4's guardrails require.

## 6. Testing strategy

DB-backed tests are Rust integration tests (cargo `tests/`), gated on the
documented local database URL with a loud skip when absent — never a silent
green. `just db-reset` wraps the same cycle for humans. House style
(sibling/flat test layout, `just ci-local` as the gate) is preserved.

| Criterion | Test |
|---|---|
| SC-1 | Migrate-up → verify objects → migrate-down → verify empty, **3 consecutive cycles**, zero manual steps |
| SC-2 | Seed two parties (five characters, effects, inventory) by inserts only; assert isolation on every party-scoped table |
| SC-3 | Parametrized orphan-insert against every FK path; assert rejection |
| SC-4 | Schema inspection for `created_at`/`updated_at` on every entity table + write round-trip on a sample |
| SC-5 | Simulated re-import: snapshot live rows, replace payload + base sheet, assert live rows byte-identical; plus orphan-anchor case (renamed item survives) |
| SC-6 | Two writes to one field in chosen receipt order: final value = later write, two strictly increasing versions, no transaction held across the simulated client round-trip |
| SC-7 | `claim_history`: insert succeeds; UPDATE and DELETE both raise |
| US5 | Lane CHECK rejects a fourth value; imported provenance round-trips; `modifiers IS NULL` tier test; custom row records creator |

**CI note (flagged open item, plan-stage):** the pinned grizzly-gate image
runs in standalone checker mode; whether it can host a Postgres service for
DB-backed tests is unknown. Until resolved, these tests run in
`just ci-local` against the compose database (documented prerequisite), and
the CI story needs Bear's input on the gate image. Not a design blocker.

## 7. Decisions raised and dismissed

- **Fully relational base sheet + version sidecar** — rejected. ~15 extra
  tables modeling an unofficial, drifting export schema turns every
  Pathbuilder drift into a migration (the failure mode the spec prices as
  most expensive), and a sidecar demotes FR-14's guarantee from
  schema-visible to data-visible while doubling every versioned write.
- **Document-store (one jsonb per character, versions inside)** — rejected.
  Entangles live state with payload, making SC-5 app discipline instead of
  schema guarantee; per-field versions inside jsonb are not
  constraint-enforceable; concurrent field writes serialize the whole
  character.
- **Per-field version counters instead of one global sequence** — rejected
  (with the human's settled-default ruling): loses the total order that makes
  E7 catch-up one predicate.
- **Operator-run migrations + DML-only runtime role** — rejected (settled
  default): app-boot migrate keeps the single-binary boring stack; append-only
  is trigger-enforced either way, so nothing is lost.
- **Surrogate `id` on `accounts` with a separate `idp_subject` column** —
  rejected by cross-epic reconciliation (human amendment): E3's keying
  contract wins. `accounts.sub TEXT PRIMARY KEY` (the provider `sub` claim, a
  stable user UUID), plus `username`, `display_name`, `role TEXT
  ('player'|'gm')`, upserted at login by E3; **all ownership bindings FK to
  `accounts.sub`** — no surrogate id. The `role` column is data E3 enforces
  from, not a schema-encoded write ban — the spec assumption ("GM read-only
  is server-enforced, not schema-enforced") still holds.
- **Bank as an append-only entry ledger** — rejected. Movements are already
  in `claim_history`; a second log is complexity, not audit. One balance row.
- **`core`-lane seed data in E2** — rejected (settled default): E2 ships the
  lane and tables only; who writes `core` rows (likely E9's curated tooltip
  prose) is that epic's spec.
- **`updated_at` by trigger** — rejected: app-managed `now()` on write; the
  trigger is machinery without payoff at this scale.
