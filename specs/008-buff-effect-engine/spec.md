# Feature Specification: Buff/Effect Engine (E8)

**Epic**: E8 — Phase 1, sync point, P0 · depends on E4 (corpus) + E5 (import) + E7 (sync) · blocks E10, E13 · GitHub issue #10
**Created**: 2026-10-01
**Status**: Specify gate accepted (2026-10-01, card `8d4b455d`); clarify answers folded same day — Q1 per-instance stats, Q2 core+lores skill set, Q3 server-side recompute (card `af988c85`)
**Input**: `docs/EPICS.md` Epic E8 (specify prompt + Constraints + AI Guardrails); PRD v3.6 FG3 + Key Risks + Non-goals; E2 `specs/002-database-schema/data-model.md` §4 (effects schema, landed); E4 `src/import/seed.rs` + `data/seed/condition-tiers.json` (tier/modifier seed, landed) + `specs/004-rules-corpus-importer/`; E5 `specs/005-pathbuilder-import/data-model.md` (`base_sheet` shape, landed); E7 `specs/007-party-sync/contracts/wire-protocol.md` §3/§8 (effect read-only now; write frames are E8's extension point)

---

## User Scenarios & Testing *(mandatory)*

E8's users are the six party accounts at the table — casters who apply
effects, players whose sheets move, the GM who watches — plus three
downstream epics that consume its output: E6 (renders the numbers and the
provenance hover), E10 (party cards render the same engine state), E13
(conflict pre-warning reads live engine state). E8 ships one internal
surface (the engine module + effect endpoints + the wire extension), not a
screen; like E7, its "users" include the epics that build on its contract.

### User Story 1 — The caster applies a buff and the table's math is done (Priority: P1) 🎯 MVP

Bear casts Bless and picks Becky and Jake as targets. Both sheets' attack
and spell-attack numbers rise immediately — on every connected device, no
refresh — and every changed number shows its math on hover: `Strike +15 =
+14 base +1 status (Bless, from Bear)`. Durgan's sheet (not a target) does
not move. When Bear ends the effect, every affected number returns to base,
again everywhere at once.

**Why this priority**: This is US-6 and US-3 in the PRD — the product's core
promise. Everything else in the epic (stacking, provenance, corpus tiers)
exists so this story stays true when the math gets weird.

**Independent Test**: Open two clients on one party; apply an effect with a
`+1 status → attack` modifier to one character via the effect API; assert
(a) that character's derived attack total rose by 1 on both clients without
reload, (b) the provenance breakdown names the effect and its source,
(c) a non-targeted character's derived stats are byte-identical before and
after, (d) ending the effect restores the exact prior state.

**Acceptance Scenarios**:

1. **Given** an active party session, **When** a member creates an effect
   targeting one or more roster characters, **Then** every connected client
   receives the change and every derived stat on every affected sheet
   recomputes — no client reload, no user action.
2. **Given** any derived stat touched by the change, **When** the sheet
   renders it, **Then** it renders from engine output (never a
   client-side second computation) and carries its full provenance
   breakdown.
3. **Given** an effect with modifiers, **When** it is applied, **Then**
   only its targets' sheets change; a blanket-target effect changes exactly
   its expansion set on each target's sheet, nothing else.
4. **Given** an active effect, **When** the creator ends it, **Then** the
   effect is retained (ended, not deleted) and all affected derived stats
   recompute to their correct new values.

### User Story 2 — Every number explains itself, including the losers (Priority: P1)

Becky is under both Bless (+1 status to attack) and Inspire Courage
(+1 status to attack). Her attack shows +1, not +2 — and the hover says
why: `+1 status (Bless) — Inspire Courage +1 also active, not stacked`.
The suppressed source is a first-class output of the engine, not a UI
guess: what suppressed what, and by which rule, is engine truth.

**Why this priority**: US-3 — "what does that do again?" is the friction
tax this product exists to kill. Suppression is the case players get wrong
most, so the engine must emit it, not infer it.

**Independent Test**: Apply two same-type bonuses to the same stat via the
API; assert the engine output's `applied` list carries the winner and the
`suppressed` list carries the loser with the stacking reason and the
suppressing effect's id; assert the sheet-visible total equals base +
winner only.

**Acceptance Scenarios**:

1. **Given** two active bonuses of the same type on one stat, **When** the
   engine computes, **Then** only the highest applies once; the loser is
   emitted as a suppressed entry naming the winning effect.
2. **Given** a bonus and a penalty of the same type on one stat, **When**
   the engine computes, **Then** both apply (highest bonus and worst
   penalty separately), each visible in provenance.
3. **Given** untyped modifiers on one stat, **When** the engine computes,
   **Then** all apply (bonuses and penalties each stack fully).
4. **Given** any derived number with zero active modifiers, **When**
   rendered, **Then** its breakdown shows base only.

### User Story 3 — Detrimental conditions ride the same rails (Priority: P1)

The table applies Frightened 2 to Becky from the condition picker. Her
attack rolls, spell attacks, saves, Perception, every skill, AC, and DCs
all drop by exactly 2 — and her damage and speed do not move, because
*frightened* is a status penalty to `all_checks_and_dcs`, and that
blanket's expansion set is data the engine expands, not prose a developer
interpreted. Applying Concealed changes no number at all — it lands as a
badge, because the corpus says display-only.

**Why this priority**: One mechanism, both directions (PRD). The corpus —
not engine code — decides which conditions carry math; the PRD made that
E4 data precisely so this engine has zero names hardcoded.

**Independent Test**: Apply a valued engine-math condition at value N via
the API; assert every stat in the mapping's expansion set moved by exactly
the polarity-scaled value and no other stat changed; apply a display-only
condition and assert zero numeric change plus a tracked-manually badge flag
in the effect's chip data.

**Acceptance Scenarios**:

1. **Given** a corpus condition of tier `engine_math`, **When** applied at
   value V, **Then** its mapping rows (constant or `condition_value` ×
   polarity) become modifiers exactly as the corpus data states.
2. **Given** a corpus condition of tier `display_only`, **When** applied,
   **Then** it creates a visible effect chip with the tracked-manually
   badge and zero engine math — the engine never fabricates modifiers.
3. **Given** the condition picker, **When** listing conditions, **Then**
   the tier and value expectations come from the imported corpus rows, not
   from any list in engine or UI code.

### User Story 4 — The engine is provably right (Priority: P1)

Every stacking rule the engine encodes is asserted against the Player
Core's own worked examples — named, ordered, exhaustive — and the expansion
sets and stacking permutations are hammered by property-based tests. An
engine bug is a stop-the-line event (Constitution Article IV), and the test
suite is the tripwire that makes stopping early cheap.

**Why this priority**: The PRD names this suite as *the* mitigation for the
product's top key risk (modifier-engine edge cases). It is not garnish; it
is the risk register's answer.

**Independent Test**: `cargo test` runs the worked-example suite and the
property suite; both are wired into `just ci-local`; mutating any stacking
rule (e.g., letting same-type bonuses stack) fails at least one named
worked-example test.

**Acceptance Scenarios**:

1. **Given** the worked-example suite, **When** it runs, **Then** every
   rule in the Stacking Math table (FR-3) has at least one named test
   asserting the Player Core outcome.
2. **Given** the property suite, **When** it runs, **Then** expansion-set
   membership and stacking outcomes hold for generated permutations of
   modifier lists (including empty, singleton, and adversarial mixes).
3. **Given** a derived total for any stat under any modifier combination,
   **When** recomputed from scratch, **Then** the result is identical —
   the engine is a pure function of (base stats, active effects), no
   incremental drift is possible.

### User Story 5 — The caster commands their effects (Priority: P2)

Bear's composer view shows every effect he created, live: name, modifiers,
duration note, current targets. He adds Durgan to Bless mid-fight; Durgan's
sheet moves. He removes a fleeing target; that sheet reverts. The GM sees
effects on every sheet but writes none of them.

**Why this priority**: Manual lifecycle is the settled ownership model
(creator is sole writer, even on other sheets); the visibility surfaces are
thin but they are the caster's whole management UI at P0.

**Independent Test**: As the creator, list own active effects (assert
targets + modifiers round-trip); add/remove a target and assert only the
delta broadcast; as GM, attempt an effect write and assert a 403 with E3's
standard payload.

**Acceptance Scenarios**:

1. **Given** an effect's creator, **When** they add or remove a target,
   **Then** the whole-effect version bumps once and the target sheets'
   derived stats recompute accordingly.
2. **Given** any party member, **When** viewing a sheet, **Then** the sheet
   shows the effects affecting that character with their sources.
3. **Given** the GM account, **When** attempting any effect write, **Then**
   the server rejects it (deny-by-default per E3) even though no UI offers
   it.

---

## The Stat Vocabulary (closed, enumerated — FR-2's data)

Single stats (11) and blanket targets (3), exactly as the PRD settled and
as `src/import/seed.rs` already validates:

| Stat | Meaning (display surface) |
|---|---|
| `ac` | Armor Class |
| `fort` / `ref` / `will` | the three saves |
| `perception` | Perception modifier |
| `speed` | land Speed (feet) |
| `attack` | every attack roll (per strike — Q1: **per instance**) |
| `damage` | every damage roll (per strike — Q1: **per instance**) |
| `spell_attack` | spell attack modifier (per caster block — Q1: **per instance**) |
| `spell_dc` | spell DC (per caster block — Q1: **per instance**) |
| `class_dc` | class DC |
| `skill:<name>` | one per skill in the skill set (Q2: **core skills + the character's lores**) |
| `all_checks` | blanket — see below |
| `all_dcs` | blanket — see below |
| `all_checks_and_dcs` | blanket — see below |

**Expansion sets — data, not prose.** A blanket target expands, before
stacking is evaluated, into exactly:

```jsonc
{
  "all_checks": [
    "attack", "spell_attack",
    "fort", "ref", "will", "perception",
    "skill:<name>"   // every skill in the character's set (Q2: core skills + lores),
                     // and per-strike/per-caster instances (Q1)
  ],                  // NOT damage, NOT speed
  "all_dcs": ["ac", "class_dc", "spell_dc"],  // AC is a DC (Player Core)
  "all_checks_and_dcs": [ /* the union of the two sets above */ ]
}
```

The expansion is a function of the character's stat instances (Q1/Q2 settled):
core skills are present on every sheet; a character's lores join their set;
strikes and caster blocks each contribute an instance. A modifier naming a
stat with no instance on that sheet (a lore the character lacks) matches
nothing — no total changes, nothing is emitted for it; the write path warns
at apply time.

*frightened*'s footprint is `all_checks_and_dcs` — that is the canonical
test case (WEx-5). A modifier addressed to a blanket target is never
evaluated against a blanket; it is expanded at evaluation time into its
member stats and stacks per stat individually.

## Stacking Math (the whole rules engine — FR-3) with the named worked examples

Per single stat, over all active modifiers that expanded to it:

1. Bonuses of a type (`circumstance`, `status`, `item`): **the highest
   applies once**; other same-type bonuses are suppressed.
2. Penalties of a type: **the worst applies once**; other same-type
   penalties are suppressed. Bonuses and penalties of the same type are
   resolved separately and both apply.
3. `untyped`: **stacks fully** — every untyped bonus and every untyped
   penalty applies.
4. Blanket targets expand **before** stacking is evaluated (a `−2 status
   all_checks_and_dcs` and a `−1 status ac` are two status penalties on
   `ac`; the worst −2 applies, the −1 is suppressed — WEx-9).
5. Ties (two equal same-type bonuses): one applies (the total is
   unaffected); which one shows as applied is deterministic (stable
   ordering by effect id), and the other is emitted suppressed with the
   tie reason.

**The Player Core worked examples the tests assert against** (test names
WEx-1…; each encodes the named general rule or condition; the plan orders
them as unit tests, paraphrase-cited per the Community Use notice):

| # | Player Core rule (source) | Asserted outcome |
|---|---|---|
| WEx-1 | Bonuses (general rule): same type doesn't stack, take the highest | +2 status and +1 status to `ac` → +2; +1 suppressed |
| WEx-2 | Bonuses: different types stack | +1 status and +2 circumstance to `ac` → +3, both applied |
| WEx-3 | Penalties (general rule): same type, take the worst | −2 status and −1 status to `will` → −2; −1 suppressed |
| WEx-4 | Penalties vs bonuses of same type: both apply | +2 status and −1 status to `attack` → net +1; both listed applied |
| WEx-5 | Frightened (condition): status penalty equal to value to all checks and DCs | frightened 2 → −2 on every `all_checks_and_dcs` member, **not** `damage`, **not** `speed` |
| WEx-6 | Bless vs Inspire Courage (the PRD's own example): two +1 status bonuses to attack | +1 total; loser emitted suppressed with winner named |
| WEx-7 | Untyped bonuses stack fully | +1 untyped and +2 untyped to `damage` → +3 |
| WEx-8 | Untyped penalties stack fully | −1 untyped and −2 untyped to `ac` → −3 |
| WEx-9 | Blanket expands before evaluation (frightened + a direct AC status penalty) | `−2 status all_checks_and_dcs` + `−1 status ac` → worst status penalty (−2) applies, −1 suppressed |
| WEx-10 | Item bonuses: highest once (armor potency vs another item bonus to AC) | +1 item and +2 item to `ac` → +2; +1 suppressed |
| WEx-11 | AC is a DC (Player Core) | an `all_dcs` modifier moves `ac`, `class_dc`, `spell_dc` and nothing else |
| WEx-12 | Valued condition sign is stored data (E4 seed `polarity`) | frightened 1..4 → −1..−4 status, never a bonus |

## Edge Cases

- **Blanket + specific, same type**: two same-type penalties from different
  effects (one blanket-expanded, one direct) compete per stat — WEx-9.
- **Zero-value modifier**: a resolved +0 modifier changes no total but still
  appears in provenance (applied, +0).
- **Two effects with the same display name**: identity is the effect id
  (E2 PK), never the name; provenance disambiguates by id + source.
- **Suppressed tie (equal values)**: deterministic winner by stable order
  (effect id); math identical either way, output is not.
- **Effect on a character later removed from the roster**: the *link* dies
  (E2 CASCADE on `effect_targets`); the effect row survives; remaining
  targets keep their math; a now-target-less active effect is legal and
  recomputes nothing.
- **Creator's character removed while their effects exist**: blocked by E2
  RESTRICT — a human decides first (end or reassign). No silent path.
- **Concurrent changes to two different effects**: each write commits
  whole-effect (E7 CAS); recompute is a pure function of current active
  effects, so interleavings converge — there is no incremental state to
  corrupt. The same guarantee covers a recompute racing a snapshot request.
- **Re-import while effects are active**: effects are live state, untouched
  (E5's anchoring law); the engine recomputes against the *new*
  `base_sheet` — derived numbers follow the new base. Max-HP clamp
  interplay is E6's display concern per E5's data model.
- **Display-only condition applied mid-buff**: badge lands, zero numeric
  change, zero recomputation of anything (nothing it touches exists).
- A modifier naming a skill the sheet lacks (a lore the character doesn't
  have — Q2): no stat instance exists, so no total changes and nothing is
  emitted; the write path warns at apply time (folded from the clarify
  ruling; see FR-2).
- **GM writes any effect path**: denied server-side (E3), UI-invisible is
  not the enforcement.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1 — module boundary (the business goal, made reviewable)**: The
  engine MUST be a module (or crate) with zero imports of UI, transport,
  HTTP, WebSocket, or database machinery — inputs are plain data (base
  stats, active effects, condition mappings), output is plain data (the
  engine-output contract). The RPGMastermind-harvest criterion: **a port
  to another host is a compile of this module plus new glue, never a
  rewrite** — reviewers verify by dependency inspection, not aspiration.
- **FR-2 — the closed vocabulary**: The engine MUST accept exactly the
  stat vocabulary above (11 single stats, 3 blanket targets,
  `skill:<name>` for the skill set of Q2) and MUST reject any other stat
  string loudly (validation error at the boundary — API/composer and
  corpus-mapping load — never a silent drop). The expansion sets MUST be
  defined as data (a table in code consumed by the expander and emitted to
  the contract), not as per-stat special cases.
- **FR-3 — stacking math**: The engine MUST implement the five stacking
  rules exactly as tabled above. Every applied and suppressed modifier
  MUST be attributable to (effect id, modifier, rule) — the stacking
  decision is output, not just arithmetic.
- **FR-4 — pure recomputation**: `engine(base_stats, active_effects,
  condition_mappings) → engine_output` MUST be a pure function: no I/O,
  no wall-clock, no randomness (ordering rules are deterministic), no
  mutable state. Derived numbers MUST NOT be stored as editable state
  anywhere (E2 settled); recomputation is total on every trigger.
- **FR-5 — the recomputation trigger and blast radius**: Any committed
  effect change (create; target add/remove; modifier change; end;
  hard-delete escape hatch) MUST recompute every derived stat on every
  **affected** sheet, where affected = (targets removed ∪ targets added)
  for that change — a superset is always correct, a subset never. The
  recompute rides E7's broadcast path (Q3 settled: **server-side
  recompute**): effects version as a whole (E7 §3), the effect diff fans
  out, and each affected character's `EngineOutput` is computed in the
  same commit and broadcast with it — clients render and never compute;
  the catch-up snapshot carries derived output for every character. No
  partial recompute exists — the engine runs over the full active set.
  Derived output is never stored as editable state (E2 settled).
- **FR-6 — provenance output**: For every derived stat, the engine MUST
  emit: base value, every applied modifier (type, value, effect id/name,
  source character), total, **and every suppressed modifier with the
  reason and the suppressing effect**. The shape is the binding contract
  `specs/008-buff-effect-engine/contracts/engine-output.md` — the single
  source both E6 and E10 render from; no consumer invents a second shape.
- **FR-7 — manual lifecycle**: The engine epic MUST expose effect
  create/retarget/end (and target remove) as authorized writes: the
  effect's creator is the sole writer (E3 enforcement, deny-by-default);
  duration is a displayed note; **nothing auto-expires, ever** (no timers,
  no range checks — non-goal guard). Ended effects MUST be retained
  (`active=false`), per E2.
- **FR-8 — corpus-driven conditions**: The condition picker MUST read
  corpus rows (`corpus_entries`, kind `condition`): tier from
  `data.import.tier`, mappings from the `modifiers` jsonb (absent/empty ⇒
  display-only ⇒ badge chip with `tracked_manually`, zero math). Valued
  conditions resolve `condition_value` mappings as
  `value × polarity` at apply time and store the resolved signed modifiers
  on the effect. **Zero condition names appear in engine code** — a
  review criterion (grep + review), not a hope.
- **FR-9 — wire extension (E7 §8)**: Effect writes MUST enter through the
  party WebSocket as `kind:"effect"` frames with whole-effect CAS
  semantics, idempotent by op id, exactly-once fan-out — extending
  `specs/007-party-sync/contracts/wire-protocol.md` via PR to that file
  (its §8 named this epic). Reserved close codes 4001+ are used only as
  that contract's amendment defines.
- **FR-10 — the test suite is the risk mitigation**: The engine MUST ship
  (a) the named worked-example suite (WEx-1…12, each citing its Player
  Core rule), (b) property-based tests for expansion-set membership and
  stacking permutations, (c) determinism tests (same inputs ⇒ identical
  output, including provenance order). All MUST run in `just ci-local`.
  **Engine bugs are stop-the-line (Constitution Article IV)** — the plan's
  test ordering encodes this: the suite is built first and fails red
  before implementation begins.

### Key Entities

- **Stat** — one of the closed vocabulary (single, `skill:<name>`, blanket).
- **Modifier** — `{type, stat, value}`; type ∈ circumstance|status|item|untyped; negative = penalty.
- **Active effect** — E2's effect row + targets + ordered modifiers + `active`.
- **Base stats** — per-character plain data extracted from `base_sheet` +
  live `level_adjust` (extraction mapping is a design deliverable).
- **Condition mapping** — E4 corpus data: tier, `[{modifier_type, stat,
  value_kind, value?, polarity?}]` per condition.
- **Engine output** — derived totals + provenance (applied + suppressed) +
  effect chip data; pinned in `contracts/engine-output.md`.
- **Suppressed entry** — a modifier that lost stacking: which rule, which winner.

### Constraints (settled by the epic — non-negotiable)

- The engine recomputes; it never stores derived numbers as editable state (PRD FG3, EPICS E8).
- Engine module clean of UI and transport — RPGMastermind harvest is a port, not a rewrite (stated business goal).
- Engine bugs are stop-the-line (Constitution Article IV in repo house rules).
- Effect model is `{name, source_character, targets[], modifiers[], duration_note, active}`; targets are roster characters only (companions/minions tracked manually — PRD).
- Stacking rules, the vocabulary, and the expansion sets are settled PRD decisions — this spec enumerates, it does not re-decide.
- E7's wire protocol, versioning, and ownership enforcement are landed contracts this epic extends, not re-designs.
- Engine bugs and test ordering carry into the plan (FR-10); `just ci-local` green before any PR; grizzly-gate fail-closed; an owner merges.

### Success Criteria

- **SC-1**: The worked-example suite (WEx-1…12) and the property suite run
  green in `just ci-local` and CI; deliberately breaking any stacking rule
  fails a named test (mutation-checked at review).
- **SC-2**: The engine module builds with zero UI/transport/DB/framework
  dependencies (verifiable by `cargo tree`/module boundary inspection) —
  the port-not-rewrite criterion.
- **SC-3**: E6's implementer can render the sheet's derived numbers,
  provenance hover, and effect chips entirely from
  `specs/008-buff-effect-engine/contracts/engine-output.md` (and E10 the
  same) — reviewer-verified at those epics, same as E7's SC-5 precedent.
- **SC-4**: Provenance completeness — for any derived stat, applied +
  suppressed entries account for every active modifier that expanded to
  it (property-tested; nothing silently vanishes).
- **SC-5**: Zero condition-name special cases in engine code (grep clean +
  review); vocabulary enforcement lives in one place, shared with E4's
  seed validation.

### Assumptions

- E6 is specced in parallel (Lane A) against the same
  `contracts/engine-output.md` — the seam is one contract, two consumers.
- The corpus seed currently carries one engine-math condition (Frightened)
  and one display-only (Concealed); growing it is curation (PR to
  `data/seed/condition-tiers.json`), not E8 scope. The engine is
  data-complete regardless of seed size.
- Base-stat extraction from `base_sheet` (which fields feed each stat's
  base, incl. strike/caster/skill instances — the export carries ranks,
  not totals, for saves/skills/perception/spell stats) is a design-step
  deliverable pinned in the contract's input section.
- Offline/degraded behavior for effect writes follows E7's landed contract
  unchanged — E8 adds a field kind, not a new failure model.

### Out of scope (guarded)

Conflict pre-warning and seeded spell library — E13 (P1). Ability-scoped
condition vocabulary (clumsy/enfeebled-style Str/Dex scoping) — engine-v2
conversation, explicit PRD non-goal for POC. Tooltip prose — E9. Sheet and
party-card rendering — E6/E10. Auto-expiry, auras, positioning, round
tracking — non-goals (Constitution Article I); a duration is a note, a
range is a human.
