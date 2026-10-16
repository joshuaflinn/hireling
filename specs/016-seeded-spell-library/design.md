# Design: Seeded Spell Library + Conflict Pre-Warn (E13)

**Status**: signed by Korrin (PM seat) 2026-10-16 — every decision here is internal to E13's
P1 scope; **nothing moves the P0/P1/P2 line, adds a dependency on Dave, Vex, or anything
outside this company, or touches `CONSTITUTION.md`**. Gate signed on those grounds; no
escalation raised. Input chain: [`spec.md`](spec.md). Contract docs:
[`contracts/spell-library-seed.md`](contracts/spell-library-seed.md) (the file schema),
[`contracts/content-ask.md`](contracts/content-ask.md) (the Josh/Dave ask). The plan
([`plan.md`](plan.md)) executes this.

The shape in one paragraph: the library is a checked-in JSON seed file validated at load against
the engine's re-exported vocabulary, served to the client through one new session-gated read;
the composer grows a library section that turns a spell selection plus a tapped degree into one
ordinary `effect_new` create frame; effects already on each target come from the manager-read
the composer already fetches and refreshes on every applied ack, so a candidate's stacking
verdict for each target is computed **in the engine** by a pure function that runs the shipped
`stack_one` over existing candidates plus a shadow candidate, and the client asks for the
verdicts for the whole roster in one party-scoped read whenever the picker changes. No engine
change, no new wire kind, no migration.

---

## D0 — The buildability claim, and the two places it was tested hardest

The task's instruction: design the code half so it is buildable against an empty or minimal seed
config, and if that is not possible, say so and name the dependency rather than paper over it.
**It is possible.** Two places could have broken the claim; neither does.

1. **The conflict pre-warn's candidate set (D6).** A candidate's stacking verdict against *real*
   shipped effects cannot be exercised at the table while the library is empty — but it does not
   need to be, because the *inputs* are data the test supplies: an `ActiveEffect` list and a
   `BaseStats` set. The unit is a pure function over those inputs, fully testable with fixtures
   that are nothing but test scaffolding (which is exactly what E8's own engine tests already
   are). Nothing about it reads the spell file.
2. **The UI's library-dependent branches (D6's client half).** Every library branch the composer
   renders is driven by the read's payload — an empty array is a rendered, tested state. Test
   fixtures inject a fictional two-spell payload through the read seam, exactly as E9/E12/E14's
   tests inject fictional engine outputs. The composer's tests never need the shipped file to be
   non-empty, and the fixtures are labelled fictional in the test tree, so no placeholder content
   enters `data/`.

**What genuinely depends on content** is one thing: SC-10 ("the shipped spells' effects look
right to the owners"). It is owned by the content PR and Josh's review, and no code PR's merge
depends on it (spec SC-10). **The one product decision with a real fork** is whether library cast
spends a slot (spec Clarify Log #5): this design builds *no*, and D7 names the single place a
future *yes* would attach.

---

## D1 — The seed file: one checked-in JSON, loaded and validated like the tier seed

`data/seed/spell-library.json`, schema in [`contracts/spell-library-seed.md`](contracts/spell-library-seed.md).
At this epic's merge time it is `{"schema": 1, "spells": []}` plus a `_readme` block.

Loading follows the shipped precedent in full — the tier seed (`data/seed/condition-tiers.json`,
`src/import/seed.rs`, `src/import/mod.rs::SEED_PATH`): the file is checked in, embedded at build
(`include_str!`), and validated at load;
a bad file is a startup failure, not a degraded run. That precedent exists precisely to make
human-authored, review-before-ship data safe to load, and E13 joining it costs one module.

**Rejected — the importer as the loader.** The tier seed is consumed inside an *import run*,
whose run-report channel ("unmapped conditions") is the right shape for pack-side ambiguity and
the wrong shape for a config defect: a library typo has no "continue anyway, note it in the
report" reading. The **validation and fail-loud posture** are borrowed from `import/seed.rs`;
the **import-run report machinery** is deliberately not. The library is static config, not pack
data: it is not keyed by Foundry `_id`, does not participate in a corpus re-import, and has no
lane.

**Rejected — a Rust-`const`/`lazy_static` literal spell list.** It puts human content in a code
file, makes the content PR a code PR, and blurs exactly the line this epic exists to keep.

## D2 — The loader and validation: `src/spellbook/` (new module, pure except the read)

```
src/spellbook/mod.rs      // the module docs, re-exports
src/spellbook/model.rs    // the serde structs (strict), the parsed Library
src/spellbook/seed.rs     // the embedded file, load(), the defect messages
src/spellbook/tests/*     // one fixture per defect class, green cases included
```

Strict serde (`deny_unknown_fields`) so a misspelled key is a defect, not a silently-dropped
field — the shipped JSON-shape-drift posture from `src/import/model.rs`. Validation is
**vocabulary and shape only**:

| check | how | defect class |
|---|---|---|
| stat text | `hireling_engine::vocab::Stat::parse` — the engine's own function, not a copy | D1 |
| modifier type | the engine's `MODIFIER_TYPES` / `ModifierType::parse` | D2 |
| value | integer, within the composer's existing bound | D3 |
| degree keys | exactly the four, all present | D4 |

The four degree keys (`crit success` / `success` / `failure` / `crit failure`) are a **PF2e rules
fact hard-coded in the loader**, not content. Rules-exactness §1 applies to the constant itself,
not only to the per-entry `citation` field that covers Dave's content: the degree-key set carries
its *Player Core* source and page in its doc comment, written off the source page. The plan
requires this in H1.
| target rule | `"chosen"` \| `"self"` | D5 |
| required non-empty strings | `name`, `duration_note`, `citation`, each `prose` entry | D6 |
| name uniqueness | set over entries | D7 |
| top-level shape / `schema` version | strict serde + version check | D8 |

Every message names **spell, degree, row index, field** — the contract's message shapes — because
the reader is Dave, in a PR, not a debugger. **Empty `spells` is green. An empty per-degree list
is green.** Both are asserted as green cases, not assumed.

`pub use hireling_engine::vocab::{…}` is the re-export pattern already in `import/seed.rs`:
one vocabulary source, no drift, and the engine's own test pins the strings to its enums.

**Fail-loud placement (the guardrail's letter).** `Library::load()` is called on the boot path
where a `Result::Err` is fatal, and it is also called by a CI step (D10's `just spellcheck`) and
by a test that asserts the shipped file parses. So a content PR is red on the same message the
server would have refused to boot with, and the failure is never reachable from the table.

## D3 — No engine change; the pre-warn lives in the engine as a pure function

The engine is pure and closed; E13 adds **no concept** of "spell", "template", or "library" to
`engine::model`, `engine::stack`, or the wire. The one thing E13 adds there is a pure query
(D6) that speaks only in the engine's existing nouns — an `ActiveEffect` list, a `BaseStats`
set, a candidate modifier — and returns the engine's existing verdict records.

**Rejected — adding a `library` flag or `spell_id` to `ActiveEffect` / the create frame.** It
would make a library effect distinguishable in engine output and on the wire, which forces
re-reasoning about provenance, suppression, offline replay, and the GM rule for a distinction
nobody consumes (spec FR-17). A library effect is an ordinary effect; that is the design's load-
bearing property, and SC-6 proves it by merging with the library empty and **modifying no P0
test**.

## D4 — The read: one new session-gated party-scoped route

`GET /api/parties/{party_id}/spells` → the parsed `Library` as JSON, joining the existing
party-scoped reads (`/conditions`, `/effects`, `/items/book-value`, `/custom`) and reusing the
router's `party_readable` scope gate (`src/engine_host/rest.rs`).

Why a route and not an embed: the client bundle must not carry the library, or a data-only
content PR would require a client rebuild and a release — which is the property that makes
content landing cheap (spec SC-9). Served from the parsed static value, so the route costs a
serialization and no query.

**Rejected — riding the WS snapshot.** The snapshot is live *state*; the library is static
config. Putting config in the snapshot would imply a version, a diff, and a sync story for
something that has no state to sync (and would collide with the "no new wire kind" constraint).

**Rejected — an embedded client bundle.** See above; also loses fail-loud's value: the file
would be validated at build for a bundle and at boot for the server, in two places.

## D5 — Where the pre-warn's inputs live, and why they are live (the guardrail)

The guardrail: *conflict warnings computed from live engine state, not cached chips.* Three
inputs are needed per candidate target; each is named with its source, because "live" is a claim
until the source is named.

| input | source | live? |
|---|---|---|
| the target's **active effects** (name, type, value, stat text, `active`, `targets`) | the composer's existing manager read — `GET /api/parties/{id}/effects`, held as `effectsState.rows`, which the composer already consumes (`EffectsComposer.svelte:33,88`) and which the sheet refreshes on every applied ack | **Yes**, and SC-4 is the test that proves it rather than asserts it. This is *engine state read back*, not a chip cache — it is the same payload the effects list renders from. |
| the target's **stat instances** (which strikes, which casters, which skills) — needed to expand a blanket and to fan per-instance stats | `EngineOutput.render_base` per character, client-side for the caster and for any roster character the party surface has loaded (`web/src/lib/engine/index.js` → `sync.derived(characterId)`) | Partially — see the fallback below |
| the target's **base values** — *not needed*: a suppression verdict depends on the candidate set, not on the base number | — | — |

**The gap, named honestly.** Party roster cards render stat *values* (E14's stat block), not
stat *instance lists*, so per-target instance sets are not guaranteed client-side for every
roster character at pick time. The PRD's message shape ("Becky: +1 status active — Bless would be
suppressed") is a **per-target verdict**, not a per-instance breakdown, so a per-target answer
computed **in the engine** is both the honest and the cheap one:

**`POST /api/parties/{party_id}/spell-conflicts`** — session-gated, party-scoped, read-only.
Body: `{ spell: "<name>", outcome: "<degree>", targets: [<character ids>] }` — the *candidate
effect definition*, plus the target ids. Response: per target, a list of
`{ stat_text, verdict: applies | would_not_stack | would_be_suppressed | no_instance,
conflict: { effect_name, type, value, reason } | null }`. The server loads the party's active
effects and each target's `BaseStats` from the same sources the derived read uses, and evaluates
D6's engine function. This is the same shape as E12's book-value read (**D5** there): a
session-gated party-scoped REST read for a computation the client cannot honestly do from a
partial view.

**Rejected — client-side computation over locally-held engine output.** Rejected for the
honest reason: it would need each target's `render_base` for blanket expansion and per-instance
fan-out, which the client does not reliably hold (above), and a client-side verdict computed only
from a target's *chip list* would silently mis-decide blanket and per-instance cases — the exact
failure mode that teaches casters to ignore warnings (spec Clarify Log #4's rejected
alternative, arrived at from the data side). If review prefers the client-side route, it must
first make `render_base` reliably available for every roster card, which is a bigger change than
this read.

**Cost, stated plainly: one request per (spell, outcome) selection**, debounced on that key alone,
not on the picker and not per keystroke. **The verdict does not depend on the selection at all** —
a suppression verdict is computed per (target × stat instance) against *that target's* active
effects, so adding Josh to the selection cannot change Becky's verdict, and the key that includes
`selected` re-reads a payload that was already complete. The request body is therefore
`{ spell, outcome }` only — no `targets` — and the response is roster-wide (one call returns
per-target verdicts for every candidate, which is what plan **H2**'s acceptance already requires);
the picker filters that response locally. That is **one read per degree tap** — fewer than a
picker-keyed design, same freshness, no cache, so the guardrail and SC-4 both stay honest. The
latency question answers itself once the key is right: the read fires when the caster picks a
degree, which is before they are looking at the picker. A composer is opened seconds before
a decision and closed; a read at party scale (six characters, ≤ ~20 spells) is trivial —
`GET /effects` already returns the larger payload on every composer-open today. Caching is
explicitly **not** added: a cache here is exactly the "cached chips" the guardrail forbids, and
SC-4 is the test that would catch a cache that goes stale.

**Offline**: the composer's existing offline posture is inherited — a failed conflicts read
renders **no flags** (spec US-2.7: absence never renders as "safe-by-decision"), the freeform and
library writes still queue as they already do. The pre-warn is an enhancement, never a gate
(FR-12).

## D6 — The pre-warn's engine function: run the shipped `stack_one`, report what moved

Inside the engine (`engine/src/preconflict.rs`, ~new pure function, no state, no I/O):

1. Build the target's existing candidates with the shipped `stack::candidates(effects, character_id, instances)` — the same function the compute path uses, so "what is already on the sheet" is never re-derived differently here than in the engine.
2. Append one **shadow `ActiveEffect`** for the effect being considered — a real one-modifier
   `ActiveEffect` carrying the reserved sentinel `effect_id` and a synthetic `ord` — and run the
   existing candidates plus the shadow through the **public** `stack::candidates(effects,
   character_id, instances)` (D6 step 1's same function). Its `stat_ref`(s) are then produced by
   the engine's own fan-out, reached *inside* `candidates`, so blanket and per-instance handling is
   the engine's own, reused, not re-implemented (spec Clarify Log #4's rejected client shortcut,
   avoided structurally).

   **Why the public entry point and not the fan-out directly:** `fan_out` is **private**
   (`engine/src/stack.rs:90` — `fn fan_out`, no `pub`), and `engine/src/preconflict.rs` is a
   sibling module under the crate root, so it cannot see it. Every `ActiveEffect` field is `pub`
   (`engine/src/model.rs`), so the shadow is constructible from outside with no edit to
   `stack.rs` — which keeps `stack.rs` on D10's and the plan's read-only list, which is the point.
   The public `expand` for blanket stats *is* reachable
   (`engine/src/vocab.rs:492`, `pub fn expand`, over the `pub const BLANKET_EXPANSIONS` at `:438`)
   if a call site ever needs it directly; nothing here does. The alternative — `pub(crate)` on
   `fan_out` — is available, but it edits a file this epic's own safety argument declares
   read-only, and it is not needed.
3. Group per `StatRef` and run the shipped `stack_one` twice: once on the existing group, once on the group with the shadow appended.
4. **The verdict is the delta**: the shadow lands in `applied` → `applies`; it lands in `suppressed` → the verdict is its `SuppressionReason`, mapped to wording (`SameTypeLowerBonus` → *"would be suppressed"*, `SameTypeTie` → *"same value — only one applies"*, `SameTypeLighterPenalty` → for a penalty candidate), and the `suppressed_by_effect_id` names which live effect to quote in the message. No instance was produced for that stat → `no_instance` and **no flag** (a modifier the engine will drop is not a conflict).

**Zero-value candidates** need their own row in the tests, because the shipped `stack_one` treats
them specially (always applied, never competing): a `+0` template row must never produce a
suppression flag, and the delta rule handles it correctly only by accident of that special case —
so it is asserted, not assumed.

**Rejected — a dedicated comparison function** ("is there a same-type positive on this target
that is ≥ this value"). Rejected because it re-implements stacking outside the stacking core and
drifts from it — on penalties (which coexist with bonuses, WEx-4), on ties, on blanket expansion,
on per-instance fan-out, and on `+0`. The engine's rule is that stacking lives in exactly one
place (`stack.rs`); a warning that computes stacking a second way is a second computation path,
which the PRD forbids for numbers and which this epic refuses for verdicts. The delta method has
one further advantage: it is correct about *every* rule in `stack.rs` including the ones nobody
thought to enumerate here, because it *is* `stack.rs`.

**The mechanism, named so the next reader can check it without reading `select_extreme`:**
`stack_one` sorts the group ascending by the candidate key `(effect_id, ord)`
(`engine/src/stack.rs:158-163`, `ordered.sort_by_key(|candidate| candidate.key())`) and the
extremum winner is the **first** position holding the extremum
(`engine/src/stack.rs:241-248`, `winner_position` then `index == winner_position`). So a sentinel
placed *above* every real id loses a same-value tie and the **incumbent** is emitted `applied`.

**The one subtlety worth a reviewer's time:** the shadow candidate's tie-break position. With a
sentinel `effect_id` chosen larger than any real id, a same-value tie is decided in favour of the
*existing* effect — which matches what the real create would do only if the new effect id is
always larger than existing ones (true: `effect_id` is monotonic from a sequence). Say so in the
code and pin it with a test asserting tie-wording, so the claim is checked and not vibes; if the
id ordering ever stops being monotonic, the tie case must be reported as "only one applies"
without naming a winner — which is what the wording already says (spec Clarify Log #3).

## D7 — The client: the composer grows a section; the write path is untouched

`web/src/lib/sheet/components/SpellLibrarySection.svelte` (new), mounted inside the existing
`EffectsComposer.svelte` above the freeform form, plus `web/src/lib/sheet/spell-library.js`
(fetch + state for the read) and `web/src/lib/sheet/conflicts.js` (the flag formatting, no
verdict logic).

- **Read**: fetched on composer open (the same moment the manager effects read happens — no new
  fetch discipline, just a second call alongside it), with the existing loading/error posture
  ("no library" and "couldn't load" never collapse into each other, the rule already enforced in
  `sheet/state.js:272`).
- **Render**: one row per spell. Selecting a spell reveals its four degrees (rendered from the
  data, so a spell with three degrees shows three — spec edge case), each labelled with its
  prose; the target picker below is the picker that already exists, unchanged in behaviour.
- **Pre-warn**: when a spell+degree is selected and targets are picked, one conflicts read per
  change (D5); each flagged target's row carries the message inline, before assignment. Flags
  never disable a row (FR-12).
- **Tap** → assemble the create payload from the template's rows for that degree and call the
  composer's existing `oncreate` — the same path the freeform form uses, so bounds validation,
  op state, ack handling, offline queueing and error surfacing are inherited, not rebuilt.
- **Empty outcome list** → no `oncreate` call; a plain line stating the outcome does nothing
  (FR-7), and the composer stays open.
- **No slot deduction anywhere** (FR-9). The one place a future *yes* would attach is the tap
  handler, and it would be a *separate* write with its own authz question, not a side effect of
  this create — named here so the future change has a home and this epic's non-choice is
  visible.
- **`self` target rule** → the definition applies to the caster regardless of picker selection;
  it does not pre-select or lock the picker.
- **Empty library** → a one-line honest state ("No library spells yet — cast freeform below."),
  and the freeform form is byte-identical to today (SC-6).
- **GM seat** → no library section, matching how the composer is already GM-suppressed
  (FR-18, SC-8).

**Rejected — putting library prose into E15's curation layer (the E15-adjacent question, recorded here for spec FR-19).** E15 edits *corpus-imported display prose* in-app, versioned in the DB. Library
templates are checked-in seed config whose review happens in a PR, by the owners, per the PRD's
provenance rule; an in-app editor writing prose that ships from a file is either a fake editor
(it can't write the file) or a runtime-mutable authorship surface (it writes the DB and now two
places claim the text). If the party ever genuinely needs in-app template editing, the design
change is to move the library into DB rows with an authored/reviewed state column — the same
move E15's curation layer already made for corpus prose — and that is a new epic, not an E13
stretch.

## D8 — Storage: no migration. The re-visit trigger, recorded.

No schema change, no migration, no rows (spec Constraints). The rejected alternative —
`spell_library` rows seeded by a migration — is rejected because it makes a human-owned,
review-before-ship artifact mutable at runtime, splits authorship between SQL/UI and a PR, and
weakens the mechanism that makes the guardrail real: a bad template fails the *build*, not the
table. It also buys a write path nobody asked for (the PRD asks for no library editor).

**Re-visit trigger, written down so it's a decision and not a discovery**: move the library to
rows when *either* (a) the party asks to edit templates at the table (i.e. E15-shaped curation of
templates), or (b) the file grows past ~150 entries / a corpus import of spells lands (E4's lane,
not this one). Both are new epics with a migration, an authz story, and a review flow — neither
is an E13 follow-up.

## D9 — Test layout: every SC and every defect class to a named suite

| item | suite | path |
|---|---|---|
| SC-1, SC-2 (tap → one create; per-degree differences; empty outcome) | composer component tests | `web/tests/sheet/spell-library-compose.test.js` |
| SC-3 (four target states in one render), SC-4 (live flags) | component + conflicts read stub driving real fan-out mutation | `web/tests/sheet/spell-conflicts.test.js` |
| SC-5 (defect classes D1–D8, each its own fixture; green: empty file, empty degree list) | loader unit tests | `src/spellbook/tests/*` |
| SC-5 CI leg | `just spellcheck` (the boot-equivalent load assertion) | `justfile` + the `gate` workflow step |
| SC-6 (empty library, P0 composer unchanged) | the **existing** composer suite, unedited, run at the PR head | existing `web/tests/sheet/composer*` |
| SC-7 (no computation outside the engine) | named grep sweep over the epic's new client files + line-by-line diff review | PR body |
| SC-8 (GM renders no write affordance) | component test, scoped region query | `web/tests/sheet/spell-library-gm.test.js` |
| SC-9 (client shows what the server read; data-only change needs no rebuild) | REST read contract test + a fixture-swapped server test | `src/tests/spellbook_rest.rs` |
| Pre-warn verdicts incl. blanket, per-instance, penalty-vs-bonus, tie, `+0`, no-instance | engine unit tests (the delta rule against shipped `stack_one`) | `engine/src/tests/preconflict.rs` |
| End-to-end: cast a fixture spell through the real router → effect row → fan-out → target's sheet | integration | `src/tests/spellbook_flow.rs` |

**Two-sided assertions required** (AGENTS.md style): two degrees differing → two results (not
one degree asserted twice); flagged target vs unflagged target in the same render; penalty-vs-
bonus no-flag **and** same-type flag; library-empty vs library-loaded; `+0` vs non-zero; instance
present vs absent (threshold sides, not one side).

## D10 — File map (the whole diff surface of the code half)

**New** — `src/spellbook/{mod,model,seed}.rs`, `src/spellbook/tests/*`;
`engine/src/preconflict.rs`, `engine/src/tests/preconflict.rs`;
`data/seed/spell-library.json`; `web/src/lib/sheet/{spell-library,conflicts}.js`;
`web/src/lib/sheet/components/SpellLibrarySection.svelte`;
`web/tests/sheet/{spell-library-compose,spell-conflicts,spell-library-gm}.test.js`;
`src/tests/{spellbook_rest,spellbook_flow}.rs`.
**Touched** — `src/http.rs` (two routes), `src/engine_host/rest.rs` (the two handlers, or a
`spellbook` handler module if it reads cleaner — either is fine, the gate is "no new pattern"),
`src/lib.rs` / `src/main.rs` (module registration, boot-time load), `engine/src/lib.rs` (the new
pure module), `web/src/lib/sheet/components/EffectsComposer.svelte` (mount the section; pass the
conflicts state; **no change to the freeform form's fields or validation**), `web/src/lib/sheet/
SheetView.svelte` or `state.js` (the two reads' state wiring), `justfile` (the `spellcheck`
recipe), `.github/workflows/gate.yml` (one step). `specs/016-seeded-spell-library/**` is this
PR's own surface.

Anything a task wants outside this list stops and flags — in particular the engine's `model.rs`,
`stack.rs`, `compute.rs` and `vocab.rs` are **read-only consumers** here (D6 adds a caller, not a
change), as are `src/sync/write.rs`, the E9 custom-entry path, and the P0 composer's freeform
branch. **The read-only claim is what forces D6 step 2 through the public `stack::candidates`:**
`fan_out` is private, so the direct-call reading of that step would require editing `stack.rs`
and would contradict this paragraph. The shadow-`ActiveEffect` reading is the one that keeps both
true.

## D12 — Implementation lane split (so lanes open the moment content lands)

The code half is one lane, because its parts are one coherent billet and the seams are thin; the
content half is a second lane that never touches code; the tail is follow-ups. This is the split
I would run, and the sequencing rationale — not a claim that a seat is assigned today.

| lane | scope | depends on | owner |
|---|---|---|---|
| **L1 — Engine pre-warn** | D6's pure function + its unit suite (blanket, per-instance, penalty, tie, `+0`, no-instance). Lands first: everything else consumes its verdict, and it is the sharpest correctness edge in the epic. | nothing (no content, no UI) | one engineer |
| **L2 — Loader + read** | D1–D2 + D4's two routes + `just spellcheck` + the D1–D8 fixture set + `data/seed/spell-library.json` (empty). | nothing; parallel with L1 | one engineer |
| **L3 — Composer section + picker flags** | D7's UI, the tap flow, the conflict read wiring, GM posture, SC-1..SC-4, SC-6..SC-9 sweep. | L1 + L2 (sequential) | one engineer |
| **L4 — Content** (Josh's list, Dave's templates) | a data-only PR against `data/seed/spell-library.json`; the load check is the reviewer's first line of defence; Josh reviews. | L2 (the schema must exist to fill it; a *minimal* config is enough to start) | **Josh + Dave**, transcribed by a seat if they hand over prose |
| **L5 — Post-content smoke** | SC-10 walkthrough with the real list, and the ~3 fixes it surfaces. | L3 + L4 merged | one engineer, small |

L1 ∥ L2, then L3. **L4 is not a blocker for L1–L3** — that is the whole point of the split, and
it is what makes this lane safe to start while two external people are elsewhere. If L4 never
lands, the epic is still a net win: the conflict pre-warn (US-2) is independent of library
content and ships value for freeform casting alone, which is worth naming — **the pre-warn is the
part of this epic that delivers whether or not the content ever arrives.**

## Self-review

- **Numbering note:** this doc has **no D11**. D0–D10 are the design decisions, and **D12** is the
  lane split; the number 11 is skipped, and the skip is recorded here rather than renumbered, so
  review threads and plan references written against `D12` keep their meaning.
- **Every spec FR maps to a decision**: FR-1/2→D1, FR-3→D10/D12 (no seat authors content; L4 is
  Josh+Dave), FR-4→the contract doc, FR-5/6/7→D7's tap, FR-8→D7 (`self`/`chosen`, no widening
  from data), FR-9→D7 (no deduction; the fork named), FR-10→D3/D7 + SC-6, FR-11/12→D5/D6,
  FR-13/14→D2 + SC-5, FR-15→D4, FR-16→D7, FR-17→D3, FR-18→D7/SC-8, FR-19→D7's rejected
  alternative.
- **Every SC maps to a named suite** (D9), SC-7 honestly labelled directional (the E14 SC-6
  discipline), SC-10 explicitly owned by the content PR.
- **No engine change** beyond one new pure module that consumes existing functions; **no new wire
  kind**; **no migration**; **no P0 test modified**. Those four claims are the epic's safety
  argument and each is checkable in the diff.
- **Two things I would want a reviewer to push on**: D5's one-read-per-picker-change (is the
  latency acceptable at the table, or should the composer prefetch on spell-select?) and D6's
  sentinel-id tie-break claim (is monotonic `effect_id` actually guaranteed by the shipped
  schema? — the plan makes that assertion testable rather than assumed).

## Assertion ledger — the design's checkable claims

Six claims above are load-bearing, and each is phrased so a reviewer can re-run it instead of
believing it. Rows 1–4 are certified by this PR's own `gate` check-run at the PR head; the
conclusion and check-run id are pasted in the PR body — that is the certificate, and no count
here is a substitute. Rows 5–6 are checkable in the diff and in the file itself.

| # | claim | command that proves it |
|---|---|---|
| 1 | The engine's vocabulary is reachable from `src/`, so **D2**'s validation can call the engine's own parser rather than a copy of its string table | `grep -n "pub use hireling_engine::vocab" src/import/seed.rs` (the shipped re-export) and `grep -n "^pub mod vocab\|fn parse" engine/src/vocab.rs` (no `^` on the second pattern: the `parse` functions sit inside `impl` blocks, so they are indented — `Stat::parse` at `vocab.rs:318`, `ModifierType::parse` at `:279`) |
| 2 | The precedent **D1**/​**D2** copy is real: a checked-in seed JSON, embedded, validated at load, fatal on a defect | `ls -l data/seed/condition-tiers.json` and `grep -n "include_str!\|SEED_PATH" src/import/seed.rs src/import/mod.rs` |
| 3 | The pre-warn's inputs are shipped, not invented: one stacking core exists (**D6**'s premise), and its verdict vocabulary is already a real type the response can quote | `grep -n "pub fn candidates\|pub fn stack_one" engine/src/stack.rs` (the two stackings at `:58` and `:158`) and `grep -n "pub enum SuppressionReason" engine/src/model.rs` (`model.rs:207` — `stack.rs` only imports the type, so grepping it there returns nothing) |
| 4 | Blanket/instance handling is reusable rather than re-implemented, and `GET /api/parties/{id}/spells` does not already exist so **D4** is one new route in the established shape | `grep -n "pub fn expand\|BLANKET_EXPANSIONS" engine/src/vocab.rs`; `grep -rn 'parties/{party_id}' src/http.rs src/engine_host/rest.rs` then `grep -rn "spells" src/` (no `/spells` arm — the shipped routes use **brace** syntax, `src/http.rs:97-132`, not `:param`) |
| 5 | **No P0 composer test needs editing** for **D10**'s empty-library state (SC-6's premise, checkable only in the implementation diff) | at implementation time: `git diff --stat origin/main -- web/tests` names no existing composer suite |
| 6 | Sentinel-id tie-break (**D6**) rests on a shipped fact, not an assumption | `grep -rn "effect_id" src/sync/write.rs migrations/*/…*.sql` — confirm ids come from a sequence; **D6**'s wording never names a winner, so the design is safe even if that ever stops holding; the assertion is pinned by a test in the plan, not by this doc |

Rows 1–4 return text today; if any comes back empty the design's premise is wrong and **D0** is
the line that has to change, not the wording of the claim. The commands are written as shipped at
this design's head, so two of them are deliberately imprecise and each says so: row 6's
`migrations/*/…*.sql` glob is a placeholder for whichever migration allocates effect ids (the
plan turns it into an assertion rather than a hunt), and row 4's route listing is a grep over the
two files that own routing today — if a third appears, the check is "no `/spells` arm anywhere",
which `grep -rn "spells" src/` answers.

**And the same ledger rule applies to the pipeline's own outputs** (AGENTS.md, the warning-count
clause): the figures a reader might otherwise take from this doc's word for it, with the command
that produced each — the **`gate` check-run's conclusion and warning count at the PR head
belong in the PR body, not here**: a check-run is a server-side fact about a pushed head, and a
count written into a doc before the push is a number nobody can re-run.

| figure | command that produced it |
|---|---|
| 17 tracked JSON files (the hook's "17 file(s), clean") | `git ls-files '*.json' \| wc -l` |
| 19 functional requirements | `grep -c "^- \*\*FR-" spec.md` |
| 10 success criteria | `grep -c "^- \*\*SC-" spec.md` |
| 21 acceptance scenarios, 17 edge cases, 6 assumptions, 4 user stories | `grep -c "^[0-9]*\. \*\*Given" spec.md`; `sed -n '187,206p' spec.md \| grep -c "^- "`; `sed -n '207,215p' spec.md \| grep -c "^-"`; `grep -c "^### User Story" spec.md` |
| 9 defect-class rows: D1–D8, which must each **fail** the load, plus D9, which is an **accepted** case (plausibility is Josh's review, not a lint) | `grep -c "^\| D[0-9]" contracts/spell-library-seed.md` → 9 |

The `sed -n` ranges in the scenario row are section-bounded counts, so they move if the spec's
sections move; the file counts and the defect-class count do not. Every figure above is cited in
[`checklists/requirements.md`](checklists/requirements.md) or in this doc's **D9**, and none is a
claim about CI. A gate run's own warning count is not in this table on purpose: it is read at the
pushed head and named in the PR body, which is where a certificate belongs.
