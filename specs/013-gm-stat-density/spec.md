# Feature Specification: GM Stat Density — Party-Card Stat Blocks (E14)

**Epic**: E14 — Phase 3, P1 · depends on E10 (closed) · GitHub issue #16
**Created**: 2026-10-14 · **Revised**: 2026-10-14 — four review rounds on PR #197 addressed in place (round 1: 6B/3M/2L — class-DC rescoped to `class_dc.base`, sheet alignment tasked, sweep re-scoped, SC-8 added; round 2: 1B/2M/1L — SC-5 exact-set assertion, honest blind-spot list, `spell_attack.total` row; round 3: 1B/2M/2L — `class_dc.total` + `casters → []` mutation rows, directional-pattern wording, revision stamps; round 4: 2B/0M/1L — `fort`/`ref`/`will`/`perception` rows, visible-text label mechanism, `Math.min`). Full changelog below.
**Status**: Specify gate run 2026-10-14 (Korrin, front half of the SDD pipeline — docs only, no code in this pass); design signed by Korrin same day. **Implementation hold (Thrane, binding): no E14 code merges to `main` before gh#13 closes — spec, design and plan on disk are fine.**
**Input**: `docs/EPICS.md` Epic E14 (specify prompt + AI Guardrails); PRD v3.6 FG5 ("GM stat density (P1)") + Non-Goals (Combat tracking) + US-9; `CONSTITUTION.md` Articles I/II/VI; E8 `specs/008-buff-effect-engine/contracts/engine-output.md` (binding — the derived shape the stat block reads); E10 `specs/009-party-view-gm-seat-pwa/` (the surface this epic extends); merged tree: `web/src/lib/party/state.js` (the per-card `view`), `web/src/lib/sheet/` (the sheet's own read of the same view), `engine/src/vocab.rs` (the closed stat vocabulary — no `initiative` in it).

---

## Position

E10 shipped the party roster: one card per character — name, portrait initial,
HP bar with down/max state, effect chips. The PRD's P1 step (FG5) extends each
card into a full glanceable **stat block**: everything a GM needs to calibrate
an encounter without tapping into a sheet. All read-only, all rendered from
the engine-recomputed values the sheet already renders.

This epic adds **no engine stat, no wire shape, no backend surface**. Every
number it displays already exists in the `EngineOutput` the roster state
already holds per card (`web/src/lib/party/state.js` — the `views` map fed by
`sync.derived(characterId)`). If an implementation task discovers it needs a
backend or engine change, that is a different task with a different reviewer —
stop and flag it.

The two decisions the PRD deliberately deferred here — the key-skills
selection rule and the testable form of the no-second-computation-path
guardrail — are **settled in this spec** (FR-2, FR-3), not handed back.

## User Story — Bruce calibrates without tapping (Priority: P1) 🎯

Bruce (GM) glances at the party screen mid-session. Each card now carries the
character's combat-relevant numbers: current/max HP (the existing bar), AC,
Fort/Ref/Will, Perception, Initiative modifier, spell/class DCs where the
character has them, and the three skills the character is actually best at.
When Bear's Bless lands on Josh, Josh's attack-adjacent numbers move on
Bruce's screen without a reload; when frightened 2 lands, the saves and key
skills drop by 2 where the rule says they do — because the card renders the
engine's recomputed values, not its own arithmetic. Nothing on the card asks
Bruce to do anything; the tap target is still exactly one: the drill-in.

**Why this priority**: US-9 and PRD FG5's decided ruling — the GM seat is
read-only and unburdened; density is how it earns its keep. This is the P1
payoff of the whole engine: one glance, full picture, zero clicks.

**Independent Test**: Mount the roster with a seeded engine-output fixture;
assert each card renders AC, the three saves, Perception, Initiative,
spell/class DC (or its absence), and exactly three key skills with their
modifiers, by visible label text. Mutate the fixture (an applied effect moves
`ac.total`, a skill total drops) and re-assert: the card tracks the fixture
exactly, key skills re-select. A second computation path cannot pass this.

### Acceptance Scenarios

1. **Given** a party character with a delivered engine output, **When** any
   account views the party screen, **Then** that character's card renders a
   stat block: current/max HP (existing bar), AC, Fort, Ref, Will,
   Perception, Initiative modifier, and three key skills with modifiers —
   each value labeled and readable without tapping into the sheet.
2. **Given** a character with at least one non-innate caster block, **When**
   the card renders, **Then** the stat block shows the same spell DC the
   sheet's StatsPane shows (first non-innate caster, else first caster),
   with the spell attack modifier as its note; a character with no casters
   shows no spell-DC entry.
3. **Given** a character whose `class_dc.base` is `null` on the wire (the
   `class_dc` object is always present — E8 contract §3; only `base` and
   `total` go null), **When** the card renders, **Then** no class-DC entry
   renders (the slot's output is null — nothing is invented), and the
   sheet's StatsPane likewise omits its tile for the same character — a
   character with a class DC shows it on both surfaces.
4. **Given** any effect change that recomputes a character's engine output
   (buff, condition, level adjust), **When** the recompute lands on the
   wire, **Then** the card's stat block reflects the new totals without a
   page reload — same WS fan-out, same store, as the existing HP bar.
5. **Given** a card whose engine output is not on the wire yet (bootstrap
   before first snapshot), **When** the card renders, **Then** the stat
   block shows its skeleton state — no invented numbers, mirroring the
   HP-bar skeleton the card already renders.
6. **Given** the GM seat, **When** the stat block renders, **Then** it adds
   zero write affordances: the card remains one navigation control, and the
   stat block adds no control of its own — its only focusable elements are
   the sheet's own read-only provenance hovers (FR-1, same affordance both
   surfaces carry).

## Functional Requirements

### FR-1 — Stat block contents (the card's numbers)

Each roster card extends, below the existing HP bar and effects strip, with a
compact stat block rendering, each entry's label rendered as visible text
(D7: `StatTile`'s `<div class="k">` — the mechanism SC-1's queries read):

- **AC** — `derived.ac.total`, plain number.
- **Fort / Ref / Will** — `derived.fort.total`, `derived.ref.total`,
  `derived.will.total`, signed (`+9` / `−1`), same formatting helper as the
  sheet.
- **Perception** — `derived.perception.total`, signed.
- **Initiative modifier** — see FR-4.
- **Spell DC** — `derived.casters` selected by the sheet's own rule (first
  non-innate block, else the first block; `StatsPane`'s existing
  `$derived`), value `spell_dc.total`, note `attack <signed
  spell_attack.total>`. Absent when `casters` is empty.
- **Class DC** — `derived.class_dc.total`; the entry is **omitted** when
  the output's `class_dc.base` is null (E8 contract §1: null base ⇒ not
  rendered; `class_dc` itself is always an object — contract §3).
  **Sheet alignment (merged-tree defect, fixed by this epic's
  implementation — plan Task 1b)**: StatsPane currently renders the tile
  unconditionally with a `—` placeholder (`StatsPane.svelte:112`), against
  contract §1 — which made sheet-equality (FR-3 statement 4) unsatisfiable
  for classless characters: spec, plan and merged code could not all hold.
  The implementation renders the sheet's tile conditionally too: null base
  ⇒ no tile on either surface. The dash carries no information a null-base
  stat can stand behind — its provenance hover renders nothing (modifiers
  to a null base are suppressed from display by the same contract line),
  so the tile shows a bare `—` for a stat the character does not have.
- **Key skills** — FR-2, exactly three rows: display name + signed total.

HP current/max stays the existing `HpBar` (it already renders `current / max`
text); the stat block does not duplicate it. Every entry reuses E6's
`StatTile`-style presentation including the provenance hover (applied /
suppressed verbatim from the engine output) — the GM can hover a number and
see its math, same as a player can on the sheet. **The hover is exercised
in its non-default state (SC-8)**: the reference fixture's `applied` lists
are empty on every stat, and `Provenance` renders nothing when both lists
are empty (`Provenance.svelte` gates on `count > 0`) — so a fixture with a
non-empty `applied` list is required to prove the affordance renders at
all, on both surfaces.

### FR-2 — Key skills: the selection rule (settled here, as the PRD deferred)

**Selection set**: every row of `derived.skills` — core skills and lores both
(E8 contract Q2 settled the wire's set; this spec ranks all of it). No
pre-filter by proficiency.

**Ranking** (a total order, deterministic on the wire data alone):

1. `total` descending — the modifier is the number; the top three totals win.
2. Equal `total` ⇒ higher `rank` wins (the character's trained investment
   beats a coincidental ability bonus).
3. Equal `total` and `rank` ⇒ canonical `name` ascending (lexicographic on
   the wire key, `lore:` prefix included — e.g. `athletics` before `thievery`,
   `lore:underworld` before `lore:warfare`).

**Consequences, stated so a reviewer can check them**:

- An **untrained** row (rank 0) **can** win a slot when its `total` is
  genuinely in the top three. The engine's proficiency arithmetic already
  encodes the untrained cost in the wire's `total` (a rank-0 row carries no
  proficiency bonus there), so in practice trained rows dominate after the
  earliest levels — but this rule is honest arithmetic over the delivered
  totals: filter nothing, rank what is. The comparator reads totals; it
  re-encodes no book math, so it carries no book citation.
- **Lores compete with core skills on equal terms.** A loremaster's best
  number is usually a Lore; excluding it would misreport "three highest
  skill modifiers."
- **Two seats render the same three, in the same order** — the comparator's
  inputs (`total`, `rank`, `name`) are engine-output fields, identical on
  every device; no timestamp, insertion order, or locale collation enters it.
- **Selection re-runs on every recompute.** A targeted skill modifier (an
  effect keyed `skill:<name>`) can move a row into or out of the top three;
  the deterministic rule keeps every seat in agreement as it happens.
- Fewer than three rows in `derived.skills` (degenerate — the extractor
  always emits the full core set): render what exists; never pad.

**Display**: the row shows the skill's display name and signed total. Display
name follows the sheet's convention: lores render as `<label> Lore` (the
wire's `label`, with the key-derived fallback E6's partition already owns);
core rows capitalize the canonical key. Rank letters do not render on the
card (density; the sheet carries them).

**Rejected alternatives** (recorded per Constitution Article II):

- *Trained-only candidates* — misreports the character's actual best rolls
  and reintroduces a judgment the numbers should make. The PRD said "three
  highest skill modifiers", not "three highest trained skill modifiers."
- *Fixed candidate list* (core skills minus lores) — same fault, plus it
  re-encodes rules data (a list) that the wire already carries.
- *Name-first tie-break* — alphabetical order has no investment signal;
  at equal totals the trained row is the better glanceable fact.

### FR-3 — One source of numbers; no second computation path (the guardrail, testable)

**The named derivation**: the stat block renders the card's engine-output
view — the `views` map entry `sync.derived(characterId)` that
`web/src/lib/party/state.js` already maintains — exactly as `SheetView`
renders `sync.derived(characterId)` through its own store
(`web/src/lib/sheet/state.js`). One engine, one wire, two views.

**Checkable statements** — each names the mechanism that checks it; the
implementation PR must show every mechanism (a statement with no mechanism
is decoration, and round 1 said so):

1. **Verbatim read** — *mechanism: diff review at the implementation PR*.
   Every stat-block number is a field of that view read as-is —
   `derived.<stat>.total`, `derived.casters[…].spell_dc.total`,
   `render_base.hp_max` (the bar) — or a vitals field store (the bar's
   current HP). Formatting (`signed`) is display, not computation. The
   reviewer's checklist at the PR: every rendered value traces to a named
   view field; any value that does not is a second path.
2. **Selection ≠ derivation** — *mechanism: a scoped sweep plus
   line-by-line diff review*. No E14 code combines, sums, offsets or
   clamps engine numbers. The sweep's scope is exact: `web/src/lib/party/`
   **plus this epic's own engine-lib helpers** (`key-skills.js`,
   `caster.js` — D4/D5 put new code in `web/src/lib/engine/`, which already
   holds pre-existing sheet-path arithmetic in `format.js:28,36`; that
   file predates E14, is outside this diff, and is named here so the
   sweep's claim stays exact). The sweep is a tripwire and its pattern is
   directional, not exhaustive — it catches `<view-field> <operator>`
   shapes (bare, spaced or no-space) plus literal `reduce(`/`Math.max`/
   `Math.min`. Its blind spots *include* right-hand operands
   (`bonus + view.ac.total`), helper calls (`sum(view.fort.total, 2)`),
   wrapped fields (`Number(view.ac.total) - Number(view.ref.total)`),
   template concatenation and aliasing — illustrative, not closed. The
   pattern is tuned to the diff at Task 5 and the compensating
   evidence is the diff review plus statements 3 and 4.
   Comparing and sorting wire totals (FR-2) is selection, not computation.
3. **Fixture tracking** — *mechanism: a component test*. Mounts the card
   against an engine fixture and mutates **every FR-1 entry's backing
   field, one at a time** — an applied buff moves `ac.total`; each of
   `fort.total`, `ref.total`, `will.total` moves (round 4 B2 — the rows
   plan and design carried since round 1 while citing this table as
   authoritative); `perception.total` moves (the Init entry moves with
   it); a skill total
   drops; `casters[0].spell_dc.total` moves; `casters[0].spell_attack.total`
   moves (the attack note moves with it); `class_dc.total` moves (the
   displayed value — the `base`-null absence test alone cannot catch a
   hardcoded Class DC, round 3 B1); `class_dc.base` goes null and
   the entry disappears; `casters` empties (`[]`) and the spell-DC entry
   and its attack note disappear (FR-1's absence — round 3 B1);
   `hp_max` moves the bar — asserting the card
   tracks each mutation exactly, every displayed value and absence
   included. A hardcoded constant, a cached recompute,
   or a second math path cannot pass.
4. **Sheet equality** — *mechanism: a component test*. The same fixture
   drives the sheet's `StatsPane` and the card's stat block; AC, saves,
   Perception, spell DC and class DC render identical values in both —
   **including absence**: a null `class_dc.base` omits the entry on the
   card *and* the tile on the sheet (FR-1's alignment makes this
   unconditional; against the merged sheet's unconditional dash it was
   unsatisfiable). One engine source, two views — the E10 chip-equality
   rule, extended to numbers.

### FR-4 — Initiative modifier display (and the non-goal, flagged)

The stat block shows an **Initiative** entry whose value is
`derived.perception.total`. Its authority is the PRD's own ruling —
`docs/PRD.md:41-42`: "displaying an initiative *modifier* is a stat, not
tracking — that's fine" — and the closed vocabulary below forces the
source: Perception is the only initiative-adjacent number the engine
recomputes. (PF2e's Perception-by-default encounter rule is why the `Init`
label reads honestly; a page-level *Player Core* citation for it lands in
the implementation PR's doc comment on the Init tile, written off the
source page per AGENTS.md rules-exactness §1 — this spec cites only what
it can read.) The entry's provenance
hover is Perception's own (`perception.applied` / `.suppressed`), because
that is the math behind the number.

This is honest by construction: the engine's closed vocabulary has no
`initiative` stat (`engine/src/vocab.rs` `SingleStat` — and the sync layer
loudly rejects writes targeting it, tested in `src/tests/sync/effects.rs`),
and the Pathbuilder import carries no separate initiative field. The only
single-path display of "initiative modifier" is the recomputed Perception
modifier. Labeling it `Init` is display, not derivation.

**Non-goal, explicit** (PRD Non-Goals, misread before): initiative **ORDER**,
round counters, and turn tracking are out of scope forever. This epic
displays a modifier — a stat, data on a card. It does not sort the party,
highlight a turn, or count anything.

### FR-5 — Read-only, every seat, by construction

The stat block renders for every viewer of the party screen — GM and players
alike (it is the shared card surface; FG5's name is about the GM's need).
It adds no write affordance anywhere: the card remains a single `<button>`
(the drill-in), the stat block's entries carry no control of their own —
their only focusable elements are E6's read-only provenance hovers, the
same `role="note"` affordance the sheet's tiles already carry (FR-1,
SC-5) — and the GM
seat's zero-affordance posture (E10's acceptance) is unchanged.

### FR-6 — Offline and loading states

The stat block renders from whatever the roster state holds: cached engine
output offline (PWA, same as the HP bar), skeleton when no output has
arrived yet. No entry renders a placeholder number — the honest gap, same
rule as the card's existing skeleton bar.

## Success Criteria

- **SC-1** Every stat FR-1 names is visible on each party card without
  drill-in, each entry's label rendered as visible text (D7), for a
  character with a delivered engine
  output. *(Bench-verifiable at the implementation PR: component test
  locating each entry by its visible label text.)*
- **SC-2** Two characters with different engine outputs render different
  stat blocks — every displayed value moves with its fixture field,
  enumerated one at a time by FR-3 statement 3's mutation table, absence
  states included (two-fixture rule). *(Bench-verifiable.)*
- **SC-3** Card stat values equal the sheet's values for the same character
  state, including under an active effect (FR-3 statement 4). *(Bench-verifiable.)*
- **SC-4** The key-skills tie-break fixtures each render deterministically:
  equal total / different rank → higher rank wins; equal total and rank /
  different names → name ascending; three-way tie → exact predicted order;
  and the selection re-runs when a fixture's totals change. *(Bench-verifiable,
  boundary-style assertions per the two-fixture rule.)*
- **SC-5** The stat block region's focusable set is exactly E6's
  read-only provenance affordances — it contains no control of its own and
  nothing that mutates state.
  *(Bench-verifiable with a **scoped** query, on SC-8's fixture. Two traps
  the assertion must not fall into. First, a card-level button count: the
  effects strip already nests `role="button"` ConditionTip spans inside
  the card's drill-in button — `RosterCard.svelte` mounts `EffectsStrip`
  inside `<button>` — so it fails on any card with effects and guards
  nothing. Second, an empty-region "no interactive role and no `tabindex`"
  claim (round 2, B1): FR-1 mandates the provenance hover on every entry,
  `Provenance.svelte` emits `tabindex="0" role="note"` whenever
  `applied`/`suppressed` is non-empty and nests a `role="button"`
  ConditionTip — so that claim only ever passes on an empty-`applied`
  fixture and is jointly unsatisfiable with SC-8, which requires the
  non-empty state. The test therefore mounts the card **on SC-8's
  non-empty-`applied` fixture, with effects present**, locates the stat
  block's own region — StatBlock's root is a labelled group, D7 — and
  asserts: every focusable or interactive element inside it is E6's own
  provenance affordance (a `role="note"` span or the ConditionTip
  nested in one), and the region holds no `<button>`, no form control, no
  control outside those affordances. The card-level drill-in assertion
  stays, as the FR-5 posture check it actually proves.)*
- **SC-6** FR-3 statement 2's sweep passes over its exact scope —
  `web/src/lib/party/` plus this epic's new engine-lib helpers — with the
  command and output pasted in the implementation PR, the pattern's blind
  spots named next to it, and the compensating diff review stated. The grep
  alone cannot prove absence (the pattern is directional — blind spots
  *include* right-hand operands, helper calls, wrapped fields, template
  concatenation and aliasing, illustrative not closed — and whatever the
  tuned pattern leaves open is named next to the pasted output; the plan's
  Task 5 list and this one say the same thing); statements 3 and 4 are the
  criteria
  with teeth, and the PR body says so rather than laundering the tripwire
  as proof. *(Bench-verifiable as stated: sweep + review, honestly
  labelled.)*
- **SC-7** A stat block on a card whose engine output has not arrived
  renders skeleton entries, no numbers. *(Bench-verifiable.)*
- **SC-8** The provenance hover renders its non-default state: a fixture
  with a non-empty `applied` list shows the breakdown on the card's tile
  and the sheet's tile for the same stat (engine text verbatim —
  `signed(value) type — effect_name`, the strings `Provenance.svelte`
  already renders); the empty-`applied` fixture renders no such affordance
  on either surface. *(Bench-verifiable: two-fixture, both surfaces — the
  non-default-state rule; the reference fixture is empty on every stat, so
  a single-fixture test never enters the branch the rule is about.)*

## Edge Cases

- **Character with no casters** — no spell-DC entry; the block shortens.
- **Character with `class_dc.base: null`** — no class-DC entry on the card
  and, after FR-1's alignment, no tile on the sheet (never "0", never
  "—"; absence is absence).
- **Innate-only caster** (sorcerer with innate block only) — the sheet's
  fallback picks the first block; the card shows the same DC the sheet
  shows (equality, SC-3, holds over the fallback too).
- **Innate block ordered first** (`casters = [innate, non-innate]`) — the
  one ordering where "first non-innate" and "first block" disagree. Both
  views pick the non-innate block, and the fixtures pin exactly this side
  of the boundary: the reference fixture's casters are non-innate-first
  (`[Wizard, Wellspring Gnome]`), an ordering on which the two rules
  return the same block and a degraded extraction passes silently — the
  discriminating fixture is required, not optional (plan Task 1).
- **Tie among lores and core at equal totals** — FR-2's comparator decides;
  lore-vs-core identity never enters the comparison (the wire names do).
- **A skill named identically to a lore key** — impossible on the wire
  (core names never carry the `lore:` prefix; canonical keys are unique per
  E8's contract).
- **Empty party / empty card set** — unchanged from E10; nothing here
  touches the empty state.
- **Fixture swap mid-glance** (re-import while effects active) — the engine
  recomputes against the new base (E8 edge case); the card simply renders
  the new output, including a possibly different key-skills set.
- **Offline render** — cached view; numbers from the last delivered output,
  clearly the same data the sheet shows offline.

## Assumptions

- The engine-output view per card (`web/src/lib/party/state.js` `views`)
  remains the roster's read path; if a future epic changes that seam, this
  spec's FR-3 moves with it.
- Six users, one party, friends-and-family POC (PRD context) — no
  performance criteria beyond "renders with the roster" are needed.
- Layout details (tile grid vs list, sizes) are design- and
  prototype-driven; this spec fixes contents and labels, not pixels.

## Clarify Log

No `[NEEDS CLARIFICATION]` markers remain. The two deliberately deferred
decisions (key-skills rule, guardrail testability) are settled in FR-2 and
FR-3 respectively, with rejected alternatives recorded. No question in this
spec needs Josh.

**Round 1 (PR #197, reject — 6 blocking / 3 medium / 2 low)**: all findings
addressed in this revision — class-DC absence rescoped to `class_dc.base`
with the sheet alignment tasked (F1, F2); SC-5 re-mechanized as a scoped
query that can fail for what it claims (F3); provenance hover given its
non-default-state test, SC-8 (F4); the sweep re-scoped onto the directories
the new code actually lands in, blind spots named (F5, M1); the
innate-first discriminating fixture required in edge cases, design and plan
(F6); FR-3's statements re-bound each to its mechanism, Task 3's mutation
table now enumerates every FR-1 entry (M2); lore-label parity pinned by test
(M3); book-without-page citations re-anchored on readable sources (L1);
date stamps corrected to the commit's date (L2).

**Round 2 (PR #197, reject — 1 blocking / 2 medium / 1 low)**: SC-5
re-mechanized as an exact-set assertion — the region's focusable set is
exactly E6's provenance affordances, no control beyond them, mounted on
SC-8's non-empty-`applied` fixture with effects present — across all six
sites (B1); plan Task 3's executable line reads `class_dc.base` null (M1);
the sweep's blind-spot list made honest in spec and plan (M2); the
`spell_attack.total` mutation row added (L1).

**Round 3 (PR #197, reject — 1 blocking / 2 medium / 2 low)**:
`class_dc.total` and `casters → []` mutation rows added to all three
enumerations — spec FR-3 stmt 3, plan Task 3, design D8 (B1); the
blind-spot lists re-worded as directional-illustrative after the
reviewer's probe (right-hand operands, helper calls, wrapped fields)
(M1); the false "`StatTile` carries `aria-label`s" claim replaced —
StatBlock supplies the labels in its own markup, verified against
`StatTile.svelte` (M2); revision stamps updated through round 3 (L1);
"focusable or interactive" wording aligned across all SC-5 sites (L2).

**Round 4 (PR #197, reject — 2 blocking / 0 medium / 1 low)**: FR-3
statement 3's table completed — `fort`/`ref`/`will`/`perception` rows
added, matching plan Task 3 and design D8, which had carried them since
round 1 while citing this table as authoritative (B2, the reviewer's own
admitted miss); the accessible-name mechanism stated truthfully — the
round-3 replacement (`aria-label` on each tile) was also unreal, since
`StatTile` destructures its props with no rest and spreads nothing onto
its root div, so unknown attributes are dropped; tile labels are read as
**visible text** (`<div class="k">`), `StatTile.svelte` stays out of the
file map, and the group root keeps the region's `aria-label` (B1);
`Math.min` added to the plan's pattern prose (L1).
