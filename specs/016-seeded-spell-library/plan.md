# Seeded Spell Library + Conflict Pre-Warn Implementation Plan (E13)

**Epic**: E13 — Phase 3, P1 · GitHub issue #15 · spec + design + contracts in this directory
(2026-10-16, Korrin). **Docs-only through this PR.** These are the three **build heats** that
open the moment this docs PR merges; each is scoped to land inside one heartbeat on a healthy
adapter, and each is independently reviewable. Heat **H2** is *not* content-gated (design **D0**);
**H3** needs a seed file with at least one spell in it to be *visible at the table*, and the
content ask ([`contracts/content-ask.md`](contracts/content-ask.md)) names exactly what that means.

## Global Constraints

- **Scope**: the whole code-half diff surface is design **D10**'s file map. Anything a task wants
  outside it stops and flags. Read-only consumers, never edited: `engine/src/model.rs`,
  `engine/src/stack.rs`, `engine/src/compute.rs`, `engine/src/vocab.rs`, `src/sync/write.rs`, the
  E9 custom-entry path, and the P0 composer's freeform branch.
- **Zero engine change beyond one new pure module** (design **D3**), **no new wire frame kind**,
  **no migration** (**D8**), **no P0 test modified** — those four claims are the epic's safety
  argument and each is checkable in the diff. A task that cannot proceed without breaking one has
  hit a stop-and-flag, not a licence.
- **No seat of this company authors spell content** (spec FR-3). Test fixtures are labelled
  fictional in the file; nothing in `data/seed/spell-library.json` is written here beyond the
  empty file plus its `_readme`. An "illustrative" spell added to make a test read nicely is a
  scope violation, not a convenience.
- **Every fail-closed claim names its assertion, per input** (AGENTS.md). The loader's defect
  classes D1–D8 are **eight inputs** — each gets its own fixture and its own assertion through the
  production load path. One test over three classes certifies nothing. Same rule for the
  pre-warn: a "no flag" claim needs a fixture that could have produced a flag.
- **Two-sided assertions** (AGENTS.md Style + rules-exactness): two degrees differing → two
  results; flagged target vs unflagged target in one render; penalty-vs-bonus no-flag **and**
  same-type flag; empty library vs loaded; `+0` vs non-zero; stat instance present vs absent.
- **Verification per task**: scoped suites first (`cargo test -p hireling-engine preconflict`,
  `cargo test spellbook`, `npm --prefix web test -- spell`), full `just ci-local` before any PR,
  paste real output, and **state the warning count** — a handover that calls the gate clean
  without the count is incomplete. A file the lint touched is a file you review and re-stage
  (`git diff` before `git add`), not a file you attribute to a stale toolchain.
- **Worktree discipline (this epic runs concurrent children)**: the harness binds a worktree to
  the *lineage*, so sibling heats provision back into the `MOR-764` tree. Do the AGENTS.md pickup
  tree check; if the branch is another PR's head or the tree holds foreign paths, hand-cut
  `hireling-764-<slug>` on a prefixed branch fresh from `origin/main` and commit there. Never
  `git add -A`.
- **Content lands as data** (design **D1**): after H1, adding a spell must touch exactly one file
  and no code. If it doesn't, that is a bug in H1, filed against H1 — not a reason to hardcode.
- `Co-Authored-By: Paperclip <noreply@paperclip.ing>` on every commit; imperative one-line
  subjects. PR bodies carry `Refs #15` (**not** `Closes` — the epic closes when built), a link to
  the Paperclip heat task, the gate output with its warning count, and the reachability sweep.

## Heat H1 — Loader + validation + read (design D1, D2, D4) — one heartbeat

**Files**: `src/spellbook/{mod,model,seed}.rs`, `src/spellbook/tests/*`,
`data/seed/spell-library.json`, `src/lib.rs`/`src/main.rs` (module + boot call), `src/http.rs` +
`src/engine_host/rest.rs` (the read), `src/tests/spellbook_rest.rs`, `justfile` (`spellcheck`),
`.github/workflows/gate.yml` (one step), `Cargo.toml` only if a dev-dep is needed for fixtures.

**Do**: the strict serde model per the contract doc; the four-degree key set as a code constant
whose doc comment names its *Player Core* source and page (rules-exactness §1 — the four keys are
a PF2e rules fact hard-coded in the loader, **D2**'s defect-class table requires "degree keys:
exactly the four, all present", and the per-entry `citation` field covers Dave's content, not this
constant); `Library::load()` that parses the embedded
file, validates **vocabulary and shape only** against `hireling_engine::vocab::Stat::parse` and
the engine's `MODIFIER_TYPES` (re-exported, never copied — the shipped `import/seed.rs` pattern),
and returns a `LibraryDefect` naming **spell, degree, row index, field** in the contract's message
shapes. Wire it into boot as a fatal error. Add `GET /api/parties/{party_id}/spells` on the
existing `party_readable` scope gate, serving the parsed value (no query, no cache). Ship the seed
file as `{"schema": 1, "spells": []}` plus a `_readme` naming the owner and the schema doc. Add
`just spellcheck` as the boot-equivalent load assertion and one `gate` step calling it.

**Acceptance**:
- Green cases asserted as green: **empty `spells` loads**, and **an empty per-degree list loads**
  (both are configurations, not accidents — spec SC-6, **D2**).
- Defect cases D1–D8 each get a fixture and assert the *message contents*, not just an error: the
  unknown stat names the spell, degree, row and field; the unknown degree key names the key; the
  duplicate name names both entries; malformed JSON and an unsupported `schema` version each
  produce their own message. Eight inputs, eight assertions, through `Library::load()` — not
  through a helper.
- The REST read contract test asserts, **off the live schema rather than a literal restated in
  the test body**, that the payload carries every degree key and every effect-row field the client
  will read; a non-member session is refused (403) and a member session is served. A fixture
  swapped into the loader changes what the route returns, which is SC-9's "data-only change ships
  with the server" half.
- `just spellcheck` exits non-zero on a fixture with a known-bad stat, and CI shows that step
  green on the real file.
- Full `just ci-local` clean at the pushed head, warning count stated; names in the diff.

## Heat H2 — Conflict pre-warn: engine verdicts + the read (design D5, D6) — one heartbeat, no content

**Files**: `engine/src/preconflict.rs`, `engine/src/tests/preconflict.rs`, `engine/src/lib.rs`
(module registration only), `src/engine_host/rest.rs` (the conflicts read handler), `src/http.rs`,
`src/tests/spellbook_conflicts.rs`.

**Do**: the pure function per **D6** — build existing candidates with the shipped
`stack::candidates`, append one **shadow `ActiveEffect`** (a real one-modifier effect carrying the
sentinel `effect_id`) and re-run the **public** `stack::candidates` so the engine's own private
`fan_out` produces its `StatRef`(s) — `fan_out` is not `pub` (`engine/src/stack.rs:90`), and
`stack.rs` stays on the read-only list — group per `StatRef`, run the shipped `stack_one` twice,
and **report the delta** as
`applies | would_not_stack | would_be_suppressed | no_instance`, quoting the suppressing effect.
No spell/template/library concept enters the engine. Then the session-gated, party-scoped
`POST /api/parties/{party_id}/spell-conflicts` that loads every roster character's active effects and
`BaseStats` from the same sources the derived read uses and returns per-target verdicts (**D5**).

**Acceptance** (all through the pure function or the real route, never a re-implementation):
- **Blanket vs specific**: a target holding a blanket AC penalty sees a flag for a specific AC
  bonus and none for a Fortitude bonus. Both sides in one fixture (this is the case a
  "same type + same stat" shortcut gets wrong).
- **Penalty vs bonus coexists**: no flag, and the same fixture with a same-type bonus does flag.
- **Tie**: same value → the "only one applies" wording, and it **names no winner** (so the design
  stays true whatever the id ordering does). Separately, assert the shipped fact **D6** leans on:
  `effect_id` values come from a monotonic source — if that assertion ever fails, the wording is
  already safe and the test says why.
- **`+0`** produces no suppression flag (the special case in the shipped `stack_one`).
- **No instance** on the target (e.g. a caster-specific caster-stat row on a target with no
  caster) → `no_instance`, **no flag** — the threshold's other side is a matching instance that
  does flag.
- **Per-instance fan-out**: a target with two melee strikes gets per-strike verdicts, not one
  collapsed verdict.
- The route returns per-target verdicts for the whole roster in one call, refuses a non-member,
  and a read-only call leaves no audit row. **Zero content dependency**: every fixture builds its
  `ActiveEffect`s and `BaseStats` directly.

## Heat H3 — Composer section + tap + picker flags (design D7) — one heartbeat, needs ≥1 spell to be *visible*

**Files**: `web/src/lib/sheet/spell-library.js`, `web/src/lib/sheet/conflicts.js`,
`web/src/lib/sheet/components/SpellLibrarySection.svelte`,
`web/src/lib/sheet/components/EffectsComposer.svelte` (mount + wiring **only** — the freeform
form's fields and validation are untouched), `web/src/lib/sheet/state.js` or `SheetView.svelte`
(the two reads' state), `web/tests/sheet/{spell-library-compose,spell-conflicts,spell-library-gm}.test.js`.

**Do**: fetch the library on composer open alongside the existing effects read; render one row per
spell, revealing its degrees from the data; the existing target picker unchanged; one debounced
conflicts read keyed on `(spell, outcome)` alone (**not** per picker-change, **not** per
keystroke — **D5**) with the roster-wide response filtered locally and flags inline on each
flagged target row, never disabling a row; tap → assemble the create payload from the template's rows for that
degree → the composer's existing `oncreate` (so bounds validation, ack handling, offline queueing
and error surfacing are inherited). No slot deduction, anywhere. Empty library renders an honest
one-liner; a degree with an empty effect list renders and taps to an explanatory line with **no
`oncreate` call**.

**Acceptance** (component tests via `@testing-library/svelte` + `fireEvent`, real events, real
branches):
- Tap a two-row spell → **exactly one** create call carrying exactly those rows, and the picker
  state the caster chose (SC-1). Assert the payload, not just the call count.
- Two degrees of the same spell produce **different** effect payloads (two-fixture rule; a
  one-degree-only test certifies nothing about degree selection).
- A degree with `[]` produces **zero** create calls and the explanatory text (FR-7).
- Four targets in one render showing the four states (full stack / partial / none / incompatible)
  come from **one** fixture-driven conflicts payload, and each flag is an engine-verbatim reason
  (SC-3, SC-4).
- **Live, not cached**: mutate the fixture's active-effect payload between two renders and assert
  the flag set changes (and that a newly-added conflicting effect produces a flag on the next
  read). This is the guardrail's test — if a cache were introduced here, this assertion is what
  catches it.
- **GM seat**: a scoped region query over the composer proves no library section and no write
  affordance (SC-8 — assert the region's control set, not a page count).
- **Empty library**: the section renders its honest line and the **existing** composer suite passes
  unedited at this head (SC-6).
- **SC-7 sweep** in the PR body: grep the new client files for any modifier-comparison or
  stacking arithmetic — the sweep's claim is *zero* computation outside the engine; directional
  like E14's SC-6, and the diff is the real check.

## Tail T1 — Content PR (Josh's list, Dave's templates) — not this company's authorship

One data-only PR against `data/seed/spell-library.json`. The `spellcheck`/loader leg is the
reviewer's first line of defence; Josh reviews. Nothing in H1–H3 changes. **This is the epic's external
dependency and the reason the content ask exists.**

## Tail T2 — Post-content smoke (SC-10) — small

Cast 2–3 real spells end to end at the table shape, confirm the effects look right to the owners,
and land the ~3 fixes that surfaces. Any fix that moves engine math or an engine rule goes to
**E8**, not to E13 (design **D6**/spec Out of Scope).

## Sequencing and cost

H1 and H2 are independent and parallel; H3 follows both (it consumes the read and the verdicts).
T1 is external and blocks nothing until T2. Two engineers can have H1+H2 in one shift; H3 is one
seat, one shift. Nothing here needs Dave, Vex, or a migration; nothing here waits on the seed list
except the *walkthrough*, which is why the walkthrough is a tail and not a gate.
