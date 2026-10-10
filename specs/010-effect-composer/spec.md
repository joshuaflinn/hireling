# Feature Specification: Freeform Effect Composer UI (E8-residual)

**Epic**: E8-residual — Phase 1, P0 · GitHub issue #74 · depends on E7 (sync) + E8 (engine, write frames) + E10 (read path) · feeds E13 (#15)
**Created**: 2026-10-10
**Status**: Specify gate folded into the task (MOR-87 review approved the slot; the
contracts below are already merged — this spec pins the write surface, it re-decides
nothing the engine epics settled).
**Input**: PRD v3.6 "Core Experience" Step 3 + FG3 + Party Screens (GM view); GitHub
issue #74 (Josh's scope sketch); specs/008 contracts (binding); specs/007
wire-protocol.md §8 (E8's extension, realized); `engine/src/vocab.rs` (the closed
vocabulary, one source).

---

## Position

E8 shipped the engine, the `effect_new` create frame and the `effect` update/end frames,
and the read faces; E10 shipped the read path (chips, provenance hovers). The PRD's P0
composer — *Bear taps "new effect" → names it Bless → freeform via the picker → selects
targets → both sheets recompute with provenance* — has no surface. This spec is the
write surface: client plumbing over the existing frames, and the dialog on the sheet.

**Zero backend contract changes.** Every wire shape, bound, and denial reason below is
read off the merged tree (`src/sync/write.rs`, `src/sync/protocol.rs`,
`engine/src/vocab.rs`, `src/engine_host/rest.rs`). If an implementation task discovers
it needs a backend change, that is a different task with a different reviewer — stop and
flag it.

## User Story — Bear casts Bless at the table (Priority: P0) 🎯 MVP

Bear opens his sheet, taps **New effect**, names it *Bless*, picks `+1 status → attack`,
notes "10 rounds", checks Josh and Becky in the target picker, and applies. Both sheets'
attack numbers rise on every connected device without reload, each changed number shows
its provenance (already wired by E6/E10 — this spec builds none of it). Mid-fight Bear
removes Josh from the effect's targets and Josh's sheet reverts; when the blessing ends,
Bear ends the effect and every affected number returns to base.

**Independent Test**: Mount the sheet with a session sync stub and the reference engine
output; drive the composer with `fireEvent` (open → fill → apply); assert the sync
surface received the exact `effect_new` create op. Drive a `rejected` op event through
the same surface; assert the composer shows the server's reason. Two differing engine
output fixtures must yield two differing stat pickers — a hardcoded stat list cannot
pass.

### Acceptance Scenarios

1. **Given** an owner-player viewing their own sheet, **When** they open the composer
   and apply a complete form, **Then** the sync surface emits a write op targeting
   `{"kind":"effect_new","party_id":N}` with value `{op:"create", name,
   source_character_id, targets, modifiers, duration_note}` — field-for-field the
   server's create shape.
2. **Given** the stat picker, **When** it renders, **Then** every option is derived from
   the character's delivered engine output (the `GET /api/characters/{id}/derived` body,
   which the socket delivers identically) — global singles from `derived`, the
   attack/damage family from `strikes`, the spell family from `casters`, `skill:<name>`
   per `skills`. No client-side stat constant exists.
3. **Given** the server denies the op (bounds, ownership, roster), **When** the
   `rejected`/`forbidden` event lands, **Then** the composer renders the server's
   human-readable reason inline and keeps the form; it is never swallowed.
4. **Given** an applied ack, **When** it lands, **Then** the dialog closes and the
   chips/numbers move through the existing read path (no composer-owned reconciliation —
   the store and `derived` frames already own it).
5. **Given** the creator's own active effects (REST face
   `GET /api/parties/{party_id}/effects`, rows carrying `effect_id` + `version`),
   **When** they remove a target or end the effect, **Then** the sync surface emits the
   `effect`-targeted `{op:"update", targets}` / `{op:"end"}` op with
   `base_version` = the fetched row's version (FR-14 whole-row CAS).
6. **Given** the GM seat (or any non-owner drill-in), **When** the sheet renders,
   **Then** no composer entry point exists — PRD: "Bruce's account never renders an
   edit control."

## Requirements

- **FR-C1 — Entry point**: A "New effect" affordance on the sheet beside the effect
  strip, rendered only for an editable sheet (owner + player, the E6 `editable`
  discipline) with a party id and a delivered engine output (the vocabulary source).
  Absent any of the three, the affordance is absent — no disabled theatre.
- **FR-C2 — The four fields** (PRD, exactly): name; modifier picker (stat → type →
  value, one or more rows — the create op carries an ordered array); duration note
  (display-only text, never enforced — Constitution non-goal); target picker from the
  party roster.
- **FR-C3 — Sourced vocabulary**: stat options derive from the delivered engine output
  (FR acceptance 2). The modifier **type** options are the closed four
  (`circumstance|status|item|untyped`, `engine/src/vocab.rs` `MODIFIER_TYPES`, pinned by
  engine tests — cited in the UI code; there is no read face that enumerates types, and
  the closed set is settled engine data).
- **FR-C4 — Apply path**: the composer writes through the sheet's state layer — the
  only module that talks to the sync surface (E6's rule, unchanged). Client-side bounds
  mirror the server's table exactly (name 1..=120 after trim; ≥1 target, distinct
  positive ids; ≤16 modifiers; integer value −50..=50); invalid input never leaves the
  layer.
- **FR-C5 — Denial and settlement**: the state layer maps `rejected`/`forbidden` events
  for effect targets to an inline reason (server text verbatim) and `applied` to close.
  Supersession stays silent per the degraded-mode contract; the manager refetches truth
  on open.
- **FR-C6 — Manager face**: the composer dialog lists the creator's own active effects
  with current targets; per-target remove composes the whole remaining target set into
  one `update` op; End composes `end`. Both address `{kind:"effect","effect_id":N}` with
  CAS `base_version` from the REST rows. A lost race surfaces on the next fetch — the
  list is refetched after every op.
- **FR-C7 — Seam for E13**: the composer is a component over the state layer's effect
  ops; the seeded-spell tapper composes the same ops. Nothing tapper-shaped is built
  here; no conflict pre-warn in the target picker (E13 owns both).

## Settled contracts this spec builds on (change none)

- Create: `effect_new` target, value keys `op|name|source_character_id|targets|modifiers|
  duration_note` (+ the corpus pair, not used by the freeform composer) —
  `src/sync/write.rs` `apply_effect_create` / `validate_effect_value(.., "create")`.
- Retarget/end: `effect` target, keys `op|targets` / `op` alone, CAS on the row version —
  `apply_effect_mutation`.
- Engine output (the vocabulary body): `specs/008-buff-effect-engine/contracts/
  engine-output.md` §3 — served verbatim by `GET /api/characters/{id}/derived` and by the
  socket's snapshot/`derived` frames.
- Party effects (manager rows): `GET /api/parties/{party_id}/effects` —
  `engine_host::rest::effects`, rows carry `effect_id` + `version`.

## Scope decision recorded: blanket targets

The engine's three blanket stats (`all_checks`, `all_dcs`, `all_checks_and_dcs`) are not
enumerable from any merged read face, and this spec forbids backend changes — so the P0
freeform picker offers no blanket rows. Blanket-shaped outcomes ride the corpus condition
picker (`GET /api/parties/{party_id}/conditions` → valued conditions), which is FR-8's
surface for exactly that. If the table needs freeform blankets, the fix is a small
vocabulary read face — a backend addition, out of this task by its own rule.

## Out of scope (binding)

Seeded-spell outcome tapper; conflict pre-warning in the target picker (both E13); corpus
condition authoring (E4's picker already reads); auto-expiry, rounds, range — Constitution
non-goals; any change to `src/` or `engine/`.

## Test matrix (house rule — the affordance lives in the wiring)

All at the mounted-component level through the production paths (`render()` +
`fireEvent`, jsdom): the SheetView wiring (entry point → dialog → composer → state layer
→ sync stub records the op); two differing engine-output fixtures against the stat
picker; a denial event driven through the sync surface's subscription; the manager's
End/remove against recorded CAS ops; GM seat renders no entry point. A green store unit
test is not evidence here.
