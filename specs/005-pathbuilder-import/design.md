# Design: Pathbuilder Import Pipeline (E5)

Companion to [`spec.md`](spec.md) · implements the E5 specify prompt
(`docs/EPICS.md`); contracts cited from
[`contracts/pb-export.md`](contracts/pb-export.md); shapes in
[`data-model.md`](data-model.md). Pipeline step 3 of 4 (brainstorm/design).

## 1. Approach chosen — curated normalization over E2's storage, pure core

Three strategies were weighed:

1. **Full relational normalization** — character data modeled as typed tables
   (items, spells, feats…). **Rejected**: E2 settled this — `payload_raw`
   text + `base_sheet` jsonb, "this contract relationally modeled is a
   migration magnet." Re-litigating E2 is out of bounds.
2. **Thin validation, verbatim-only storage** — validate shape, store the
   raw text, serve it back. **Rejected**: E6 must render and E8 must compute
   from typed data; `base_sheet` exists precisely so downstream epics don't
   re-parse the unofficial export format. Every consumer re-parsing the
   export duplicates the drift problem six ways.
3. **Curated normalization + typed passthrough (chosen)** — a pure transform
   builds `base_sheet` with normalized sections for everything downstream
   consumes (identity, abilities, HP inputs, AC breakdown, proficiencies,
   spellcasting with slot layouts, equipment with resolved containers,
   weapons, armor, money, focus, companions) and passes unmodeled-but-known
   sections through verbatim under `raw`. Unknown fields are logged and
   dropped (class (c)). Storage is 100% E2's landed tables — no new tables,
   no new columns.

The parser follows the house thin-shell/pure-core convention (E3 `authz.rs`,
E4 `transform.rs`): parse → validate → transform are pure functions over
`serde_json::Value`-free typed inputs, exhaustively unit-tested against the
frozen fixture and labeled mutations of it. The axum handler and sqlx store
are thin shells.

## 2. Module layout

| Module | Role | Tested by |
|---|---|---|
| `src/pbimport/caps.rs` | size/depth caps (constants + `check` fns) | unit |
| `src/pbimport/model.rs` | envelope types, required-shape validation (class a/b detection + exact messages), unknown-field walker (class c) | unit, fixture + mutations |
| `src/pbimport/transform.rs` | **pure heart**: validated export → `BaseSheet` (normalized sections + `raw` passthrough + container UUID resolution + max-HP derivation) | unit, exhaustive |
| `src/pbimport/anchor.rs` | **pure heart 2**: new slot layout + existing live rows → preserve/keep/seed actions + post-import diff | unit, exhaustive |
| `src/pbimport/store.rs` | sqlx: character upsert-by-owner, first-import vitals seed, slot-row materialization, audit insert | integration (compose Postgres) |
| `src/pbimport/handlers.rs` | axum: `POST /api/characters/import`, `GET /api/characters/me`; error envelope | integration + router matrix |
| `src/pbimport/mod.rs` | orchestration: caps → parse → validate → transform → anchor (load live state) → one transaction → audit → response | integration |
| `web/src/lib/import/` | minimal import page (paste, file, submit, diff render) — FR-18 | `just web-test` |

New routes register in E3's `API_ROUTES` table (the ownership-matrix route
test iterates it — a write route without an explicit authz rule fails the
suite by construction).

## 3. API surface

### `POST /api/characters/import`

- **Body**: the export JSON verbatim (`application/json`). One endpoint
  serves paste and upload — the page posts the same body either way.
- **AuthN/AuthZ**: `resolve_session` → `require_auth` → `gm_read_only`
  (existing layers; GM POSTs are rejected + audited by the middleware before
  the handler runs). Handler asserts `role == player` (belt-and-braces;
  deny-by-default) and scopes everything to `session.sub` — there is no
  character-id parameter to forge.
- **Success (first or re-import)**: `200` with `{character: {id, name,
  level, class, owner, first_import: bool}, diff: <Diff per data-model §5>,
  advisory: {skipped_fields: N}}`.
- **Failure**: `400/413` with E3's standard error envelope carrying
  `{code, message}` — exact strings from `contracts/pb-export.md` §4.
- **Audit**: one `character_import` row per attempt — outcome `allowed` or
  `denied`, target = resulting character id or `import:<failure-code>`.

### `GET /api/characters/me`

Returns the caller's character (or `204`/empty when none): identity summary,
`base_sheet`, vitals, slot rows, inventory live rows. Read-only, self-scoped,
party-readable later by E7/E10. Exists so E5's independent tests and E6 have
one read path; no other character's data is reachable through it.

## 4. The anchoring algorithm (FR-10–FR-13, exact)

```
import(actor, export):
  caps → parse → validate            # classes: size, depth, (a), (b)
  base  = transform(export)          # new BaseSheet
  chars = store.load_character_by_owner(actor.sub)
  if chars is none:                  # FIRST IMPORT
      char   = insert(owner=actor.sub, party=poc_party, payload_raw, base)
      seed_vitals(hp = base.max_hp, temp = 0, money = export.money, level_adjust = 0)
      seed_slot_rows(all slots, used=false, prepared from export lists)
      diff  = empty
  else:                              # RE-IMPORT (FR-8: target is chars)
      live_slots   = load character_spell_slots(char)
      layout       = base.slot_layout()          # [(caster_key, rank, slot_index)]
      for row in live_slots:
          if row.(caster_key, rank, slot_index) in layout: keep row untouched
          else: keep row + diff.add(KeptSlot{...})           # never dropped
      for pos in layout where no live row:
          insert seeded row (used=false, prepared from export) + diff.add(SeededSlot{...})
      for row kept where export_prepared(pos) exists and != row.prepared_spell:
          diff.add(PrepDivergence{pos, live, export})        # live wins, no write
      for live inventory row whose item_name not in base.item_names:
          diff.add(KeptItem{...})                            # kept, never dropped
      for base item with live delta where base.qty + delta < 0:
          diff.add(NegativeQuantity{...})                    # surfaced, not corrected
      replace payload_raw + base_sheet wholesale; touch updated_at
  audit(actor, outcome, target)
  return {character, diff, advisory}
```

Vitals (HP/temp/money/level-adjust) are never read or written on the
re-import path — the anchor is character identity itself (E2's
`character_vitals` design). Active effects are never queried (FR-14).

Determinism: `caster_key` assignment (name, else `name#n` by array position)
is a pure function of the export's block list, so identical exports produce
identical anchor sets and FR-13 idempotency holds; the second import of the
same export finds every live row matched and produces an empty diff.

Concurrency: one import transaction per attempt
(`BEGIN … SELECT … FOR UPDATE on characters … COMMIT`) — a concurrent
double-import from the same account serializes on the character row. Live
state tables are only appended/updated within the same transaction, so a
crash mid-import leaves the previous state fully intact (all-or-nothing).

## 5. Migration findings (raised for gate review — not slipped in)

E5 adds **zero tables and zero columns**. Two small reversible migrations are
proposed and flagged:

1. **`poc_party_seed`** — `INSERT` the single POC party, guarded to fire only
   when `parties` is empty (`INSERT … SELECT WHERE NOT EXISTS (SELECT 1 FROM
   parties)`); down deletes the seeded row only if it is still untouched and
   unoccupied. Rationale: `characters.party_id` is NOT NULL and E2 seeds
   nothing; without a party row the first import cannot insert. Rejected:
   handler-side auto-create (hides party provisioning behind a write path);
   ops runbook (untestable first-run). This is data, not schema — but it is
   a migration file in an epic that was told to raise findings, so it is
   raised.
2. **`audit_import_event`** — `ALTER TABLE audit_events` to add
   `character_import` to the `event` CHECK. Rationale: FR-16 requires
   audit-logged import attempts; E3's closed enum admits none. Rejected:
   tracing-only (not an audit trail); reusing `forbidden_character_write`
   (false record — imports by rightful owners are allowed events).

Both carry up/down pairs and integration tests (seed idempotent under
re-run; CHECK accepts the new kind and still rejects garbage).

## 6. Testing strategy

- **Unit (pure layers)** — `model.rs`: every class (a)/(b) mutation of the
  fixture (truncated, `{"hello":"world"}`, `success:false`, missing each
  required key, wrong types), exact-message assertions, unknown-field walker
  counts nested paths. `transform.rs`: fixture round-trip — every normalized
  section asserted against captured values; container UUID resolution;
  max-HP formula; homebrew names pass through. `anchor.rs`: the full
  preserve/keep/seed matrix — vanished caster, shrunk rank, grown rank,
  duplicate names (`name#2`), innate block, prep divergence, negative
  effective qty, idempotent empty diff.
- **Integration (compose Postgres)** — first import seeds; six accounts/six
  characters roster; re-import drill (US-2 test); double-import
  serialization; audit rows per outcome; migration up/down.
- **Router matrix (E3 suite)** — new routes enumerated in `API_ROUTES`;
  GM POST → forbidden + audit; unauthenticated → 401; owner path only.
- **Frontend** — `just web-test` node runner for the import page's submit
  logic (body assembly, error render, diff render).
- Gate: `just ci-local` (fmt, clippy `-D warnings`, unit + integration with
  compose Postgres, cargo-deny, svelte-check) before PR; grizzly-gate on the
  PR.

## 7. Dependencies

None new. serde/serde_json/axum/sqlx are already in the tree (E1/E3/E4);
the frontend page uses the existing Svelte scaffold. This satisfies Article
V — no written-justification-needed crates.

## 8. Decisions with rejected alternatives (summary)

| Decision | Chosen | Rejected (why) |
|---|---|---|
| Storage strategy | curated normalization + `raw` passthrough | full relational (E2 settled against); verbatim-only (forces every consumer to re-parse) |
| Failure class (b) detector | 6-path required shape (§2 of contract) | key-count heuristics (brittle); full strict schema (class (c) philosophy violated) |
| caster_key tie-break | `name`, ordinal suffix on duplicates | tradition+type composite (unstable across re-exports that swap thesis); index-only (renames break anchoring) |
| Inventory anchor | case-sensitive exact name | case-insensitive fold (false matches between distinct homebrew items) |
| Party provisioning | guarded seed migration | handler auto-create (hidden config); runbook (untestable) |
| Import attempt logging | `audit_events` new kind | tracing-only (not audit); enum reuse (false records) |
| Import UI | minimal functional page in E5 | defer to E6 (E5's story loses its actor; E6's scope grows backward) |
