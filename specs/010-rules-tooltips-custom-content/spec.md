# Feature Specification: Rules Tooltips + Custom Content Entry (E9)

**Epic**: E9 — Phase 1, Lane A, P0 · depends on E4 (corpus) + E6 (sheet UI); consumes E8 seams already merged (picker REST, effect apply) · blocks E11 (go-live) · GitHub issue #11
**Created**: 2026-10-20
**Status**: Specify gate run 2026-10-20 (Korrin, front half of the SDD pipeline — specs only, no code in this pass); clarify answers folded same day from the existing corpus (see *Clarify Log*); design signed by Korrin 2026-10-20
**Input**: `docs/EPICS.md` Epic E9 (specify prompt + Constraints + AI Guardrails); PRD v3.6 FG1 (rules tooltips, custom content entry, entry-flow edge cases) + FG2 (offline rules) + Rules Corpus & Data Sources section + Reference Prototype freeze; `CONSTITUTION.md` Articles I/II/IV; E4 `specs/004-rules-corpus-importer/` (corpus row contract, lane law); E6 `specs/006-live-sheet-ui/components.md` (the reuse contract); E8 `specs/008-buff-effect-engine/` (picker REST, effect apply, `corpus_entry_id`); E7 `specs/007-party-sync/contracts/wire-protocol.md` (what syncs and what does not); the frozen prototype `docs/reference/lorum_ipsum_dashboard.html` (seed prose, interaction baseline, `setHTML` discipline)

---

## User Scenarios & Testing *(mandatory)*

E9's users are the six party accounts. It ships two capabilities: every
condition explains itself where it renders (tooltips), and the party can
put its own content into the system at the point of use (custom rows). It
adds no sync semantics, no engine math, and no import behavior of its own;
structure comes from the corpus, prose from the curated seed, custom rows
from the party.

### User Story 1 — "What does that condition do again?" (Priority: P0) 🎯 MVP

Becky's sheet shows a `Frightened 2` chip. She hovers it: a popup with
paraphrased rules prose — *Status penalty equal to the value on all checks
and DCs. The value drops by 1 at the end of each of the creature's turns.*
— a `Player Core p. 444` cite, and an Archives of Nethys link. The prose
mentions *off-guard-like* nested condition names; those are links; clicking
one swaps the popup to that condition. Clicking the chip pins the popup so
it stays while she reads; Esc or a click elsewhere closes it. The same
thing works on every surface a condition name renders: sheet chips, party
roster chips, the condition picker, and the provenance breakdown.

**Why this priority**: US-3 — the product's founding friction ("never ask
what does that condition do again — one hover away"). The PRD names this
the P0 tooltip scope: every Player Core condition.

**Independent Test**: Mount the chip surface with an effect named
`Frightened`; fire `mouseenter` and assert the popup shows the paraphrase,
the page cite, and the AoN href; fire `click` and assert pinning (popup
survives `mouseleave`); fire `keydown Escape` and assert it closes. Mount
a second fixture with an effect name that has no curated prose and assert
the fallback badge renders — two fixtures, different results.

**Acceptance Scenarios**:

1. **Given** any rendered condition name with curated prose, **When** the
   pointer hovers it (or it receives keyboard focus), **Then** a popup
   renders: paraphrased prose, `Player Core p. N` cite, and an AoN link
   (`https://2e.aonprd.com/Conditions.aspx?ID=N`).
2. **Given** an open (hover) popup, **When** the user clicks the trigger,
   **Then** the popup pins — it no longer closes on pointer-out; Esc or a
   click outside closes it.
3. **Given** prose containing other condition names, **When** rendered,
   **Then** those names are links; clicking one swaps the popup to that
   condition's prose (second layer); the link text is never mangled.
4. **Given** a condition name with no curated prose (corpus row outside
   the freeze, or a freeform effect name that matches nothing), **When**
   hovered, **Then** a minimal popup renders — name, badges, "no
   paraphrase yet" — never a crash, never invented rules text.
5. **Given** a custom condition applied to a character, **When** its chip
   is hovered, **Then** the popup shows the creator's one-line description
   and the `custom` badge.

### User Story 2 — The 500 Toads moment: custom spell at the composer (Priority: P0)

Bear's player wrote a homebrew spell, *Conjure Toad Swarm*, that no corpus
will ever contain. At the caster's spell surface (the spellbook/prepare
flow), she taps "Add custom spell", fills minimal fields — name, rank,
one-line description — and it lands: listed alongside known spells,
preparable into a slot by name, badged `custom`, visible to every party
member's composer. If it someday needs engine math, that goes through the
freeform effect composer as a separate effect — the spell row itself
carries no math.

**Why this priority**: The PRD's own bar — "'500 Toads'-class content must
be representable at the first session, or the caster seat fails for the
party's actual sheet."

**Independent Test**: Render the spellbook surface with the custom-rows
store seeded; assert the custom spell renders with the `custom` badge and
a Prepare affordance that commits its name. Two fixtures: store without
custom rows (no badge section) vs with one (badge + preparable row).

**Acceptance Scenarios**:

1. **Given** the caster's spell surface and a character owner at the
   controls, **When** they open "Add custom" and submit
   `{name, rank, description}`, **Then** the form validates (name
   required, ≤ 64 chars; description ≤ 280 chars; rank 0..10) and the row
   surfaces immediately in the composer, badged `custom`.
2. **Given** a custom spell exists, **When** any party member's composer
   renders, **Then** the spell is listed and preparable into a rank
   slot — the prepare write is the existing slot write by name, nothing
   new.
3. **Given** invalid input (over-cap name, blank name, rank 11), **When**
   submitted, **Then** inline validation names the field and the reason;
   no request leaves the client; the server re-validates and rejects with
   the same reasons (defense in depth).

### User Story 3 — Custom item at the point of use (Priority: P0)

Josh's character loots a named wagon no import will ever have. In the
inventory panel he taps "Add custom item", fills `{name, description}`,
and the item appears in **his character's inventory** immediately — a real
row, quantity-adjustable from then on like any item, badged `custom` in
the item context. The party sees it through his sheet (cross-member
read), and the row exists in the corpus for E12's book-value future.

**Independent Test**: Render the inventory panel editable; fire the
add-custom flow with valid fields; assert the optimistic row renders with
the badge; fire the qty control against it and assert the normal `inv`
write targets the new name. Cap-violating fixture asserts the inline
error.

**Acceptance Scenarios**:

1. **Given** the inventory panel and the character's owner at the
   controls, **When** "Add custom item" submits valid fields, **Then** a
   `custom`-lane corpus row is created AND an inventory row (qty 1)
   exists for that character, surfacing immediately, badged `custom`.
2. **Given** the custom item row exists, **When** its quantity is
   adjusted, **Then** it rides the existing `inv` write path by exact
   name — no special inventory handling.
3. **Given** a non-owner (another member, or the GM) views the sheet,
   **When** the inventory renders, **Then** the item shows read-only, as
   inventory already does — no new permission surface.

### User Story 4 — Custom condition at the picker (Priority: P0)

The table invents a house condition, *Sunlit* — no math, just a reminder.
Anyone opens the condition picker, taps "Add custom condition", fills
`{name, description}`, and it joins the picker for the whole party,
badged `custom`. Applying it lands a chip with the tracked-manually badge
— display/tracking, zero engine math — exactly like a display-only
corpus condition, and hovering the chip shows the creator's description.

**Independent Test**: Through the real router, POST a custom condition;
assert `GET /api/parties/{id}/conditions` includes it with
`lane: "custom"`, `tier: "display_only"`, `valued: false`; apply it via
the existing effect-create frame; assert the chip carries
`tracked_manually` and zero modifiers.

**Acceptance Scenarios**:

1. **Given** the condition picker, **When** listing conditions, **Then**
   custom rows appear alongside corpus rows with the `custom` badge —
   same list, same search, no separate section.
2. **Given** a custom condition applied to a character, **When** the
   sheet renders, **Then** its chip carries both badges — `custom` and
   `tracked` — and no number anywhere moves.
3. **Given** the creator revisits their row, **When** they edit its
   description, **Then** the edit lands (creator is sole writer). Given
   anyone else, **When** they attempt the edit, **Then** 403, an audit
   `forbidden_custom_write` row is written, and the client surfaces the
   refusal inline.

### User Story 5 — The prose is safe, sourced, and licensed (Priority: P0)

All rules prose — curated paraphrases, custom descriptions, the license
notice — renders through an inert HTML setter: DOMParser, no script
execution, no event-handler attributes, no `javascript:` URLs; nested
condition links are inserted into text nodes only, never into markup.
The paraphrase corpus ships under the Paizo Community Use Policy / ORC
notice, extended to name the curated-prose lane, and the app renders that
notice in an about view.

**Why this priority**: The AI guardrails are acceptance criteria, not
advice — inert rendering and length caps are the attack surface for a
party-writable text lane; the notice file is a settled constraint of this
epic.

**Independent Test**: Unit-drive the inert setter with hostile fixtures
(`<script>`, `<img onerror>`, `<a href="javascript:…">`); assert no
script node survives and no handler fires; render a tooltip whose prose
contains the same fixtures through the mounted component and assert the
same. Integration-test the custom-create endpoint at the caps and beyond.

**Acceptance Scenarios**:

1. **Given** any prose-carrying popup, **When** it renders, **Then** the
   prose passed through the inert setter — `<script>` content does not
   execute and does not appear as a script node; `onerror`-style
   attributes do not survive; links are `https:`-only.
2. **Given** the custom form, **When** input exceeds caps, **Then** both
   client and server reject with the field and reason named.
3. **Given** the deployed app, **When** the about view opens, **Then**
   it renders the repo's `NOTICE.md` content, including the
   curated-prose (CUP) lane, through the same inert setter.

---

## Functional Requirements

- **FR-1 — condition tooltips, everywhere conditions render**: Condition
  names on sheet chips, roster chips, the condition picker, and the
  provenance breakdown MUST render as hoverable/focusable and
  click-to-pin popups with paraphrased prose, a page cite, and an AoN
  link wherever the curated seed provides them. Esc and click-outside
  close; nested condition links swap the popup (second layer).
- **FR-2 — curated prose coverage**: The curated seed MUST cover every
  Player Core condition as of the prototype freeze — the 42-entry map,
  ported from the frozen prototype (paraphrase, page, AoN id, link
  flag). Coverage is asserted by test against the pinned corpus.
- **FR-3 — inert rendering**: ALL prose (curated, custom descriptions,
  license notice) MUST render through a shared inert HTML setter
  (DOMParser, no script execution, no inline handlers, no
  `javascript:`/non-`https:` URLs). Condition-name linkification touches
  text nodes only. Svelte `{@html}` is banned for prose.
- **FR-4 — custom entry at three points of use**: "Add custom"
  affordances MUST exist at the item context (inventory panel), the
  caster's spell surface, and the condition picker. The form takes
  minimal fields (name; level/value where applicable; one-line
  description), enforces caps (name 1..64 chars; description 0..280
  chars; spell rank 0..10; condition value 1..20 optional), creates a
  `custom`-lane row that surfaces immediately where created, and joins
  the relevant picker for the whole party, badged `custom`. A custom
  condition's optional value renders as a display note in its tooltip
  body and picker row — never an apply input (FR-6).
- **FR-5 — creator-owned write model**: Any character owner in the party
  may create; the creator is the row's sole writer (edit path); the GM
  is read-only (no create, no edit); non-creator writes are refused with
  403 AND an audit event `forbidden_custom_write` (enum already exists).
  Dave is curation lead, not a gate — no approval step exists.
- **FR-6 — custom rows are display/tracking entries**: Applying a custom
  condition lands `tracked_manually` with zero modifiers via the
  existing E8 apply path (NULL mappings ⇒ display-only — no new apply
  code). Custom spells prepare by name via the existing slot write.
  Homebrew engine math goes through the freeform effect composer —
  never through custom rows.
- **FR-7 — the condition picker surface**: E9 ships the picker surface
  itself (E8 shipped the REST + apply frames only; no picker UI exists
  in the tree): a dialog listing `GET /api/parties/{id}/conditions`
  rows with tier/lane/valued badges, a value input for valued
  conditions, apply through the existing effect-create write, and the
  "Add custom condition" affordance. Owner-gated (no GM apply).
- **FR-8 — importer isolation (corpus importer)**: The **E4 corpus
  importer's** re-runs MUST never touch `custom` rows — structurally
  guaranteed (custom rows carry no `source_id`, so they sit outside the
  upsert index) and asserted by an integration test that runs a corpus
  import with a custom row present. The **Pathbuilder importer** is a
  separate path (Edge Cases): it never writes corpus rows, but a custom
  item surfaces as a kept-delta notice in every pbimport diff — accepted
  review noise, not a mutation.
- **FR-9 — licensing**: `NOTICE.md` MUST name the curated-paraphrase
  lane (Paizo Community Use Policy, consistent with ORC/OGL as
  archived), and an in-app about view MUST render the notice through
  the inert setter.
- **FR-10 — offline tooltips**: Tooltips MUST render offline — the
  curated seed is a build-time static module (no runtime fetch), so the
  SW-cached shell carries it (E7/E10 offline contract; US-3 at the
  table).

## Constraints & Guardrails (from gh#11 — binding)

- Paraphrased rules text ships under the Paizo Community Use Policy /
  ORC notice in-repo; the notice update ships in this epic (FR-9).
- The frozen prototype is display-prose seed only — never a structured
  data source. Structure comes from the corpus import (E4). The seed is
  ported verbatim as prose; nothing joins by prototype ids.
- Inert HTML setter for all prose (FR-3). Input validation + length caps
  on custom forms (FR-4). Tooltip content read-only for non-creators at
  P0; curation editing of imported prose is E15 (P1) — no edit affordance
  on curated prose ships here.
- No new dependencies (Constitution Art. II/V). Reuse E6 components and
  E8's engine seam; do not rebuild.

## Non-Goals (guarded)

- **Game terms beyond conditions** (traits, actions, feats): out of POC
  scope, added on demand (PRD). The linkify contract is built so adding
  kinds later is data, not surgery.
- **Curation editing / prose versions** — E15 (P1). Curated prose
  changes by PR to the seed file at P0, exactly like E4's
  `condition-tiers.json` precedent.
- **Custom-row deletion** — not in the PRD's P0 line (create/edit only).
  Rows are cheap; deletion interacts with live references (prepared
  slots, inventory rows, applied effects all key by name/id) and is not
  wanted for the POC.
- **The freeform effect composer UI** (PRD FG3 Step 3 P0 element,
  unbuilt) — a real gap, but not gh#11's scope; **filed as gh#74**
  (standalone, sequenced ahead of E13/gh#15 — does not block E9;
  see Clarify Log Q2 and design D4). Not silently absorbed, not
  silently dropped.
- **Party-wide item picker / book value** — E12 (P1). Custom items exist
  in the corpus now; E12 consumes them later.
- Constitution non-goals stand: no builder, no combat tracker, no GM
  tooling beyond read-only, no dice roller.

## Clarify Log (settled from sources, 2026-10-20)

1. **"The caster's spell composer" = the spell preparation surface**
   (spellbook + prepare dialog, E6's `CasterPanel`/`MagicPane`), not the
   FG3 freeform effect composer. Grounds: the PRD's custom-entry flow
   (row "surfaces immediately where created and joins the relevant
   picker") is incoherent for a math-carrying composer — custom rows are
   display/tracking by FR-6, so their home is where spells are listed
   and prepared by name. `docs/EPICS.md` line ~54's "effect composer"
   wording is loose shorthand for the caster's composing surface; the
   rejected reading is logged in design.md D9.
2. **The condition picker UI does not exist anywhere** — E8's spec was
   API-first (its stories apply conditions "via the API"); E10 renders
   chips only; grep confirms no picker surface in the tree. E9 builds
   the minimal picker (FR-7) because three gh#11 requirements live
   there. The **freeform effect composer UI** (PRD Step 3 P0) remains
   unbuilt after E8 closed — E9 does not absorb it; flagged to Thrane
   for an ownership call with the sizing read. **Answered: filed as
   gh#74** — standalone, ahead of E13; not E9 scope.
3. **Custom item surfacing**: creator's inventory row (qty 1) + corpus
   row; the party-wide surface for items at POC is the read-only sheet
   (E10 cross-member view) — the party-wide *picker* for items is E12
   (P1), per PRD's own "items feed FG4 book value" tiering.
4. **Corpus conditions without curated prose** (added upstream after the
  freeze): minimal tooltip — name, badges, "no paraphrase yet". PRD:
  coverage is "every Player Core condition as of the freeze"; the rest
  is "added on demand" (curation, by PR).
5. **Caps**: name 1..64, description 0..280 ("one-line"), spell rank
   0..10 (wire's rank bound), condition value 1..20 optional. Design
   detail, recorded for the plan's tests.
6. **AoN URL shape**: `https://2e.aonprd.com/Conditions.aspx?ID={id}`
   — the prototype's own format, carried by the seed.
7. **Custom rows vs importer re-runs**: no `source_id` ⇒ outside
   `corpus_entries_kind_source_id_key`; E4's spec already guarantees
   re-runs never touch `custom` rows. E9 asserts it with a test
   (FR-8) rather than re-trusting the comment.
8. **Custom condition tier**: the picker reads
   `data->'import'->>'tier'` with a `display_only` default on NULL —
   custom rows simply carry no `import` block; no importer-namespace
   fields on rows E4 never wrote. `valued` reads NULL modifiers ⇒ false.
   Zero contract changes to E8's REST.

## Edge Cases

- **Effect name that matches a condition but was freeform-built** (no
  corpus link): tooltip still renders by name match — the prototype's
  own discipline; the corpus link only *adds* badges where present.
- **Custom row named like an upstream row**: allowed (E4 spec edge —
  collides harmlessly outside the unique index); picker shows both,
  distinguished by lane badge.
- **Two creators, same custom name**: both rows exist; both list; each
  creator edits only theirs.
- **Renamed upstream condition**: corpus `name` updates in place; the
  seed joins by name and a rename orphans the prose entry — surfaced by
  the coverage test (fixture-driven), fixed by curation PR.
- **Custom item quantity reaching 0**: row persists (qty rows persist
  today); no auto-delete.
- **Pathbuilder re-import of a party holding custom items**: a custom
  item is absent from every Pathbuilder export by construction, so the
  importer's kept-delta reconciliation (`KeptEntry::item`,
  `src/pbimport/anchor.rs`) surfaces it as a notice in the import diff
  on every re-import, forever. Accepted: the diff is a review surface,
  never a mutation; the corpus row and inventory row are untouched.
  Suppressing the notice is an explicit non-goal.
- **Tooltip inside tooltip** (second layer open, hover a third name):
  second layer replaces its content in place — the prototype's pattern;
  never a third stacked layer.
- **Offline**: pickers that need a fetch degrade to their last fetched
  state; tooltips over already-rendered chips work fully offline (FR-10).
