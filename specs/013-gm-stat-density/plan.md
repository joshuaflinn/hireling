# GM Stat Density Implementation Plan (E14)

**Epic**: E14 — Phase 3, P1 · GitHub issue #16 · spec + design in this
directory (2026-10-16, Korrin).
**Seat note**: this plan lands now, ahead of implementation, per the lane's
opening task. **Binding hold (Thrane): no E14 implementation merges to
`main` before gh#13 closes.** The implementation seat may build and PR; the
merge ask waits for the hold to lift.

## Global Constraints

- **Read-only epic**: the whole diff surface is `web/` files named in
  design §File map. No engine change, no wire change, no backend change, no
  schema. A task that wants one stops and flags.
- **No second computation path** (spec FR-3): no arithmetic over
  engine-derived values anywhere in `web/src/lib/party/`. Selection
  (comparing, sorting) is allowed; combination is not.
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

**Acceptance**: `caster.test.js` covers non-innate preferred over a later
innate, innate-only fallback, empty list → `null`, and null/undefined input
→ `null`. `npm --prefix web run test -- run web/tests/engine/caster.test.js`
plus the sheet suites green: `… run web/tests/sheet`.

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
- mutate the fixture (`ac.total` +2, a skill total −5 reordering key
  skills, casters emptied) → every rendered value tracks exactly (SC-2);
- the same fixture through `StatsPane`: AC, saves, Perception, spell DC,
  class DC values equal (SC-3);
- null view → zero stat numbers rendered (SC-7), skeleton present;
- `queryAllByRole('button')` inside the mounted StatBlock → empty (SC-5
  unit half).
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
block below the bar; the card's only button remains the drill-in (SC-5);
down/full/effect-chip regressions untouched (existing tests stay green).
`npm --prefix web run test -- run web/tests/party`.

## Task 5 — Sweep, gate, PR

**Do**:
1. SC-6 sweep, pasted with its command: `grep -rnE "derived\.[a-z_]+\.total [+*/-]|\.total [+*/-] [a-z0-9]" web/src/lib/party/` → no hits (tune the pattern to the diff's reality; the claim is "no arithmetic over engine numbers in party/", the grep is its evidence — hand-review the diff alongside it);
2. `just ci-local` full run, pasted; warning count named (a clean handover
   states warnings, and warnings naming diff files get fixed before review);
3. branch up to date with `origin/main` before the review ask;
4. PR body: spec/design/plan links, `Refs #16` (NOT `Closes` — the epic
   closes on shipped reality), the merge hold named up top, gate output,
   sweep, reachability (every SC mapped to its test per design D8).

**Acceptance**: gate green at the PR head (check-run read at the SHA named
from the ref), review round announced on the PR, merge ask to Thrane only
after the review verdict and **only after gh#13 closes**.

## Self-Review

- Every SC in the spec maps to a named test (D8) — none relies on review
  goodwill alone.
- Tasks 1–4 are each one sitting of work with their own green command;
  Task 5 is the gate. Task 1 is deliberately first and mechanical — the
  sheet-parity extraction lands before anything renders from it.
- No task touches anything outside the design's file map; the stop-and-flag
  rule is written into the constraints so the seat never has to guess.
- The plan contains zero implementation detail that contradicts the spec:
  layout pixels, final markup shape, and helper file choices are the seat's
  to make inside the design's fences.
