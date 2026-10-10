# Rules Tooltips + Custom Content Entry (E9) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Player Core condition explains itself in a hoverable/pinnable popup on every surface it renders; the party can create `custom`-lane items, spells, and conditions at three points of use, creator-owned, badged, inert-rendered; the condition picker surface exists; the CUP notice ships and renders in-app.

**Architecture:** One build-time static prose module joined client-side by name; one inert-rendering module every prose path routes through; one `ConditionTip` primitive mounted on existing name surfaces; two party-scoped REST routes writing existing tables (zero migrations); one picker dialog over E8's existing REST and write frames. No engine change, no wire change, no schema change.

**Tech Stack:** existing — Rust/axum + sqlx (two routes + tests through the real router), Svelte 5 runes + hand-rolled CSS, vitest + `@testing-library/svelte` (jsdom, `fireEvent`), Vite JSON import for the seed.

**Spec:** `specs/010-rules-tooltips-custom-content/spec.md` (requirements) · `design.md` (D1–D9) · `contracts/inert-html.md` + `contracts/custom-rows-rest.md` + `data-model.md`. Executors read spec + design + contracts + this plan.

**Lane/heat proposal (for the sizing read, not binding on executors):** Tasks 1–4 (inert core + prose + tip + surface wiring) are Lane 1 — one seat, they share the module and the component. Tasks 5–6 (REST) are Lane 2 — a second seat, pure server, merges clean (touches only `src/`). Tasks 7–9 (points of use + picker) need both lanes' outputs and run after, one seat, with the second seat taking Task 10 in parallel. Task 11 is the convergence gate. Wall-clock ≈ 4–5 heats on two seats; ≈ 7 single-lane. Durgan takes Lane 1 → continuity through 7–9 (E9 is Lane A's tail); Brynn takes Lane 2 → 10.

## Global Constraints

- Branch: `feat/11-rules-tooltips-custom-content` (one issue, one branch, one PR; never `main`). gh#11 is claimed; comment the branch before the first code commit.
- Every commit ends with exactly `Co-Authored-By: Paperclip <noreply@paperclip.ing>` — no agent names.
- **No new dependencies** (Constitution Art. II/V). No sanitizer library, no tooltip library, no JSON-schema crate.
- Reuse, don't rebuild: E6 components (`components.md` is the contract; additive props only), E8's picker REST + effect-create frames, E7's existing writes (slot, inv), `Dialog` primitive, `util/keyboard.js`.
- **Inert rendering is the only prose path** (`contracts/inert-html.md`); `{@html}` banned for prose, boundary-checked.
- Tests drive production paths (AGENTS.md): router tests through the real router; component tests render non-default states and drive `fireEvent`; imported/seeded fields proven with two fixtures; observability assertions read the persisted `audit_events` row through the HTTP path that emitted it.
- `just ci-local` green before the PR; paste output + reachability sweep in the PR body. Grizzly-gate fail-closed; merge ask to Thrane (never Josh).
- The engine is sacred (Constitution Art. IV): this epic writes **zero** engine code. If a task seems to need any, stop and flag — that is a design failure, not an implementation choice.

---

### Task 1: The inert core — **extend** `util/inert-html.js` + boundary rule

**Files:** **Modify** `web/src/lib/util/inert-html.js` — the module **already ships** (E6; `specs/006-live-sheet-ui/spec.md` names E6 the shipper, E9 the consumer) with `parseInert`/`scrub`/`adoptHTML` and **zero production callers** (reachability rule: unbuilt until E9 mounts it). **`adoptHTML` is the setter** — no rename, no second setter (D7). Extend: `scrub`'s discard list (`iframe`, `object`, `embed`, `link`, `meta`, `style` beyond the shipped `script`), attribute filter upgraded to https-or-fragment-only (contract §1), and two new exports `esc` + `linkifyConditions` (contract §2, ported from the prototype's `esc`/`linkConds`). **Extend** `web/tests/util/inert-html.test.js` (7 tests ship — add contract §4's new rows only, keep the 7 untouched); Modify the boundary check to ban `{@html}` outside `ConditionTip`/`AboutView` (find the boundary script; same pattern as the engine-boundary rule).

**Steps:**
- [ ] Port `esc`/`linkifyConditions` verbatim-in-spirit from the frozen prototype; extend `scrub` + `scrubAttributes` per contract §1 — beyond both the prototype and the shipped module, deliberate.
- [ ] Test first: contract §4's new rows only — discard-set fixtures, scheme allowlist (`data:`/protocol-relative/relative dropped; `https:` + fragment survive), `esc`, linkify (text-segment-only, tag passthrough, `skip`, `link:false`). The 7 shipped tests stay untouched. Each new row is a named test.

**Done when:** `npx vitest run web/tests/util/inert-html.test.js` green; `grep -rn "{@html}" web/src` shows zero outside the two sanctioned components; `just ci-local` green.

### Task 2: The curated seed + lookup

**Files:** Create `web/src/lib/rules/condition-prose.json` (42 entries ported verbatim from the prototype's `CONDITIONS` map — paraphrase, `page`, `aonId`, `link:false` for `broken`/`controlled`; `_readme` per data-model §3); Create `web/src/lib/rules/prose.js` (`lookup(name)` → entry | fallback shape; ci-exact match); Create `web/tests/rules/prose.test.js`.

**Steps:**
- [ ] Extract the map from the frozen prototype **as prose only** — no prototype ids, no structured rules data (the constraint is binding; `page`/`aonId`/`text`/`link` are the only fields).
- [ ] Test: lookup finds by case-insensitive exact name ("Frightened"→frightened entry, "off-guard"→entry); two-fixture coverage rule — a fixture corpus list where every seed key matches, and one with a renamed condition, asserting the join (match vs miss → fallback shape, never invented prose).

**Done when:** vitest suite green; a coverage check asserts all 42 keys against the pinned corpus condition-name fixture list (checked in beside the test); no key unmatched.

### Task 3: `ConditionTip` — hover/focus, pin, second layer

**Files:** Create `web/src/lib/sheet/components/ConditionTip.svelte` (trigger wrapper + popup; `Dialog`-adjacent but NOT a modal — port the prototype's layer behavior); Create `web/tests/sheet/condition-tip.test.js`.

**Steps:**
- [ ] Behavior (prototype baseline): `mouseenter`/`focus` opens; `mouseleave`/`blur` closes *unless pinned*; `click`/`Enter` pins; `keydown Escape` and click-outside close; a linkified name inside the popup swaps content in place (second layer, never a third).
- [ ] Renders: prose via `adoptHTML` + `linkifyConditions` (curated only — descriptions are plain text), `Player Core p. N`, AoN anchor (`https://2e.aonprd.com/Conditions.aspx?ID=N`, `rel="noopener"`), tier/lane/`custom` badges, custom condition's optional value as a display note in the tip body, fallback badge for no-prose.
- [ ] Test first (fireEvent, per AGENTS affordance rule): hover→visible with prose+cite+href; click→pinned (survives mouseleave); Escape→closed; focus+Enter pins (keyboard path); hostile prose through the mounted component (matrix row 5 — production path); two fixtures — matching name vs non-matching (fallback) — assert different visible results.

**Done when:** vitest green; keyboard discipline reuses `util/keyboard.js` patterns (focus restore on close).

### Task 4: Wire triggers into existing name surfaces

**Files:** Modify `web/src/lib/sheet/components/EffectsStrip.svelte` (chips become `ConditionTip` triggers — additive, no prop break; E10's roster inherits for free since it composes the same component); Modify `web/src/lib/sheet/components/Provenance.svelte` (breakdown entries become triggers); Create/extend tests in `web/tests/sheet/`.

**Steps:**
- [ ] Test first: render `EffectsStrip` with an effect named `Frightened` → fireEvent mouseenter on the chip → tip visible (production path — the strip, not the tip alone); second fixture: custom-applied condition (description + `custom` badge renders); provenance entry with a condition-named modifier → tip opens.
- [ ] Implement: wrap, don't rewrite — chip markup/DOM stays (E6 audit pins its shape); the tip is an overlay child.

**Done when:** both component suites green through their real render path; existing E6/E10 component tests still green (no prop break); `just ci-local`.

### Task 5: Custom rows REST — create (+ item inventory row)

**Files:** Modify `src/http.rs` (two `API_ROUTES` rows); Create `src/custom/mod.rs` (or extend an existing module home — executor's call per locality); Create `src/tests/custom_rows.rs`; register in the test tree where `engine_rest.rs` registers.

**Steps:**
- [ ] Test first, through the real router: 201 per kind with the row visible via `GET /conditions` (condition) / custom read (all); custom item → inventory row exists for the creator's character **with `qty_delta = 1` asserted** (renders qty 1 — base 0 + 1; the column default 0 would render qty 0 and fail US-3 AC-1); every cap boundary → 400 with `{field, reason}` (`"name" 65`, description 281, rank 11, item-with-value); GM create → 403; no session → 401.
- [ ] Implement `POST /api/parties/{party_id}/custom` per `contracts/custom-rows-rest.md` §1 (transaction: corpus row + item inventory row; validation before write).

**Done when:** integration suite green; `GET /conditions` shows the custom condition with `lane:"custom"`, `tier:"display_only"`, `valued:false` — asserted, not assumed.

### Task 6: Custom rows REST — edit gate + audit + importer isolation

**Files:** extend Task 5's files.

**Steps:**
- [ ] Test first: creator PATCH → 200 + updated row; other member PATCH → 403 **and** an `audit_events` row `forbidden_custom_write`/`denied` (assert the persisted row — observability rule); GM PATCH → 403 + audit; PATCH an imported row → 403 + audit; **E4 corpus-importer** re-run with custom rows present → custom rows byte-identical (FR-8 — run the real corpus import path against the test DB, not the Pathbuilder importer; the pbimport kept-delta notice is expected, §3 of the REST contract).
- [ ] Implement the PATCH gate + audit emission.

**Done when:** suite green; audit assertions read the table, not a log line.

### Task 7: `AddCustomForm` + item context (inventory)

**Files:** Create `web/src/lib/sheet/components/AddCustomForm.svelte` (kind-parameterized minimal fields, caps, inline errors, submit states); Create `web/src/lib/rules/custom-store.js` (fetch + create + optimistic insert; the only module that talks to the custom REST); Modify `web/src/lib/sheet/components/InventoryPanel.svelte` (additive `onaddcustom` affordance in the item context, `editable`-gated); Create `web/src/lib/sheet/SheetView.svelte` wiring (or wherever panels compose — keep locality with E6's structure); tests.

**Steps:**
- [ ] Test first (fireEvent): open form from the inventory affordance; over-cap name → inline error names field+bound, no request (spy at the store boundary); valid submit → optimistic row renders badged `custom`; qty control on the new row issues the ordinary `inv` write by exact name; view-only (`editable:false`) renders no affordance.
- [ ] Implement; server caps re-tested at Task 5 already — client mirrors the same bounds and reasons.

**Done when:** suite green; the affordance is inside the item context per spec US-3, not a global button.

### Task 8: Spell composer entry

**Files:** Modify `web/src/lib/sheet/components/CasterPanel.svelte` (custom-spell section in the spellbook, badged, `Prepare` affordance; `onaddcustom`); `custom-store.js` (spell list); tests in `web/tests/sheet/magic.test.js` (extend — it already pins CasterPanel behavior).

**Steps:**
- [ ] Test first: with a custom spell in the store, the spellbook lists it with `custom` badge and a working Prepare (fireEvent click → `onprepare` called with the name — the existing callback, no new write); prepare dialog lists custom spells at their rank; without custom rows, no section (two fixtures); "Add custom spell" opens the form (Task 7 unit); rank validation 0..10 inline.
- [ ] Implement; prepare writes are unchanged slot writes — assert the wiring, don't touch `sheet/state.js` write paths.

**Done when:** magic suite green; `just ci-local`.

### Task 9: The condition picker surface

**Files:** Create `web/src/lib/sheet/components/ConditionPicker.svelte` (dialog over `GET /api/parties/{id}/conditions`: search, tier/lane/valued badges, value input for valued, apply, custom rows inline, "Add custom condition"); Modify `SheetView.svelte` (owner-gated "Add condition" affordance near `EffectsStrip`); `custom-store.js` (apply via the existing effect-create write the sync layer already exposes — `sheet/state.js` is the only socket module; route through it); tests.

**Steps:**
- [ ] Test first: picker renders corpus rows from the fetched list with badges (two fixtures: `engine_math`/valued vs `display_only` vs `custom` — different badges, different affordances; custom rows show their optional value as a display note); valued condition requires value input, apply issues the existing effect-create call shape (`corpus_entry_id` + `condition_value`); display-only applies without value; GM/view-only never sees the open affordance; custom row applies → chip renders `tracked` (integration with Task 4's trigger is the reachability proof for FR-6).
- [ ] Implement; tooltips on picker rows (Task 3 unit mounted here — production path).

**Done when:** suite green; applying `Frightened 2` through the picker moves the target sheet's numbers via the engine — covered by E8's existing suites; E9's new assertion is only the wiring (call shape + badges).

### Task 10: NOTICE lane + `AboutView`

**Files:** Modify `NOTICE.md` (curated-prose lane, CUP, prototype-freeze provenance — text drafted in `data-model.md` §3); Create `web/src/lib/rules/notice.md` (build-time copy or import) + `web/src/lib/sheet/components/AboutView.svelte` (renders through `adoptHTML`); entry point (app footer/info — executor's call, keep it one link); tests.

**Steps:**
- [ ] Test first: about view renders a known NOTICE line (e.g. the curated-prose lane sentence) through the mounted component; hostile markup fixture in a test copy proves the inert path (production-path rule).
- [ ] Implement.

**Done when:** suite green; `license-verdict` still green (NOTICE structure intact).

### Task 11: Gate + reachability sweep

**Steps:**
- [ ] `just ci-local` → paste exit 0 output in the PR body.
- [ ] Reachability sweep (AGENTS.md): walk spec FR-1..FR-10 and name each one's production-path test in the PR body; grep each new export for a caller (`grep -rn "<name>" src web/src`).
- [ ] PR: `feat/11-rules-tooltips-custom-content` → `Closes #11` + link to the Paperclip task; merge ask to **Thrane** as a task.

**Done when:** gate green, sweep table in the PR body, merge-ask task assigned to Thrane.

---

## Self-review

- Every FR maps to tasks: FR-1 (T3/T4), FR-2 (T2), FR-3 (T1 + every render task), FR-4 (T5/T7/T8/T9), FR-5 (T5/T6), FR-6 (T5/T9 — apply path assertion), FR-7 (T9), FR-8 (T6), FR-9 (T10), FR-10 (T2 by construction + T11 sweep).
- Every guardrail maps: inert setter (T1), caps (T5/T7), read-only curation (T6 403 path), notice (T10), no structured prototype data (T2 steps), reuse (T4/T8/T9 constraints).
- TDD ordering holds: every task's tests are written first and named; no task implements before its failing test exists.
- Engine untouched: no task touches `engine/` or `src/engine_host/apply.rs`; the plan says stop-the-line if that changes.
