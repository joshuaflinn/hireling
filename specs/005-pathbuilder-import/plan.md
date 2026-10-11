# Pathbuilder Import Pipeline (E5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Paste/upload a Pathbuilder 2e export; the character derives from it, is owned by the importer, and re-imports preserve all live table state by exact anchors.

**Architecture:** Pure parse→validate→transform→anchor core in `src/pbimport/` (thin shell / pure core per `docs/toolkit-conventions.md`), writing only E2's landed tables plus two raised migrations (party seed, audit CHECK — gate-approved before you start). One axum endpoint plus a self-scoped read; a minimal Svelte import page.

**Tech Stack:** Rust (axum, sqlx, serde_json — all in-tree), Svelte/Vite scaffold (no new deps).

**Spec:** `specs/005-pathbuilder-import/spec.md` (requirements) · `design.md` (decisions) · `data-model.md` (shapes) · `contracts/pb-export.md` (the captured input contract — wire code builds against this, never prose memory).

## Global Constraints

- Exact failure-class message strings from `contracts/pb-export.md` §4 — asserted verbatim in tests. Wording changes are spec changes.
- Caps: SIZE = 1,048,576 bytes; DEPTH = 64. Server-side always (FR-3).
- E2's schema is untouchable except the two raised migrations (`design.md` §5). Zero new tables/columns.
- Active effects: never queried, never written (FR-14). `characters.owner_sub` UNIQUE is the one-character-per-account law (FR-8).
- Anchors: vitals by character identity; slots `(caster_key, rank, slot_index)` with `caster_key = name` else `name#n` on duplicates; inventory by case-sensitive exact name (FR-10).
- Unknown fields: logged + skipped + counted; import continues (FR-6). Never stored in `base_sheet`.
- New routes go in `API_ROUTES` (`src/http.rs`) with explicit `writes: true` — the E3 ownership-matrix suite iterates that table; a write route without authz coverage fails the suite.
- Branch: this branch (`issue-7-pathbuilder-import`). Claim gh#7 (self-assign) before the first code commit; PR carries `Closes #7` + Paperclip link. `just ci-local` green before PR. Commit messages end with `Co-Authored-By: Paperclip <noreply@paperclip.ing>`.
- No `#[allow]` around lints; propose tuning in the PR instead (AGENTS.md).

---

### Task 0: Fixture extraction (contract capture)

**Files:**
- Create: `tests/data/pb_export_reference.json`
- Test: `src/pbimport/tests/fixtures.rs`

**Interfaces:**
- Produces: `pub(crate) fn reference_export() -> String` (the fixture, verbatim) used by every later test module.

- [ ] **Step 1: Extract the fixture.** Parse `docs/reference/lorum_ipsum_dashboard.html`, pull the `#pbExport` script block's JSON **byte-verbatim** (do not reformat), write to `tests/data/pb_export_reference.json`. Record byte length in the test (assert 5,493).
- [ ] **Step 2: Failing test** — `reference_export()` parses as valid JSON with `success == true`, `build.name == "Lorum Ipsum"`, 39 `build` keys, 2 `spellCasters`.
- [ ] **Step 3: Run** `cargo test -p hireling pbimport::tests::fixtures` — FAIL (module absent).
- [ ] **Step 4: Implement** the module (read the file via `include_str!` or a relative-path reader following the house test-convention — check how E4's tests load `data/seed/`).
- [ ] **Step 5: Run** — PASS. **Step 6: Commit** `test: E5 — extract frozen pbExport fixture as importer test data (#7)`.

### Task 1: Caps (FR-3)

**Files:** Create `src/pbimport/caps.rs`, `src/pbimport/tests/caps.rs` · Register module in `src/pbimport/mod.rs` (create) + `src/lib.rs`.

**Interfaces:** Produces `pub const MAX_BODY_BYTES: usize = 1_048_576;`, `pub const MAX_DEPTH: usize = 64;`, `pub fn check_size(len: usize) -> Result<(), ImportError>`, `pub fn check_depth(value: &serde_json::Value) -> Result<(), ImportError>`; `ImportError::PayloadTooLarge | PayloadTooDeep` carrying the §4 codes+messages.

- [ ] Write failing tests: 1 MiB + 1 byte → `payload-too-large` (413); a 70-deep Value → `payload-too-deep` (400); boundary values pass. Depth counts objects/arrays, scalars are depth 1 (fixture is 8 — test it).
- [ ] Run → FAIL. Implement (depth = recursive walk with an early `depth > MAX_DEPTH` bail; no recursion-unbounded panics — iterative or bounded).
- [ ] Run → PASS. Commit `feat: E5 — server-side size/depth caps (#7)`.

### Task 2: Envelope validation + failure classes (a)/(b) + class (c) walker (FR-4/5/6)

**Files:** Create `src/pbimport/model.rs`, `src/pbimport/tests/model.rs`.

**Interfaces:** Consumes Task 1 errors. Produces `pub fn parse_and_validate(body: &str) -> Result<ValidExport, ImportError>` (class a: serde error; class b: the §2 six-path rule) and `pub fn unknown_fields(export: &ValidExport) -> Vec<String>` (dotted key paths, nested).

- [ ] Failing tests, each asserting code + **exact message string**: truncated JSON; `{"hello":"world"}`; `{"success":false,"build":{…}}`; missing each of the six required paths (one test per path); `level: 0` and `level: 21`; abilities missing `wis`; martial export (`spellCasters` absent, required paths present) **passes**; fixture + injected `build.futureField` and nested `build.attributes.newThing` → walker returns both paths, validation passes.
- [ ] Run → FAIL. Implement. Run → PASS.
- [ ] Commit `feat: E5 — envelope validation, failure classes, unknown-field walker (#7)`.

### Task 3: Transform — `BaseSheet` (FR-1, design §1, data-model §2)

**Files:** Create `src/pbimport/transform.rs`, `src/pbimport/tests/transform.rs`.

**Interfaces:** Consumes `ValidExport`. Produces `pub struct BaseSheet` (serde to the §2 jsonb shape) and `pub fn transform(export: &ValidExport) -> (BaseSheet, SectionSkips)`; `SectionSkips` lists type-drifted sections for the diff (`contract` §5).

- [ ] Failing tests against fixture ground truth (cite `contracts/pb-export.md` §3): identity verbatim; six ability scores; `hp.max_hp == 14` (8+6+0+0×2); `ac.acTotal == 18`; 2 spellcasters, `caster_key`s `["Wizard", "Wellspring Gnome"]`, `per_day[0..2] == [6,4,3]`; prepared rank-1 list starts `"500 Toads"`; 16 equipment entries — `Bedroll.container == "Backpack"`, `Lock (Simple).container == "Giant body's sack"`; 2 containers, the sack `extradimensional: true`; money `{cp:4,sp:2,gp:24,pp:0}`; familiar `Pippin` present; `raw.feats[0][0] == "Charming Liar"`. Plus: duplicate caster names → `["Wizard", "Wizard#2"]`; unresolved container UUID → `container: null` + skip notice; `equipment` as objects → section skipped into diff, everything else imports.
- [ ] Run → FAIL. Implement. Run → PASS.
- [ ] Commit `feat: E5 — export→base_sheet transform, pure (#7)`.

### Task 4: Anchoring core (FR-10–13, design §4)

**Files:** Create `src/pbimport/anchor.rs`, `src/pbimport/tests/anchor.rs`.

**Interfaces:** Consumes `BaseSheet::slot_layout()` + existing live-row snapshots (`SlotRow {caster_key, rank, slot_index, used, prepared_spell}`, `ItemDelta {name, qty_delta}`). Produces `pub fn anchor(layout: &[(String,i64,i64)], export_prepared: &PreparedMap, live_slots: &[SlotRow], live_items: &[ItemDelta], old_max_hp: i64, new_max_hp: i64) -> AnchorPlan` where `AnchorPlan { seeds: Vec<SlotSeed>, diff: Diff, unchanged_rows: usize }`. `Diff` per data-model §5.

- [ ] Failing tests — the full matrix: all matched (empty diff); vanished caster; shrunk rank (kept+diffed); grown rank (seeded+diffed); duplicate-name tie-break stability across re-imports; innate block seeds; prep divergence (live wins, no mutation); idempotency (same layout twice ⇒ zero seeds, empty diff); vanished item kept; negative effective qty noticed; `max_hp_changed` notice when old≠new.
- [ ] Run → FAIL. Implement. Run → PASS.
- [ ] Commit `feat: E5 — re-import anchoring + post-import diff, pure (#7)`.

### Task 5: Raised migrations (design §5 — gate-approved before start)

**Files:** Create `migrations/20260924000008_poc_party_seed.sql` + `.down.sql`, `migrations/20260924000009_audit_import_event.sql` + `.down.sql`.

**Interfaces:** Produces: exactly one POC party row when `parties` is empty (idempotent re-run); `audit_events.event` CHECK admitting `character_import`. Down: seed row removed only if still `name = '<seed>'` and referenced by no character; CHECK restored to E3's seven.

- [ ] Integration tests (compose Postgres, follow `src/tests/db.rs` harness): up → one party; up again → still one; insert `character_import` audit row OK; garbage event still rejected; down → party gone (when unoccupied), CHECK back to seven.
- [ ] Run `just db && cargo test -p hireling --features … migrations` (use the house integration pattern). FAIL → implement → PASS.
- [ ] Commit `feat: E5 — POC party seed + character_import audit kind (raised findings) (#7)`.

### Task 6: Store + orchestration transaction (FR-7/8/9/12, design §4)

**Files:** Create `src/pbimport/store.rs`, `src/pbimport/tests/store.rs` (integration).

**Interfaces:** Consumes Tasks 3–4. Produces `pub async fn run_import(pool: &PgPool, actor: &Actor, body: &str) -> Result<ImportOutcome, ImportError>` — the whole flow: caps→parse→validate→transform→load-by-owner→(first: insert+seed / re: anchor+apply)→audit→`ImportOutcome {character, diff, advisory}`.

- [ ] Failing integration tests: first import creates character owned by actor in the POC party, `payload_raw` byte-identical to body, vitals seeded (hp=14, money from export), all layout slots materialized with export prep; second import replaces base_sheet (rename visible) and changes zero vitals/slot/inventory rows; concurrent double-import (two tasks, same account) → one character, both succeed serially; six accounts → six characters; audit row per attempt with correct outcome/target.
- [ ] Run → FAIL. Implement (single transaction; `SELECT … FOR UPDATE` on the character row; `sqlx::migrate!` picks up Task 5). Run → PASS.
- [ ] Commit `feat: E5 — import store + transactional orchestration (#7)`.

### Task 7: HTTP surface (FR-15/17, design §3)

**Files:** Create `src/pbimport/handlers.rs`; Modify `src/http.rs` (route table + mounts), `src/pbimport/tests/http.rs` (integration + router matrix).

**Interfaces:** Consumes Task 6. Produces `POST /api/characters/import` and `GET /api/characters/me` inside the protected nest (E3 layers apply); error responses use E3's standard envelope; success envelope per data-model §5 + `advisory.skipped_fields`.

- [ ] Failing tests: unauthenticated POST → 401 before validation; GM POST → 403 + `forbidden_gm_write` audit (middleware) — handler never runs; player POST fixture → 200 with `{character, diff, advisory}`; each failure class through HTTP asserts status+code+exact message; `GET /api/characters/me` returns own character, 204 when none; ownership-matrix suite now iterates the two new `API_ROUTES` entries.
- [ ] Run → FAIL. Implement. Run → PASS.
- [ ] Commit `feat: E5 — import + me endpoints behind the ownership matrix (#7)`.

### Task 8: Minimal import page (FR-18)

**Files:** Create `web/src/lib/import/ImportPage.svelte` (+ submit/diff logic in a testable module per the scaffold's `web/src/lib` pattern); Modify `web/src/App.svelte` (route to it).

**Interfaces:** Consumes Task 7's HTTP contract. Produces: paste textarea + file input (reads file to text, same POST), submit → result render (character summary, diff sections, skipped-field count, exact error messages).

- [ ] `just web-test`: body assembly (paste and file paths produce identical POST bodies), error render maps `code → message`, diff render covers all §5 kinds. `just web-check` clean.
- [ ] Implement → green. Commit `feat: E5 — minimal import page (paste/upload + diff) (#7)`.

### Task 9: End-to-end verification + gate

- [ ] Full drill via API: import fixture as player A → mutate live state (direct SQL or future-path stub: set hp, spend a slot, prep a swap, add inventory delta) → re-import modified export → assert preservation + diff (SC-3).
- [ ] `just ci-local` green end-to-end (fmt, clippy -D warnings, unit, integration w/ compose Postgres, cargo-deny, svelte-check) — paste the output in the PR body.
- [ ] Push, open PR: body carries `Closes #7`, Paperclip task link, decisions-with-rejected-alternatives summary (from design §8), the two raised migrations called out, and the ci-local evidence. Request Orsik's independent review via the Paperclip flow; merge ask goes to Thrane.

## Self-review checklist (before PR)

- [ ] Every exact-message test asserts the full string (grep the tests for the quotes).
- [ ] No `base_sheet` write path touches vitals/slots/inventory on re-import (assert in store tests: row versions unchanged).
- [ ] No code path queries the effects tables (grep `effects` in `src/pbimport/` — expect zero hits).
- [ ] `payload_raw` never re-serialized (store test compares bytes).
