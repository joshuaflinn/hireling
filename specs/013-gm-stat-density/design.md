# Design: GM Stat Density — Party-Card Stat Blocks (E14)

**Epic**: E14 — Phase 3, P1 · GitHub issue #16
**Created**: 2026-10-14 · **Revised**: 2026-10-14 (review rounds 1–4 on PR #197) · **Signed**: Korrin, 2026-10-14 — design internal to
one epic's P0/P1 scope, signed per the SDD pipeline's gate rules. Moves no
P0/P1/P2 line; the one cross-line fact (implementation merge hold until
gh#13 closes) is Thrane's standing constraint, carried in the plan.
**Consumes**: spec.md (this directory, same date); E8 engine-output contract
(binding); E10's roster surface and `state.js` seam; E6's `StatTile`,
`Provenance`, `HpBar`, `format.js`, `partition.js`.
**Constraint carried from the task**: read-only epic — no engine change, no
wire change, no backend change. Everything below is `web/` presentation over
data that already exists per card.

---

## 1. What the design decides

Where the new numbers come from (one store, already there), how the stat
block is composed (one new unit, E6 tiles reused), how key skills are
selected (one pure helper, spec FR-2), how the sheet and the card stay
byte-equal on spell DC (one extracted helper, pinned by the ordering the
reference fixture cannot express), how the sheet's class-DC dash comes back
inside its own contract (one conditional render), and what the tests pin.

## D1 — Source: the roster state's existing per-card `view`

The card's engine output is the `view` store `web/src/lib/party/state.js`
already builds (`views` map, `sync.derived(characterId)`, refreshed on
`fields` + `derived` events). The stat block adds **zero fetching, zero
state, zero stores**: `RosterCard` already receives `card`; the state layer
exposes the card's `view` (it currently exposes `hpMax` and `effects`
derived from it — the same `$derived` pattern extends to the stat fields, or
the component derives from `card.view` directly — Task 3 picks the cheaper
shape; both are the same store, which is the point).

**Rejected**: a per-card `GET /api/characters/{id}/derived` fetch — a second
path (spec FR-3 forbids), plus six requests the sync already answers;
computing anything in the state layer — same fault, one layer down.

## D2 — Composition: one new unit, `StatBlock.svelte`, E6 tiles inside

`web/src/lib/party/StatBlock.svelte` — props `{ view }` (the engine output
or null). Inside: `StatTile` per spec FR-1 entry (AC, Fort, Ref, Will,
Perception, Init, Spell DC, Class DC — the last two conditionally), the
key-skills line (Task 3's markup, Task 4's CSS), skeleton rows when `view`
is null. Tiles carry their `provenance` — E6's hover, free.

`RosterCard` mounts it between the HP bar and the effects strip.

**Rejected**: growing `RosterCard` inline — the card is already five units
composed; the stat block is a coherent, testable unit (Article II: locality,
not fewer files); a shared "compact stats grid" abstraction for sheet + card
— the sheet's pane and the card have different density, notes, and layout;
premature.

## D3 — Initiative: label over `derived.perception`, nothing else

Spec FR-4: the Init entry is `derived.perception.total` with perception's
provenance, labeled `Init`. No new stat, no vocabulary change, no PB field
invention.

**Rejected**: adding `initiative` to `SingleStat` — an engine-contract
change (E8's vocabulary is binding and closed), ripple through `vocab.rs`,
rejection tests, and the wire docs, all to display a number that already
exists as Perception. Out of this epic's read-only scope by its own guardrail.

## D4 — Key skills: one pure helper, comparator per spec FR-2

`keySkills(skills, count = 3)` in `web/src/lib/engine/key-skills.js` (pure
lib, components render, libs compute and test — the house rule from
`partition.js`). Sort by `total` desc, then `rank` desc, then `name` asc;
slice to three; return the picked rows (with the display name resolved: lore
rows through the `label`-fallback convention E6 owns in `partition.js` —
Task 2 decides whether it calls `partitionSkills`' lore branch or mirrors
its two-line fallback; either way the *convention* is E6's, not new).

Stable and locale-free by construction: the comparator touches only wire
fields; `Array.prototype.sort` in V8 is stable, and the comparator is a
total order anyway (names unique per E8 contract) — stability never matters.

**Lore-label parity is pinned, not assumed**: the two-line fallback mirrors
E6's `partition.js` convention, and mirroring invites drift — so a parity
test runs the same lore rows through `partitionSkills`' lore resolution and
`keySkills`' display resolution and expects identical labels (spec M3
finding; the test is the guard, not the abstraction).

**Rejected**: doing the sort inline in the component — untestable at the
comparator level (the tie-break fixtures are the point, SC-4); a settings
knob for "how many key skills" — the PRD fixed three; a knob is speculative.

## D5 — Spell DC selection: extract `primaryCaster`, sheet and card share it

The sheet's StatsPane already picks its spell-DC block with
`casters.find((c) => !c.innate) ?? casters[0] ?? null`. That is a rules
flavored choice (innate blocks rank below tradition blocks); two copies of
it in two consumers is exactly the drift the sheet-equality criterion (SC-3)
guards. Extract `primaryCaster(casters)` into `web/src/lib/engine/`
(`key-skills.js`'s sibling — `caster.js`, or fold into `partition.js`;
Task 1 picks the file, both are pure-lib homes), StatsPane switches to it,
the stat block calls it.

**The extraction's regression guard is the ordering the reference fixture
cannot express**: `engine_output_reference.json` orders casters
non-innate-first (`[Wizard, Wellspring Gnome]`), an ordering on which
`find((c) => !c.innate)` and `casters[0]` return the same block — a
degraded extraction passes silently. The guard is a fixture ordered
`[innate, non-innate]`: `caster.test.js` pins it at unit level, and a
StatsPane-level test with the same ordering pins it at render level (the
sheet-side guard the extraction previously lacked — round-1 finding F6).

**Rejected**: leaving StatsPane's inline find and copying it — two call
sites, one rules choice, and the equality test can only catch drift after it
ships; a generic "first by predicate" helper — abstraction for its own sake.

## D6 — Loading and absence states

`view` null → the whole block is skeleton rows (spec FR-6), mirroring the
card's existing bar skeleton. Caster-list empty → no spell-DC tile.
`class_dc.base` null → no class-DC tile **on the card, and none on the
sheet**: this epic aligns StatsPane with contract §1 — its unconditional
dash tile (`StatsPane.svelte:112`, `class_dc.total ?? '—'`) is the
merged-tree defect spec FR-1 names, and the sheet's tile becomes
conditional on a non-null base (plan Task 1b). `skills` shorter than
three → render what exists. No entry ever renders `—` or `0` for an absent
stat — absence is absence (E8 contract §1's null rule, displayed honestly,
on both surfaces).

## D7 — Markup, CSS, accessibility

Hand-rolled CSS in the component (no library — Article V). The card stays
one `<button>`; the stat block is divs and spans inside it. **The effects
strip already nests `role="button"` ConditionTip spans inside the card
button** (`RosterCard.svelte` mounts `EffectsStrip` inside `<button>`), so
"the card has no nested interactive element" is not available as a claim —
and neither, since round 2 (B1), is "the region holds no interactive role
at all": FR-1 puts the provenance hover on every entry, and `Provenance`
is focusable by design (`tabindex="0" role="note"` on any non-empty
`applied`/`suppressed`, with a nested ConditionTip `role="button"`) — the
same read-only affordance the sheet's tiles already carry. The stat
block's own region is: its root renders as a labelled group
(`role="group"` + `aria-label="Stats"`), its focusable set is exactly
those provenance affordances, and it adds no control of its own — nothing
that mutates state. SC-5's test asserts exactly that region, on SC-8's
non-empty-`applied` fixture, so the affordances are present to be
counted. **Tile labels are visible text, not ARIA names** (round 4 B1):
`StatTile` renders `label` as a plain `<div class="k">` and forwards no
attributes — its props are destructured with no rest element
(`StatTile.svelte:9`) and nothing spreads onto the root div (`:12`), so
an `aria-label` on a `<StatTile>` invocation would be dropped, never
reaching the DOM. This design renders tiles through `StatTile` **as-is**:
SC-1's queries are text queries on the visible `<div class="k">`, the
group root alone carries `aria-label="Stats"` in `StatBlock`'s own
markup, and `StatTile.svelte` stays out of this epic's file map — no
template change, no code file enters the diff. The key-skills rows
render as a definition-style list with `<dt>/<dd>` (name / modifier) —
readable by text, assertable by role.

## D8 — Test layout (where each SC lands)

- `web/tests/engine/key-skills.test.js` — comparator unit tests: SC-4's
  tie-break fixtures (rank beats name; name ascending; re-selection on
  changed totals; fewer-than-three; lore-vs-core tie; the two-fixture
  boundary style throughout) **plus the lore-label parity test against
  `partitionSkills` (D4)**.
- `web/tests/engine/caster.test.js` — `primaryCaster`: **the discriminating
  ordering first — `[innate, non-innate]` picks the non-innate (the case
  `casters[0]` gets wrong and the reference fixture cannot express)**;
  non-innate-first; innate-only fallback; empty → null (sheet parity:
  StatsPane's existing tests keep passing after the extraction).
- `web/tests/party/stat-block.test.js` — SC-1, SC-2, SC-3, SC-7, SC-8:
  mount with fixture; assert by visible label text (D7: text queries on
  `StatTile`'s `<div class="k">`); **mutate every FR-1 entry's backing
  field one at a time** (spec FR-3 stmt 3's table — `ac.total`, saves,
  perception, `casters[0].spell_dc.total`, `casters[0].spell_attack.total`,
  `class_dc.total`, `class_dc.base` → null, `casters` → `[]` (entry and
  attack note disappear), a skill
  total, `hp_max`) and re-assert; sheet-equality via the same fixture
  through `StatsPane`, **absence included**; **provenance hover two-fixture
  (empty `applied` → no affordance; non-empty → breakdown on both
  surfaces)**; null-view skeleton.
- `web/tests/party/roster-card.test.js` (existing, extended) — SC-5:
  **scoped query** — card mounted on SC-8's non-empty-`applied` fixture
  with effects present, stat-block group located by role/name, every
  focusable/interactive element inside it a provenance affordance
  (`role="note"` span or its nested ConditionTip) and no `<button>` or
  form control beyond them; card-level drill-in posture asserted for what
  it proves (FR-5).
- Sheet suites — Task 1b's class-DC alignment: a null-`class_dc.base`
  fixture renders no Class DC tile through StatsPane (suite that mounts it:
  `web/tests/shell/components.test.js` or a focused new one — seat's
  pick); Task 1's innate-first spell-DC render guard lands here too.
- Implementation PR — SC-6: the FR-3 sweep, command and output pasted,
  scope and blind spots named, compensating diff review stated.

## File map (the whole diff surface)

```
web/src/lib/engine/caster.js          (or partition.js)  — primaryCaster   [D5]
web/src/lib/engine/key-skills.js                          — keySkills      [D4]
web/src/lib/party/StatBlock.svelte                       — the unit       [D2]
web/src/lib/party/RosterCard.svelte                      — mount point    [D2]
web/src/lib/sheet/components/StatsPane.svelte            — switch to primaryCaster [D5]
                                                        + class-DC tile conditional [D6, Task 1b]
web/tests/engine/{caster,key-skills}.test.js             — unit tests     [D8]
web/tests/party/{stat-block,roster-card}.test.js         — component tests [D8]
web/tests/sheet|shell (StatsPane-rendering suite)         — alignment + innate-first guard [D8]
```

No engine, no src/, no wire, no schema. If a task wants any of those, stop —
wrong task.
