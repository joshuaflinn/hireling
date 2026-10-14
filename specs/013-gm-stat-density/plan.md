# GM Stat Density Implementation Plan (E14)

**Epic**: E14 — Phase 3, P1 · GitHub issue #16 · spec + design in this
directory (2026-10-14, Korrin; revised same day after round-1 review).
**Seat note**: this plan lands now, ahead of implementation, per the lane's
opening task. **Binding hold (Thrane): no E14 implementation merges to
`main` before gh#13 closes.** The implementation seat may build and PR; the
merge ask waits for the hold to lift.

## Global Constraints

- **Read-only epic**: the whole diff surface is `web/` files named in
  design §File map. No engine change, no wire change, no backend change, no
  schema. A task that wants one stops and flags.
- **No second computation path** (spec FR-3): no arithmetic over
  engine-derived values in `web/src/lib/party/` or in this epic's new
  engine-lib helpers (`key-skills.js`, `caster.js`; pre-existing
  `format.js` arithmetic is outside this diff and outside the claim).
  Selection (comparing, sorting) is allowed; combination is not.
- **No new dependencies** (Article V); hand-rolled CSS only.
- Tests run the production path: components render via
  `@testing-library/svelte` (`web/vite.config.js` jsdom setup); pure helpers
  test as plain units (`vitest run`). Two-fixture / both-sides-of-the-
  boundary style for every data-sourced assertion (AGENTS.md Style).
- Verify per task: `npm --prefix web run test -- run <file>` scoped, then
  `just ci-local` full before the PR. Paste real output; count warnings.
- Commit messages imperative; `Co-Authored-By: Paperclip <noreply@paperclip.ing>`
  on every commit.

## Task 1 — Extract `primaryCaster` (D5)

**Files**: `web/src/lib/engine/caster.js` (new — small enough to keep
`partition.js` single-purpose), `web/src/lib/sheet/components/StatsPane.svelte`
(switch its inline `find` to the helper), `web/tests/engine/caster.test.js`
(new).

**Do**: move the spell-DC block selection — first non-innate caster, else
the first block, else `null` — verbatim out of StatsPane's `$derived` into
`primaryCaster(casters)`, JSDoc'd with the design D5 note (rules-flavored:
innate ranks below tradition blocks). StatsPane's rendered output must not
change; its existing tests are the guard.

**Acceptance**: `caster.test.js` leads with the **discriminating ordering**
— casters `[innate, non-innate]`: the non-innate block wins the pick. This
is the one case `casters[0]` gets wrong, and the reference fixture
(`[Wizard(non-innate), Wellspring Gnome(innate)]`) cannot express it — on
that ordering the two rules agree, so a degraded extraction passes silently
(round-1 finding F6). Then: non-innate-first ordering, innate-only
fallback, empty list → `null`, null/undefined input → `null`. **Plus one
StatsPane-level test with the same innate-first fixture asserting the
rendered spell DC is the non-innate block's** — the sheet-side regression
guard the extraction previously lacked. `npm --prefix web run test -- run
web/tests/engine/caster.test.js` plus the sheet suites green: `… run
web/tests/sheet`.

## Task 1b — Align StatsPane's class-DC tile with contract §1 (D6, spec FR-1)

**Files**: `web/src/lib/sheet/components/StatsPane.svelte` (line 112 —
the tile's conditional render), the suite that mounts StatsPane
(`web/tests/shell/components.test.js`, or a focused new test file — seat's
pick).

**Do**: render the Class DC tile **conditionally** — `class_dc.base` null ⇒
no tile (today: unconditional `value={numbers.class_dc.total ?? '—'}`,
against E8 contract §1's null rule; the dash tile has no provenance hover
and no information). This is a deliberate, contract-restoring **sheet
behavior change** — the PR body names it as such. No test currently pins
the dash (checked: `grep 'Class DC' web/tests/sheet` finds no render
assertion), so nothing breaks silently.

**Acceptance**: a fixture with `class_dc.base: null` renders **no** Class DC
tile through StatsPane; a non-null fixture still renders it with the same
value and note as before (two-fixture, both sides). Sheet suites green.

## Task 2 — `keySkills` helper, tie-break pinned (D4, spec FR-2)

**Files**: `web/src/lib/engine/key-skills.js` (new),
`web/tests/engine/key-skills.test.js` (new).

**Do**: `keySkills(skills, count = 3)` — sort `total` desc → `rank` desc →
`name` asc, slice, return rows with display name resolved (lore rows:
`<label or key-derived> Lore`, E6's `partition.js` convention — mirror its
two-line fallback, do not re-abstract it; core rows: capitalized key).

**Acceptance** (each fixture asserts both sides, different results):
- equal `total`, ranks 2 vs 8 → rank-8 row wins the earlier slot;
- equal `total` and `rank`, `athletics` vs `thievery` → athletics first
  (and a `lore:underworld` vs `lore:warfare` pair → underworld first);
- top-3 boundary: a fourth row one point lower stays out; raising it one
  point puts it in and drops the old third — **selection re-runs on changed
  totals** (SC-4);
- untrained row (rank 0) with a genuinely top-3 total wins its slot;
- fewer than three rows → all returned, never padded;
- **lore-label parity**: the same lore rows through `partitionSkills`'
  lore resolution and `keySkills`' display resolution produce identical
  labels — a `label`-carrying row and a label-less row (key-derived
  fallback), both sides asserted (round-1 finding M3: the mirrored
  fallback is drift-prone; the parity test is the guard);
- input not an array → empty result, no throw.
`npm --prefix web run test -- run web/tests/engine/key-skills.test.js`.

## Task 3 — `StatBlock.svelte` (D2, D3, D6, D7; spec FR-1, FR-4, FR-6)

**Files**: `web/src/lib/party/StatBlock.svelte` (new),
`web/tests/party/stat-block.test.js` (new).

**Do**: props `{ view }`; tiles per FR-1 via `StatTile` (AC plain; Fort /
Ref / Will / Perception / Init signed via `signed()`; Init = perception's
total + provenance, label `Init`; Spell DC via `primaryCaster` with attack
note, omitted when no caster; Class DC omitted when `class_dc` null); the
key-skills `<dt>/<dd>` list via `keySkills`; skeleton rows when `view`
null. No interactive elements anywhere in the unit.

**Acceptance**:
- mount with a full fixture: every FR-1 entry found by accessible label
  with the fixture's exact values (SC-1);
- **mutate every FR-1 entry's backing field, one at a time, and re-assert**
  (spec FR-3 stmt 3's table — round-1 finding M2): `ac.total` +2; each of
  `fort/ref/will.total` moved; `perception.total` (the Init entry moves
  with it); `casters[0].spell_dc.total` moved; `class_dc.base` → null and
  the entry disappears; a key-skill total moved far enough to re-select;
  `render_base.hp_max` moves the bar — every rendered value tracks exactly
  (SC-2);
- the same fixture through `StatsPane`: AC, saves, Perception, spell DC,
  class DC values equal — **absence included**: `class_dc.base` null omits
  both the card entry and the sheet tile (SC-3, unconditional after
  Task 1b);
- **provenance hover, two-fixture (SC-8)**: reference-style empty `applied`
  → no provenance affordance on any tile; a fixture with non-empty
  `ac.applied` → the breakdown renders on the card's AC tile and the
  sheet's, engine text verbatim;
- null view → zero stat numbers rendered (SC-7), skeleton present;
- **SC-5 unit half**: the stat block's root is a labelled group
  (`role="group"`, `aria-label="Stats"`); `within(group)` finds no
  interactive role and no `tabindex`.
`npm --prefix web run test -- run web/tests/party/stat-block.test.js`.

## Task 4 — Mount in `RosterCard`, card-level acceptance (D2)

**Files**: `web/src/lib/party/RosterCard.svelte`,
`web/src/lib/party/state.js` (only if the `view` needs exposing — prefer
deriving from what `card` already carries; D1 allows either, pick the
smaller), `web/tests/party/roster-card.test.js` (extend).

**Do**: mount `StatBlock` between HP bar and effects strip; CSS for the
density (tiles wrap, key-skills line, `--panel`/`--edge` tokens like the
card).

**Acceptance**: roster-card suite extended — the card renders the stat
block below the bar; **SC-5 scoped query: card mounted with effects
present (ConditionTips render — the case the card-level count cannot
guard), stat-block group located by role/name, no interactive role and no
`tabindex` inside it**; the drill-in remains the card's navigation control;
down/full/effect-chip regressions untouched (existing tests stay green).
`npm --prefix web run test -- run web/tests/party`.

## Task 5 — Sweep, gate, PR

**Do**:
1. SC-6 sweep over its exact scope — `web/src/lib/party/` **plus this
   epic's new engine-lib helpers** (`key-skills.js`, `caster.js`):
   `grep -rnE "(total|base|rank)[[:space:]]*[-+*/]|reduce\(|Math\.(max|min)" \
     web/src/lib/party/ web/src/lib/engine/key-skills.js web/src/lib/engine/caster.js`
   → no hits over code introduced by this diff (tune the pattern to the
   diff's reality). Paste command and output; **name the pattern's blind
   spots next to it** (no-space operators are caught here but template
   concatenation and aliasing are not) and state the compensating evidence:
   line-by-line diff review plus the SC-2/SC-3 tests — the grep is a
   tripwire, not proof, and the PR body says so;
2. `just ci-local` full run, pasted; warning count named (a clean handover
   states warnings, and warnings naming diff files get fixed before review);
3. branch up to date with `origin/main` before the review ask;
4. PR body: spec/design/plan links, `Refs #16` (NOT `Closes` — the epic
   closes on shipped reality), the merge hold named up top, **the Task 1b
   sheet behavior change named as a change** (classless characters lose
   the `Class DC —` tile — contract-restoring), gate output, sweep,
   reachability (every SC mapped to its test per design D8, SC-1..SC-8).

**Acceptance**: gate green at the PR head (check-run read at the SHA named
from the ref), review round announced on the PR, merge ask to Thrane only
after the review verdict and **only after gh#13 closes**.

## Self-Review

- Every SC in the spec maps to a named test (D8) — SC-1..SC-8, including
  SC-6 (sweep + review, honestly labelled) and SC-8 (provenance hover,
  non-default state) — none relies on review goodwill alone.
- Tasks 1–4 are each one sitting of work with their own green command;
  Task 5 is the gate. Task 1 is deliberately first and mechanical — the
  sheet-parity extraction lands before anything renders from it, pinned by
  the innate-first ordering the reference fixture cannot express. Task 1b
  is deliberately separate from Task 1: Task 1's guard is "output must not
  change", Task 1b changes output on purpose — keeping them apart keeps
  both guards honest.
- Every test that claims to guard a choice uses a fixture on which the
  wrong choice fails: innate-first casters (F6), `class_dc.base` null
  (F2), non-empty `applied` (F4), effects-mounted scoped query (F3).
- No task touches anything outside the design's file map; the stop-and-flag
  rule is written into the constraints so the seat never has to guess.
- The plan contains zero implementation detail that contradicts the spec:
  layout pixels, final markup shape, and helper file choices are the seat's
  to make inside the design's fences.
