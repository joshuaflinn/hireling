# Plan — E6: Live Sheet UI (desktop)

**Consumes:** `spec.md` (rulings §8) + `design.md` (this directory).
**Branch:** `feat/8-live-sheet-ui`, cut from `issue-8-live-sheet-ui` (carries
the spec docs). **Claim gh#8 before any code** (AGENTS.md sync point 1:
self-assign + branch-name comment). Tasks are bite-sized and TDD-ordered:
each writes failing tests first, then code, then its verify command. Final
gate every task: `just ci-local` must be green before the PR opens.

---

## Task 0 — Preflight

- Cut `feat/8-live-sheet-ui` from `issue-8-live-sheet-ui`; push.
- Self-assign gh#8, comment the branch name (sync point 1).
- **Done when:** branch pushed, gh#8 assigned to you with branch comment.
- **Verify:** `git rev-parse --abbrev-ref HEAD` → `feat/8-live-sheet-ui`.

## Task 1 — Spell-economy fields: migration + contract + write path

**Goal:** the Q1 ruling's three vitals fields sync end-to-end (design §3).

1. Failing tests first, mirroring `src/tests/sync/write.rs` style: valid
   write applies + bumps version; bounds reject (`focus_current < 0`,
   `staff_charge_rank > 10`, non-object `daily`); CAS supersede path;
   snapshot carries the fields.
2. Migration `20261002000002_vitals_spell_economy(.down).sql` per design §3;
   migration up/down test in `src/tests/migrations.rs` style.
3. Extend the vitals write/validate path for `focus_current`, `hero_points`,
   `daily` (value shapes per design §3 table).
4. Update `specs/007-party-sync/contracts/wire-protocol.md` §3's vitals row —
   the sanctioned extension PR content rides this branch.
- **Done when:** all new sync tests green; wire-protocol §3 lists the three
  fields with value shapes.
- **Verify:** `cargo test sync` and `cargo test migrations` green.

## Task 2 — Bootstrap item-bulk map

**Goal:** bootstrap payload carries `item_bulk` for the character's imported
names (design §5).

1. Failing test: bootstrap for the seeded character returns exact-name
   (case-insensitive) bulk lookups from the corpus; missing names → `null`.
2. Implement server-side resolution at bootstrap build; log misses
   (structured, name + character).
- **Done when:** test green; misses logged.
- **Verify:** `cargo test bootstrap` (or the router test module containing it).

## Task 3 — Web foundations: inert HTML + dialog utils

**Goal:** the two safety/discipline utilities every component rides.

1. Failing tests: `inert-html.js` parses via DOMParser, adopts nodes, never
   executes `<script>`/`onerror` payloads; `keyboard.js` traps focus in a
   container, Escape closes, Enter commits, focus restores on close.
2. Implement both, porting the prototype's `setHTML` discipline.
- **Verify:** `cd web && npx vitest run tests/util`.

## Task 4 — Engine seam: adapter + base-only derivation

**Goal:** `lib/engine/` per design §4 — `derive(character, liveState)`
returning the engine-output-contract shape, base-only mode.

1. Failing tests against `tests/data/pb_export_reference.json` transformed to
   `base_sheet` (reuse E5's transform output as fixture): AC from `acTotal`
   parts; each save; Perception; every skill + lore; strike rows with MAP
   −5/−10 (agile −4/−8); `hp_max` at levels 1/3/20; cantrip/focus rank =
   ceil(level/2); `level_adjust` re-derives prof bonus/HP/DCs/rank (§4 spec);
   `effect_chips: []`, provenance = base parts.
2. Implement `index.js` (interface + mode switch) and `base.js`; `format.js`
   (signed numbers, bulk text: `3 Bulk + 2 L`) with its own tests.
- **Verify:** `cd web && npx vitest run tests/engine`.

## Task 5 — Sheet state layer over the sync stack

**Goal:** `sheet/state.js` — the only module that talks to `createSync`
(design §2 data flow).

1. Failing tests with E7's sync fakes (`web/tests/sync/fakes.js`): bootstrap
   → derived stores populated; write → echo pending → `applied` settles;
   `superseded` silently reverts; `rejected`/`forbidden` surfaces an op
   record for inline display; `offline` flips the affordance store (reads
   stay); New Day burst = used slots → false + focus → 0 + daily → zeroed,
   enqueued FIFO; syncing store = queue non-empty.
2. Implement. Components get zero knowledge of the socket.
- **Verify:** `cd web && npx vitest run tests/sheet`.

## Task 6 — Shell, header, states, CSS pass 1

**Goal:** the app frame a user lands in.

1. `App.svelte` view switch (import | sheet); sheet route renders
   `EmptyState` (designed import CTA) when no character; skeletons while
   bootstrap loads; calm error with retry where one exists.
2. `CharacterHeader` (name, ancestry/class line, level display + adjust
   control slot, `SyncIndicator`, New Day button); port the prototype's
   design tokens + three-column grid into `app.css` (design §8).
3. Component tests: empty/loading renders; SyncIndicator visible iff
   syncing store true.
- **Verify:** `cd web && npx vitest run tests/shell` + `npm run build` clean.

## Task 7 — Stats pane

**Goal:** spec §2.2 rendered. `HpBar` (clamp 0–max, temp segment absorbs
first, ±1/±5/Full, temp input, client validation before write), `StatTile`
grid (AC/saves/Perception/Speed/DCs/Size from adapter), `FocusPips` +
hero pips (Task 1 fields), attributes, `SkillList`/`SkillRow` with rank
letters + popups (base provenance), meta lines. All `editable`-aware.
Component tests: HP clamp/temp-order/pending tint; pips write correct field.
- **Verify:** `cd web && npx vitest run tests/sheet` (stats suite).

## Task 8 — Level adjust control

**Goal:** spec §4. Header control, bounds 1–20 (level + `level_adjust`),
confirm dialog on level-down (`Dialog`, not `window.confirm`), writes
`vitals.level_adjust`, derived re-derivation visibly re-renders (adapter
output changes), boost/feat immovability shown by not offering it.
Tests: bounds reject locally; down-confirm path; re-derive assertion at
fixture deltas.
- **Verify:** `cd web && npx vitest run tests/sheet` (level suite).

## Task 9 — Magic pane: spells

**Goal:** spec §2.3. `CasterPanel` per caster (header pills from adapter),
`SpellSlotRow` (cast toggle → `slot.used`; prep via keyboard `Dialog` picker
→ `slot.prepared`; empty-slot affordance), cantrips at heightened rank,
innate rows (locked), spellbook collapsible, reset-to-export prep,
curriculum-slot display, Staff Nexus panel (charge-rank select + charge
pips → `daily`), Drain Bonded Item toggle, focus cantrips section,
`FocusPips` if caster has focus. New Day button (header) wired to Task 5's
burst. Tests: cast/prep writes hit fake sync with right targets; staff
charge math; used-state render.
- **Verify:** `cd web && npx vitest run tests/sheet` (spells suite).

## Task 10 — Companions panel

**Goal:** spec §2.4. Pet/familiar stats from `base_sheet.companions` via
adapter (HP 5×level, owner AC/saves, 3+level skills line, abilities list,
command note). No summons browser, no minion tracking (parked — Q4).
Tests: render from fixture companion; absent companion → clean empty note.
- **Verify:** `cd web && npx vitest run tests/sheet` (companions suite).

## Task 11 — Inventory pane

**Goal:** spec §2.5 + design §5. `ContainerGroup`s from `base_sheet` with
Bulk rollups (tenths math, extradimensional containers excluded and labeled),
`item_bulk` from bootstrap (null → "—"), qty edit → `inv.qty_delta` (qty 0 =
removed, design decision 6), `CoinBar` → `vitals.money` absolute write, tag
chips from corpus traits. No container editing (parked — Q2). Tests: rollup
math incl. extradimensional skip + L/number/— handling; qty write path;
money write path.
- **Verify:** `cd web && npx vitest run tests/sheet` (inventory suite).

## Task 12 — Strikes + feats panes

**Goal:** spec §2.6–2.7. `StrikeRow`s from adapter (name, atk, MAP row,
damage, traits — trait *names* only; text popups are E9). `FeatsPanel`:
feats level-grouped names + features tab (class features, ancestry &
heritage) from `base_sheet`. No detail prose (parked). Tests: strike render
at fixture; feats grouping.
- **Verify:** `cd web && npx vitest run tests/sheet` (strikes/feats suites).

## Task 13 — E10 reuse contract pass

**Goal:** design §6 made reviewable. Audit every component for `editable`
behavior (view-only renders no controls); write `specs/006-live-sheet-ui/components.md`
listing each reusable unit with props/events and its E10 consumer (roster
card = HpBar + EffectsStrip + SyncBadge; drill-in = SheetView editable={owner}).
- **Verify:** doc exists; audit checklist all-green; full `cd web && npx vitest run`.

## Task 14 — Final gate + PR

- `just ci-local` green, paste output in PR body.
- PR `feat/8-live-sheet-ui` → `main`: `Closes #8`, link the Paperclip task,
  decisions with rejected alternatives in the body (AGENTS rule 5), scope
  table fence restated.
- Grizzly-gate must pass; an owner merges. Orsik reviews independently.
- **Verify:** `just ci-local` exit 0, evidence pasted.

---

**Out of scope (bounce with owner named):** tooltip/condition prose + custom
entry forms (E9) · stash/bank (E12) · spell outcome templates (E13, Dave) ·
effect math (E8) · mobile layout (P2 gh#18) · notes/container-editing/
summons-bestiary/layout-resize/save-templates (parked, scope table).
