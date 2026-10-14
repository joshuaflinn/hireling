# Feature Specification: GM Stat Density — Party-Card Stat Blocks (E14)

**Epic**: E14 — Phase 3, P1 · depends on E10 (closed) · GitHub issue #16
**Created**: 2026-10-16
**Status**: Specify gate run 2026-10-16 (Korrin, front half of the SDD pipeline — docs only, no code in this pass); design signed by Korrin same day. **Implementation hold (Thrane, binding): no E14 code merges to `main` before gh#13 closes — spec, design and plan on disk are fine.**
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
modifiers, by accessible label. Mutate the fixture (an applied effect moves
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
3. **Given** a character whose `class_dc` is `null` on the wire, **When**
   the card renders, **Then** no class-DC entry renders (the slot's output
   is null — nothing is invented); a character with a class DC shows it.
4. **Given** any effect change that recomputes a character's engine output
   (buff, condition, level adjust), **When** the recompute lands on the
   wire, **Then** the card's stat block reflects the new totals without a
   page reload — same WS fan-out, same store, as the existing HP bar.
5. **Given** a card whose engine output is not on the wire yet (bootstrap
   before first snapshot), **When** the card renders, **Then** the stat
   block shows its skeleton state — no invented numbers, mirroring the
   HP-bar skeleton the card already renders.
6. **Given** the GM seat, **When** the stat block renders, **Then** it adds
   zero write affordances: the card remains one navigation control, and
   nothing inside it is interactive.

## Functional Requirements

### FR-1 — Stat block contents (the card's numbers)

Each roster card extends, below the existing HP bar and effects strip, with a
compact stat block rendering, each with an accessible label:

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
- **Class DC** — `derived.class_dc.total`; the entry is **omitted** when the
  output's `class_dc` is null (E8 contract §1: null base ⇒ not rendered).
- **Key skills** — FR-2, exactly three rows: display name + signed total.

HP current/max stays the existing `HpBar` (it already renders `current / max`
text); the stat block does not duplicate it. Every entry reuses E6's
`StatTile`-style presentation including the provenance hover (applied /
suppressed verbatim from the engine output) — the GM can hover a number and
see its math, same as a player can on the sheet.

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
  genuinely in the top three. Per Player Core's proficiency rules an
  untrained skill adds no level and no proficiency bonus, so in practice
  trained rows dominate after the earliest levels — but the rule is honest
  arithmetic: filter nothing, rank what is.
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

**Testable statements** (each is an acceptance criterion the implementation
PR must demonstrate, and a reviewer can check):

1. **Verbatim read**: every stat-block number is a field of that view read
   as-is — `derived.<stat>.total`, `derived.casters[…].spell_dc.total`,
   `render_base.hp_max` (the bar) — or a vitals field store (the bar's
   current HP). Formatting (`signed`) is display, not computation.
2. **Selection ≠ derivation**: the key-skills comparator (FR-2) sorts and
   compares engine-provided totals; no party-lib code combines, sums, or
   offsets engine numbers. A static sweep of `web/src/lib/party/` finds no
   arithmetic over engine-derived values.
3. **Fixture tracking**: a component test mounts the card against an engine
   fixture, mutates the fixture's derived values (an applied buff moves
   `ac.total`; a skill total drops; a caster list empties), and asserts the
   card tracks every mutation exactly. A hardcoded constant, a cached
   recompute, or a second math path cannot pass.
4. **Sheet equality**: the same fixture drives the sheet's `StatsPane` and
   the card's stat block; AC, saves, Perception, spell DC and class DC
   render identical values in both. One engine source, two views — the E10
   chip-equality rule, extended to numbers.

### FR-4 — Initiative modifier display (and the non-goal, flagged)

The stat block shows an **Initiative** entry whose value is
`derived.perception.total` — PF2e's initiative is by default a Perception
check (Player Core, Encounters: Rolling Initiative). The entry's provenance
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
(the drill-in), the stat block's entries are non-interactive, and the GM
seat's zero-affordance posture (E10's acceptance) is unchanged.

### FR-6 — Offline and loading states

The stat block renders from whatever the roster state holds: cached engine
output offline (PWA, same as the HP bar), skeleton when no output has
arrived yet. No entry renders a placeholder number — the honest gap, same
rule as the card's existing skeleton bar.

## Success Criteria

- **SC-1** Every stat FR-1 names is visible on each party card without
  drill-in, labeled accessibly, for a character with a delivered engine
  output. *(Bench-verifiable at the implementation PR: component test by
  label.)*
- **SC-2** Two characters with different engine outputs render different
  stat blocks — every displayed value moves with its fixture field
  (two-fixture rule; FR-3 statement 3). *(Bench-verifiable.)*
- **SC-3** Card stat values equal the sheet's values for the same character
  state, including under an active effect (FR-3 statement 4). *(Bench-verifiable.)*
- **SC-4** The key-skills tie-break fixtures each render deterministically:
  equal total / different rank → higher rank wins; equal total and rank /
  different names → name ascending; three-way tie → exact predicted order;
  and the selection re-runs when a fixture's totals change. *(Bench-verifiable,
  boundary-style assertions per the two-fixture rule.)*
- **SC-5** No write affordance exists on the stat block for any role; the
  card's only interactive element remains the drill-in. *(Bench-verifiable:
  role query on the mounted card.)*
- **SC-6** The static sweep of FR-3 statement 2 passes: no arithmetic over
  engine numbers in `web/src/lib/party/`. *(Bench-verifiable: the named grep,
  run and pasted in the implementation PR.)*
- **SC-7** A stat block on a card whose engine output has not arrived
  renders skeleton entries, no numbers. *(Bench-verifiable.)*

## Edge Cases

- **Character with no casters** — no spell-DC entry; the block shortens.
- **Character with `class_dc: null`** — no class-DC entry (never "0",
  never "—").
- **Innate-only caster** (sorcerer with innate block only) — the sheet's
  fallback picks the first block; the card shows the same DC the sheet
  shows (equality, SC-3, holds over the fallback too).
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
