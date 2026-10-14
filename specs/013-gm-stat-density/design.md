# Design: GM Stat Density — Party-Card Stat Blocks (E14)

**Epic**: E14 — Phase 3, P1 · GitHub issue #16
**Created**: 2026-10-16 · **Signed**: Korrin, 2026-10-16 — design internal to
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
byte-equal on spell DC (one extracted helper), and what the tests pin.

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
key-skills line (Task 4's markup), skeleton rows when `view` is null. Tiles
carry their `provenance` — E6's hover, free.

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

**Rejected**: leaving StatsPane's inline find and copying it — two call
sites, one rules choice, and the equality test can only catch drift after it
ships; a generic "first by predicate" helper — abstraction for its own sake.

## D6 — Loading and absence states

`view` null → the whole block is skeleton rows (spec FR-6), mirroring the
card's existing bar skeleton. Caster-list empty → no spell-DC tile.
`class_dc` null → no class-DC tile. `skills` shorter than three → render
what exists. No entry ever renders `—` or `0` for an absent stat — absence
is absence (E8 contract §1's null rule, displayed honestly).

## D7 — Markup, CSS, accessibility

Hand-rolled CSS in the component (no library — Article V). The card stays
one `<button>`; the stat block is divs and spans inside it — no nested
interactive elements (invalid HTML and an accidental second tap target).
Tiles carry `aria-label`s through `StatTile`'s existing structure; the
key-skills rows render as a definition-style list with
`<dt>/<dd>` (name / modifier) — readable by text, assertable by role.

## D8 — Test layout (where each SC lands)

- `web/tests/engine/key-skills.test.js` — comparator unit tests: SC-4's
  tie-break fixtures (rank beats name; name ascending; re-selection on
  changed totals; fewer-than-three; lore-vs-core tie; the two-fixture
  boundary style throughout).
- `web/tests/engine/caster.test.js` — `primaryCaster`: non-innate preferred,
  innate-only fallback, empty → null (sheet parity: StatsPane's existing
  tests keep passing after the extraction).
- `web/tests/party/stat-block.test.js` — SC-1, SC-2, SC-3, SC-7: mount with
  fixture; assert by label; mutate fixture fields (`ac.total`, a skill
  total, empty casters) and re-assert; sheet-equality via the same fixture
  through `StatsPane`; null-view skeleton.
- `web/tests/party/roster-card.test.js` (existing, extended) — SC-5: role
  query — the card's only button is the drill-in; stat block adds none.
- Implementation PR — SC-6: the FR-3 grep sweep, command and output pasted.

## File map (the whole diff surface)

```
web/src/lib/engine/caster.js          (or partition.js)  — primaryCaster   [D5]
web/src/lib/engine/key-skills.js                          — keySkills      [D4]
web/src/lib/party/StatBlock.svelte                       — the unit       [D2]
web/src/lib/party/RosterCard.svelte                      — mount point    [D2]
web/src/lib/sheet/components/StatsPane.svelte            — switch to primaryCaster [D5]
web/tests/engine/{caster,key-skills}.test.js             — unit tests     [D8]
web/tests/party/{stat-block,roster-card}.test.js         — component tests [D8]
```

No engine, no src/, no wire, no schema. If a task wants any of those, stop —
wrong task.
