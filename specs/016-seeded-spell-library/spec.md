# Feature Specification: Seeded Spell Library + Conflict Pre-Warn (E13)

**Epic**: E13 — Phase 3, P1 (FG3) · GitHub issue #15 · depends on E8 (engine, closed) and E9 (custom content + composer entry point, closed)
**Created**: 2026-10-16
**Status**: Specify gate run 2026-10-16 (Korrin, front half of the SDD pipeline — docs only, no code in this pass). All clarify decisions settled in place (Clarify Log below); **none moves the P0/P1/P2 line**. One ask genuinely belongs to humans outside this company and it is named precisely, in prose a card can carry verbatim: [`contracts/content-ask.md`](contracts/content-ask.md). **The code half is buildable and shippable without it** — see *The split*, and the stop-and-flag clause in Out of scope.
**Input**: `docs/EPICS.md` Epic E13 (specify prompt + AI Guardrails); PRD v3.6 FG3 (Seeded effect library, two tiers; **Seeded spell library (P1)** paragraph; *Conflicting effects*; Core Experience Step 3; UI Elements "P1 additions"; Party Screens; Non-Goals); `docs/EPICS.md` dependency note ("E13 both touch the effect composer … sequence E13 after E9"); E8 `specs/008-buff-effect-engine/` (contracts §1–§4, the closed stat vocabulary, the stacking rules, the effect write frames); E9 `specs/010-rules-tooltips-custom-content/` (the custom-entry flow at point of use, the inert-setter discipline); the E8-residual composer spec `specs/011-effect-composer/spec.md` (the P0 write surface this epic extends); E4 `data/seed/condition-tiers.json` + `src/import/seed.rs` (the shipped precedent for human-authored checked-in seed data with fail-loud vocabulary validation); merged tree: `engine/src/vocab.rs`, `engine/src/model.rs`, `engine/src/stack.rs`, `src/sync/write.rs`, `src/engine_host/rest.rs`, `src/engine_host/load.rs`, `web/src/lib/sheet/components/EffectsComposer.svelte`, `web/src/lib/sheet/vocab.js`.

---

## Position

E8 shipped the engine and the `effect_new` create frame. E9 shipped custom content at the point of use. The E8-residual composer shipped the P0 write surface: name, modifier rows over the derived stat vocabulary, duration note, target picker. Every spell at the table is cast through that form today — a human typing "+1 status, attack" while the fight waits.

E13 removes the typing for the ~20 spells this party actually casts:

> cast → the composer offers the spell's **degrees of success** → the caster taps what happened → the spell's **defined effects apply to the chosen targets automatically**, with no manual modifier definition.

and it answers the stacking question **before** the caster commits, in the target picker, with the engine's own math rather than a remembered rule of thumb.

Three things this epic is not, stated up front because each is a plausible misread:

- **It is not a spell database.** There is no spell corpus in this product. The shipped corpus is conditions + equipment only (`src/import/model.rs` `Kind` — two arms); the PRD records spells as "importer-capable but unused" and deliberately leaves the library list **une-numerated in the document** ("*The definitive list is importer seed config owned by Josh* … **No questions asked, literally.**"). E13 ships the *mechanism* and the *file* the list lives in. It does not ship the list.
- **It is not a spellbook, a prepared-cast tracker, or a casting log.** Slot usage and prep live in E5/E6's import path. Casting a library spell here mints effects; it never spends a slot. Slot deduction on library cast is a PRD change and a stop-the-line item (Out of scope).
- **It is not enemy-facing.** Enemies are not roster entities; the effects of a spell that lands on an enemy stay player-managed, exactly as the PRD's scope fence says. Modeling enemy state is combat tracking, a permanent non-goal.

### The split — content half and code half (binding, and the reason this spec reads differently from E12's and E14's)

`gh#15` names human dependencies the way no previous epic did: **Josh owns the seed-list config; Dave authors the outcome templates; Josh reviews them; "never model-generated."** That half of the epic is not this shop's work — not in part, not as a placeholder, not as "illustrative" template prose that quietly becomes the shipped text. So the epic splits, and this spec says which side of the line every artifact sits on:

| | owned by | what it is | in this epic's PR? |
|---|---|---|---|
| **Content half** | **Josh** (seed-list config, review) + **Dave** (template prose) | the ~20 named spells; each spell's per-degree template prose and its effect rows | **No.** Arrives later as data in a PR of its own, reviewed by the owners. |
| **Schema** | this shop | the shape content is written against, its validation, and its documented example | Yes — `contracts/spell-library-seed.md`. |
| **Code half** | this shop | loader + fail-loud vocabulary validation, the library read, the degrees-of-success tapper, automatic effect application, conflict pre-warn | Yes — buildable and mergeable against an **empty** seed file. |

**The code half is not content-gated.** It is designed, planned, built, tested and merged against the shipped precedent's own fail-safe posture: an empty seed file is a valid configuration, the library is simply empty, every spell is freeform exactly as P0, and nothing in the composer is broken by it. When content lands it is a **data-only PR** — no code change, no schema change, and the same tests still green. If implementation discovers a place where that is not true — where a code path genuinely cannot be built or tested without shipped content — that is not licence to author content: it is a stop-and-flag, and what it depends on gets named in the PR body. (Design **D0** records the two places this was checked hardest and why both came out clean.)

**The one illustrative template in this epic** lives in [`contracts/spell-library-seed.md`](contracts/spell-library-seed.md), is labelled *schema illustration*, sits in a `contracts/` document rather than in `data/seed/`, and is deliberately *not* one of the party's likely spells. The shipped seed file ships with `"spells": []` and the same kind of `_readme` block `condition-tiers.json` carries.

## User Scenarios & Testing *(mandatory)*

The users are the six party accounts. Two seats matter in every story: the **caster** — the character owner casting and composing — and every **target**, whose sheet is where the result becomes visible. The **GM** reads everything and writes nothing, unchanged from every prior epic. Library spells are *party-targeted only*, so the target set is always roster characters.

### User Story 1 — Bear casts Bless without typing (Priority: P1) 🎯 MVP

*Bless* here is quoted from `docs/PRD.md` — it is the PRD's own example spell at `:205,320,353,373` —
and is used under FR-3's quoting exemption. It is not a candidate entry for the party's list.

Bear taps *Bless* in the composer's spell-library section instead of writing it out. The form arrives already named, already carrying a duration note, already carrying the four outcomes. He picks his targets, taps **"success"**, and the effect is on Josh and Becky — both sheets' attack numbers rise on every connected device with their provenance, exactly as if he'd typed the modifier, and he typed nothing. The next round he casts again and taps **"crit failure"**; the critical-failure outcome's effects apply instead.

**Why this priority**: this is the epic's whole promise and the PRD's Step 3 rewrite — "if it's in the seeded spell library (P1), its outcome template applies automatically; otherwise it's freeform via the picker." A library section that only pre-fills a form Bear still has to assemble is not this story.

**Independent Test**: load the composer with a two-spell test library fixture (a fixture file in the test tree, *not* the shipped seed); open the library section; select the first spell; assert the four degrees of success are offered and the target picker is the roster picker already in the composer; select two targets; tap *success*; assert the sync surface receives the exact `effect_new` create op whose modifiers are the fixture's *success* rows, field-for-field. Then drive the same spell with *critical failure* and assert a **different** modifier set — a flow that reads one hardcoded outcome cannot pass.

**Acceptance Scenarios**:

1. **Given** a caster whose library has a spell with structured outcomes, **When** they select it and tap a degree of success, **Then** exactly one `effect_new` create op is sent — no manual modifier definition happened anywhere — carrying the spell's name as the effect name, the chosen targets, that degree's modifier rows, and the spell's duration note, with the caster as source.
2. **Given** the same spell cast twice, once tapped *success* and once tapped *critical success*, **When** both creates are read, **Then** their modifier sets differ (two fixtures that differ in the tapped outcome, asserting two different results — the two-fixture rule applied to the degree axis).
3. **Given** a spell whose *failure* outcome defines no effects, **When** the caster taps *failure*, **Then** no effect is created and the composer says so plainly — casting into nothing is a real PF2e outcome and is not an error state.
4. **Given** a tapped outcome with effects, **When** the create is applied, **Then** every affected number recomputes with provenance through the existing engine path, the caster's sheet never computes anything, and no second computation path exists (the PRD's one-mechanism rule).
5. **Given** the GM seat, **When** the library section renders, **Then** it renders zero write affordances — no spell row is tappable into a create — as everywhere else.

### User Story 2 — "Becky: +1 status active — Bless would be suppressed" (Priority: P1)

Bear is about to Bless Becky. Becky already carries a +1 status from the Bard's Inspire Courage. The composer's target picker says so **before** he assigns the target: *"Becky: +1 status active — Bless would be suppressed."* He decides with the math already done, and either picks someone else or casts it anyway knowing what he's buying.

**Why this priority**: it is the epic's second named deliverable, and it is the one the PRD quotes verbatim twice. A warning that arrives after assignment is a receipt, not a decision aid; the PRD's demand is *pick time*, and the AI Guardrail's demand is that the warning come from **live engine state, not cached chips**.

**Independent Test**: seed a party state where Becky carries an active +1 status effect and Josh carries none; render the composer's target picker for a library spell whose success outcome is a +1 status to attack; assert Becky's row carries the conflict flag naming the live effect and Josh's does not. Then, without reloading, apply a second +1 status of a higher value to Josh through the real write path and assert Josh's row flags on the fan-out. Then end Becky's effect and assert her flag clears. (Three states of live state; a cached chip list passes none of the last two.)

**Acceptance Scenarios**:

1. **Given** a target whose active effects include one of the same modifier type the spell's outcome would add, competing on a stat the outcome touches, **When** the picker renders, **Then** that target's row carries a conflict flag naming the live effect's name, its type and its value, and states the outcome — *would be suppressed* or *would not stack with it* (the honest wording per Clarify Log #3).
2. **Given** two candidates that differ only in **type** — a target carrying a +1 item bonus where the spell is +1 status — **When** the picker renders, **Then** no flag. Type competition is what stacking is, and a flag that fires across types teaches the caster to ignore flags.
3. **Given** a target carrying a same-type **penalty** where the spell is a bonus, **When** the picker renders, **Then** no suppression flag — bonuses and penalties of a type resolve separately and both apply (the engine's WEx-4 rule); the two mechanisms are not the same question.
4. **Given** an equal-value same-type tie, **When** the picker renders, **Then** the flag fires and is worded as a tie ("same value — only one applies"), never as "lower".
5. **Given** the flag's inputs change while the composer is open (a fan-out adds, ends, or revalues an effect on that target), **When** the picker re-renders, **Then** the flag appears/clears/updates without reload and without reopening the composer. This is the guardrail's testable form: **computed from live engine state, not cached chips.**
6. **Given** a blanket outcome (`all_checks`, `all_dcs`, `all_checks_and_dcs`) or a per-instance one (`attack`, `damage`, `spell_attack`, `spell_dc`), **When** the flag is computed, **Then** it is decided on the concrete stat instances the outcome expands to on that target's sheet — the engine's own expansion and stacking, not a per-type string comparison invented for the picker.
7. **Given** a target whose derived state is not loaded client-side at pick time, **When** the picker renders that row, **Then** no flag is shown for it (absence never renders as "safe-by-decision"), and the design's chosen read path is what makes this case rare rather than structural (design **D5**).

### User Story 3 — A bad template fails at config load, not at the table (Priority: P1)

Dave's PR adds a spell whose template says `stat: "attak"`. Nothing at the table can ever see it. The server refuses to start with that file; the message names the spell, the outcome, the row, and the field; the CI job that runs the load is red on the PR. The same holds for an unknown modifier type, an outcome with no degree, a target kind the engine does not take, and a spell entry with no structured outcomes at all.

**Why this priority**: this is the AI Guardrail stated as a user story, and it is the reason a human can own the content safely. The failure the guardrail exists to prevent is a template that half-works at 10pm on a Tuesday.

**Independent Test**: write fixture files each carrying exactly one defect (invalid stat text, invalid modifier type, an unknown degree, a missing required field, an empty targets kind, a malformed JSON shape) and assert the load refuses each with a message naming the spell and field; assert the shipped seed passes; assert a **valid but empty** (`"spells": []`) seed loads green.

**Acceptance Scenarios**:

1. **Given** a seed file where any spell references a stat outside the closed vocabulary, **When** the config load runs, **Then** it fails loudly, naming the spell, the degree, the row index and the offending text — no partial library is served and no bad entry is silently dropped.
2. **Given** a seed file that is valid, **When** the load runs, **Then** every stat and modifier type in it resolves against the engine's own vocabulary tables — the ones `engine/src/vocab.rs` re-exports — so there is one source of truth and the library cannot drift from the engine.
3. **Given** the shipped seed is empty, **When** the load runs, **Then** it succeeds and serves an empty library (the fail-safe direction, as with an unmapped condition in the tier seed).
4. **Given** a defect class named in FR-13's table, **When** the load runs on a fixture carrying it, **Then** the load refuses it. Each class has its own assertion through the production load path — a table row nobody tested is a claim, not a behaviour (AGENTS.md: a fail-closed claim carries its own assertion per named input).
5. **Given** a defect is refused, **When** the refusal surfaces in CI, **Then** it is a red check on the content PR naming the same message, so the content owner sees the fix in the place they already look.

### User Story 4 — Everything that is not in the library is exactly as it was (Priority: P1)

**A spell outside the list** is cast the way it always was. The story deliberately names no spell:
"a spell outside the library still casts freeform" needs no name to be legible, and a coined name
would behave like a candidate entry that was quietly kept out of the list. Bear casts an unlisted spell the way he
always has: *New effect*, name, `+1 status → attack`, duration, targets. The library section does not intercept it, does not grey anything out, does not hint that he is doing it wrong, and — the sharp edge — **no freeform spell ever mints a `custom` corpus row on its way past the composer**. The library is an extra door, not a new hallway.

**Why this priority**: the PRD's sentence is "*Spells outside the library stay freeform exactly as P0*." The cheap way to build a library is to route the composer through it, and that way breaks this story and, if the composer is reused carelessly, drags E9's custom-content side-effects into the P0 path.

**Independent Test**: open the composer with a non-empty library and use the freeform form exactly as P0's tests drive it; assert the same `effect_new` op the existing composer tests assert, assert zero custom-content writes occurred, and assert the library section's presence did not change the freeform form's fields, validation, or reject-reason surfacing.

**Acceptance Scenarios**:

1. **Given** any spell, in the library or not, **When** the caster uses the freeform form, **Then** behaviour is byte-identical to the shipped P0 composer — same fields, same bounds, same server-reason surfacing.
2. **Given** a library spell selected and then abandoned, **When** the caster switches to freeform, **Then** no phantom effect was created and the form is clean.
3. **Given** the library read fails (server error, empty response), **When** the composer opens, **Then** the freeform form works normally and the library section states its own state — the P0 path never depends on the library's availability.
4. **Given** the library contains a name the party also types freeform, **When** either path is used, **Then** both work independently. Library membership is not a lock on a name.

## Requirements *(mandatory)*

### Functional requirements

**Half-marked**: **[C]** = code half, this shop's work, not content-gated. **[X]** = a requirement whose *subject* is content (Josh/Dave's) but whose *enforcement* is our code — the code enforces a rule about data it does not author.

- **FR-1 [C] — The library is checked-in seed config.** The system MUST ship the library as a checked-in, human-reviewed data file in the same posture as `data/seed/condition-tiers.json`: version-controlled, PR-reviewed by the owners, never derived from a pack document, never model-generated by this product's code (Constitution Article IV's posture, restated for this file). The file's definitive contents are Josh's (the seed-list owner's). **The shipped file at this epic's merge time is empty** — `"spells": []` — and that is a valid, tested, shippable configuration, not a stub awaiting rescue.

- **FR-2 [C] — Where the file lives and how it is named.** `data/seed/spell-library.json`, loaded by the same mechanism the tier seed uses (checked in, `include_str!`'d / embedded at build, validated at load, served through a session-gated read). One directory, one loading story, one precedent — the tier seed is the shipped answer to "how does human-authored data get into this product safely," and this epic joins it rather than inventing a second. *(File naming is design-confirmable; the posture is not.)*

- **FR-3 [X] — The list is not enumerated here.** Neither this spec, nor a design, nor a plan, nor a PR body enumerates or implies the party's spell list. A spell name appearing outside `contracts/spell-library-seed.md`'s labelled illustration is a scope violation — **unless it is quoted from a binding source, and each one names that source at first use.** *(This requirement exists because the temptation is structural: a spec that needs "an example spell" to be legible will otherwise grow the list one example at a time.)*

  The quoting exemption is narrow, and it is the difference between the two failures this rule
  guards against. **Allowed:** a name carried in from `docs/PRD.md` or `docs/EPICS.md` — the PRD's
  own *Conflicting effects* example (Bless vs Inspire Courage, both +1 status) is the requirement
  being specified, not a candidate entry; a spec that could not name it could not state US-2 at
  all. Each such name cites its source file and line where it first appears. **Not allowed:** a
  name invented to make a list-membership or behaviour claim legible. A coined name carries no
  source, so the claim rests on data nobody owns — and it can then be *used* like data, which is
  exactly how the one violation of this rule surfaced: a fictional spell was used to assert that
  a spell outside the list still casts freeform, i.e. it behaved like a candidate entry that was
  quietly kept out. Use an unnamed spell ("a spell outside the library") or quote a sourced name
  and cite it; do not coin one.

  Two named spell occurrences satisfy the exemption at this head, and each cites its source where
  it is load-bearing: **Bless** (`docs/PRD.md:205,216,320,353,373,377`; `docs/EPICS.md:165,236`) and
  **Inspire Courage** (`docs/PRD.md:206,353`). US-2's and FR-11's quoted message string is the PRD's
  own `:216` verbatim, not this spec's wording. US-1 and US-2 name *Bless* because they are the PRD's
  own *Step 3* scenario and *Conflicting effects* example restated — a US-1 that could not name the
  spell the PRD names could not state the story. **Checkable** — one command, the whole spec directory, not one
  file: see `checklists/requirements.md`, "the one check unique to this epic".

- **FR-4 [X] — What one library entry must carry.** A library entry is, at minimum: a name; one or more **named outcomes**, each of which carries **zero or more** effect definitions and display prose; and a duration note. An effect definition carries a modifier type, a stat from the closed vocabulary, a value, and a target selection rule resolvable at cast time. The precise field names, the ordering, and the JSON shape are fixed by [`contracts/spell-library-seed.md`](contracts/spell-library-seed.md) — which is this shop's deliverable and the thing Dave and Dave's PR are written against. Content authors are not asked to design the schema; they are asked to fill it.

- **FR-5 [C] — Degrees of success are exactly the four PF2e ones.** The library's outcome axis is the critical-failure / failure / success / critical-success axis, offered as four tappable choices. The epic defines no fifth outcome, no "auto" choice, and no inference about which one happened. The caster is the authority on what happened, in the same way the caster is the authority on when an effect expires (PRD US-3's rule).

- **FR-6 [C] — One tap mints one effect through the existing write path.** Tapping a degree MUST send exactly one `effect_new` create frame through the existing sync write path — with the spell's name, the caster as source, the chosen roster targets, that degree's modifier rows, and the spell's duration note. **No new effect frame, no new field kind, no second write path, no engine change.** The engine sees a library spell's effect and cannot tell it from a freeform one; that indistinguishability is the design's correctness argument, and the epic's proof obligation to keep it.

- **FR-7 [C] — A degree with no defined effects creates nothing.** Where a spell's *failure* or *critical failure* legitimately does nothing (the common case for a buff), tapping it MUST NOT mint an effect, MUST NOT error, and MUST state in plain text that nothing happened. A silent no-op is as unreadable at the table as an error.

- **FR-8 [C] — Targeting stays the composer's, and roster-only.** The library supplies *what* applies; the caster supplies *whom*, through the picker that already exists. Library effects target roster characters only — companions, minions, and enemies are not targetable (engine rule, PRD non-goal), and no library template can widen that set from data.

- **FR-9 [C] — A library spell never spends a slot and never logs a casting.** Minting a library effect MUST NOT touch spell slots, prep marks, or any import-owned row. Whether library casting deducts a slot is an open product question (Clarify Log #5), filed and unanswered by design; the answer "no" is what this build ships, and the answer "yes" is a PRD change, not an implementation liberty.

- **FR-10 [C] — Spells outside the library are freeform, exactly as P0.** The library MUST be additive to the composer: same freeform form, same validation, same server-reason surfacing, same availability when the library read fails, and **no custom-content row minted by either path** (E9's custom-entry affordances stay where E9 put them — a library section is not a custom-entry point).

- **FR-11 [C] — Conflict pre-warning at pick time.** The composer's target picker MUST flag, for each candidate target and each effect the caster is about to apply, whether that effect would be **suppressed** or would **not stack** with an effect already active on that target — decided **before** assignment, from live engine state, by the engine's own stacking rules (`engine/src/stack.rs` — expansion, per-instance competition, type rules, tie-break), never by a client-side rule of thumb and never from a cached chip snapshot. The flag names the live effect (name, type, value) so the message reads like the PRD's example: *"Becky: +1 status active — Bless would be suppressed."*

- **FR-12 [C] — A flag never blocks.** The warning is information: the caster may apply a suppressed effect anyway (they may be pre-positioning for an expiry, or casting to bait a dispel). No flag disables a target row, blocks the apply, or dims a choice. Conversely a flag that is wrong often enough to be ignored is worse than none — which is why FR-11's decision must be the engine's.

- **FR-13 [C] — Fail-loud validation at config load, per named class.** A library file that fails validation MUST prevent the server from starting, and MUST be refused in CI on the PR that introduced it, with a message naming the spell, the degree, the row and the field. The defect classes are enumerated and each is individually asserted through the production load path — see design **D2**'s table. A silently-skipped bad entry is a failure of this requirement even when the load succeeds for the rest of the file.

- **FR-14 [C] — One vocabulary source.** Validation resolves stat text and modifier-type text against the engine's own re-exported tables — the same constants `engine/src/vocab.rs` publishes and the tier seed re-exports. A library-specific stat list is prohibited; a second list is how a library and an engine stop agreeing.

- **FR-15 [C] — The library reaches the composer through one read.** The library MUST be served to the client through a session-gated, party-scoped read in the established shape (`GET /api/parties/{party_id}/spells`, joining `/conditions` and `/effects`), so the client carries **no** embedded copy of the library and content changes ship with the server, not with a rebuilt bundle. *(Design **D4**.)*

- **FR-16 [C] — The composer's entry point is not rebuilt.** The library section is added to the composer that shipped in the E8-residual work — the same dialog, the same target picker, the same state layer. The PRD's sequencing note is honoured by extending that surface, not by standing a parallel one beside it.

- **FR-17 [C] — Provenance and the engine are consumed, never extended.** Effects applied from the library produce ordinary provenance entries and ordinary suppression records; the engine's output contract does not change shape, and no new engine concept ("library", "template", "degree") ever appears in engine output or on the wire.

- **FR-18 [C] — GM seat renders no library write affordance.** Unchanged house rule, stated here so the acceptance sweep has a row for it: the GM sees the library section read-only (or not at all, per design) and can never mint an effect from it.

- **FR-19 [C] — Curation prose stays manageable.** The library's display prose is authored as data and reviewed as data. E15's in-app curation editing does **not** reach library outcome prose in this epic: templates ship through the same owner-reviewed PR path as the list itself, and in-app editing of a file that is checked-in seed config is a different product shape (Out of scope; design **D7** records the rejected alternative).

### Key entities

- **Library entry** — one spell in the seed config: name, duration note, the set of named outcomes. *Data; Josh's list.*
- **Outcome template** — one degree of success's display prose plus its effect definitions. *Data; Dave's prose, Josh's review.*
- **Effect definition** — one modifier row inside an outcome template: type, stat, value, target selection rule. *Data, validated by our code against the engine's vocabulary.*
- **Library read** — the session-gated party-scoped delivery of the config to the composer. *Code.*
- **Degrees tapper** — the four-way choice and the single create op it sends. *Code.*
- **Conflict flag** — the pick-time verdict on one (target × effect-definition) pair: would apply / would not stack / would be suppressed, with the live effect named. *Code, computed from engine state.*
- **Suppression reason** — the engine's existing record of *why* a modifier lost (`same-type-lower-bonus`, `same-type-tie`, `same-type-lighter-penalty`). Reused read-only for the pre-warning; E13 adds no new reason.

### Constraints (settled by the epic — non-negotiable)

- **No engine change.** The engine is pure and closed; E13 consumes `vocab`, `stack`, and `model` read-only. An implementation task that wants an engine change stops and flags.
- **No new wire kind.** Effects flow on the shipped `effect_new` / `effect` frames. No `spell_cast` frame, no library version column, no new `FieldTarget` arm.
- **No new migration.** The library is checked-in data, not rows. (Design **D8** records the rejected DB-backed alternative and when it would become worth its cost.)
- **Content is never authored here.** No placeholder spell, no "sample" spell in `data/seed/`, no illustrative prose in the shipped file, no seed data in a test fixture presented as the party's list. Test fixtures are fictional and labelled as fixtures.
- **No model-generated templates, anywhere in this epic's work.** The PRD's prohibition binds this shop's seats as tightly as it binds a generator: the code half writes code, the schema, and the ask.
- **PRD non-goals bind**: no combat/round/initiative tracking, no enemy state modelling, no auto-expiry, no aura/positioning automation, no dice roller. A degrees-of-success tapper is a *tap*, not a roll — it never rolls, never reads a roll, and never infers an outcome.
- **Stack settled** (Rust/axum single binary, Svelte static bundle, Postgres) — unchanged.
- **The composer stays one dialog** (FR-16), and the P0 freeform path stays byte-identical (FR-10).

### Success Criteria

- **SC-1** With a two-spell **test fixture** library (fictional spells, in the test tree), a caster selects a spell, taps *success*, and one `effect_new` create lands carrying that degree's modifiers; tapping *critical success* on the same spell yields a create whose modifiers differ. *(Bench-verifiable; the shipped empty seed is not needed.)*
- **SC-2** Every one of the four degrees is drivable end to end, including the empty-effect outcomes, which produce zero creates and a plain statement. *(Bench-verifiable.)*
- **SC-3** A target carrying a competing same-type effect is flagged in the picker **before** assignment; a target carrying a non-competing type is not; a same-type penalty against a bonus is not; an equal-value tie is flagged as a tie. *(Bench-verifiable: four targets in four states, one render.)*
- **SC-4** Flags are live: an effect applied or ended on a target while the composer is open makes its flag appear or clear without reload and without reopening the dialog. *(Bench-verifiable — this is the "not cached chips" criterion.)*
- **SC-5** Every defect class in FR-13's table is refused by the production load path with its named message, each by its own assertion; the shipped (empty) seed loads green; a CI job runs the load so a content PR is red on the defect. *(Bench-verifiable.)*
- **SC-6** With the shipped empty seed, the composer's freeform path behaves exactly as its existing tests assert it today — the epic merges with the library empty and no P0 test modified. *(Bench-verifiable: the existing suite, unedited.)*
- **SC-7** Library and conflict code introduce no computation of a game number outside the engine: the epic's new client files carry no stat arithmetic, and the flag's verdict comes from the engine's stacking output. *(Named grep sweep + line-by-line diff review, honestly labelled directional — the E14 SC-6 discipline.)*
- **SC-8** The GM seat renders zero write affordances in the library section. *(Bench-verifiable: scoped query in the section's region.)*
- **SC-9** With a **non-empty test-fixture** library, the client displays the same spell set the server read — and after the config file is extended by a data-only change plus a server restart, the client's next read shows the addition with **no client rebuild**. *(Bench-verifiable on the read contract; this is what makes content landing cheap.)*
- **SC-10** *(content-gated — not this epic's gate)* Once the party's real seed is merged, casting each shipped spell through the UI produces effects the owners recognise. This criterion is owned by the content PR and Josh's review, and **no code PR's merge depends on it**.

## Edge Cases

- **Library file empty (the shipped state).** Library section renders its honest empty state; the composer is exactly P0. The epic is not "half installed" in any user-visible way.
- **Library file has one spell.** Everything works; nothing in the UI assumes a count.
- **A spell has three outcomes, not four** (say no defined critical failure). The missing degree is not offered; the four-way row is generated from the data, not fixed at four.
- **A degree whose effects list is empty.** FR-7 — no effect, plain statement, no error.
- **A template names a blanket stat (`all_checks_and_dcs`).** Validation accepts it; the conflict pre-warn expands it to concrete instances per that target's sheet (engine expansion) before deciding flags.
- **A template names a per-instance stat (`attack`) on a target with three strikes.** The flag is decided per instance; the message does not claim more precision than the instance set supports.
- **A template names a skill the target does not have.** The engine drops the modifier for want of an instance (shipped behaviour). The pre-warn must not report a conflict the engine will silently not apply — no instance, no flag.
- **The caster targets themselves and a suppression exists on themselves.** Same rule as any target; the caster's own row is not exempt.
- **Two effects from the same spell, cast twice in the same round.** Ordinary stacking: same type, tie → `same-type-tie`, one applies. No de-duplication, no "you already have Bless" special case — the engine already answers this and the picker flag already surfaces it.
- **A library spell and a freeform effect with the same name.** Ordinary stacking again: identity is `effect_id`, never the name (shipped engine rule). Nothing about the library makes names unique.
- **The library read fails mid-session.** The library section states its state; the freeform path is untouched (FR-10.3).
- **The caster opens the composer, a conflict flag is shown, and they cast anyway.** The apply proceeds; the effect lands suppressed; the target's sheet shows it in the suppressed provenance list as it does today. The warning was advice, correctly taken.
- **A party member's derived state isn't loaded client-side when the picker renders.** FR-11/US-2.7: no flag shown for that target; the design's read path (**D5**) is what makes this rare.
- **Offline.** The composer's library section behaves per the existing offline posture — reads are last-known, writes queue through the existing queue, flags render from last-known engine state, and the epic adds no queue, no cache layer, and no offline claim of its own.
- **Content lands while a session is live.** A data-only PR changes the served library; the next read picks it up (SC-9). No client rebuild, no migration, no client release.
- **The shipped file grows past a handful of entries.** At ~20 entries the file is tens of KB and the read is trivial. If it ever grew toward a real corpus, that is E4's importer lane, not this one — recorded so the discovery is a decision.
- **A spell targets a companion.** Not targetable (shipped rule); the picker does not offer it, and a template cannot override that from data (FR-8).

## Assumptions

- One party at POC (config, inherited). The library is global to the deployment, not per-party — it is the party's list because there is one party. Multi-party is P2 and the file's shape does not pretend otherwise.
- The six accounts are the whole audience; no external/beta library exposure is assumed.
- The caster knows which degree of success happened (the table rolls physical dice). No roll input surface exists in this epic.
- Duration is a **note**, not a timer — inherited from the composer as shipped. Auto-expiry remains a non-goal.
- Content arrives as a PR against the checked-in file. It does not arrive through a UI in this epic's lifetime (FR-19).
- The "prototype's paraphrase style" Dave authors in is Dave's established voice (rules-accurate, AoN-linked per PRD FG3's provenance paragraph). The schema requires prose and a citation field; it does not attempt to validate voice — Josh's review is the check on that, which is why the PRD assigns it to Josh.

## Out of scope (guarded)

- **The spell list itself and all outcome content** — Josh's and Dave's, per *The split*. Not this epic's PR. Stop-and-flag if a seat is asked to author it.
- **A fifth degree of success, or any outcome inference/automation.** The four are the axis.
- **Spending a spell slot on library cast.** Open question, Clarify Log #5, filed unanswered by design; the shipped answer is *no*. Building *yes* without a PRD change is the stop-the-line case.
- **Enemy-targeted effects, enemy state, attack roll resolution.** PRD scope fence and permanent non-goal.
- **Rolling dice.** The tapper taps; it never rolls.
- **In-app editing of outcome templates** (E15's shape applied here) — FR-19; design **D7** records the rejected alternative.
- **A DB-backed or per-character library, spell corpus import, spell search, ritual/traction handling, cantrip elevation, heightened-cast variants beyond what a template row expresses.** Any of these is a PRD conversation; a template that cannot express a real case is feedback *to* the PRD, not a code-around (Constitution Article V's spirit, AGENTS.md rule 5).
- **Auto-applying conditions to a caster's own sheet from an enemy's attack** — combat tracking, forever.
- **Modifying the freeform composer's P0 behaviour** in any way except adding a section to it (FR-10).
- **Custom-content row creation from the spell path** — explicitly prohibited (FR-10) rather than merely unlisted, because the E9 custom-entry precedent sits three feet away in the same dialog and reuse there would be a plausible-looking mistake.

## Clarify Log

No `[NEEDS CLARIFICATION]` markers remain. Eight decisions the prompt, the PRD, or the epic's own shape left open are settled in place, each with its reason. None moves the P0/P1/P2 line. One (#1) generated a human ask; it is precise and lives in [`contracts/content-ask.md`](contracts/content-ask.md), and **the code half proceeds without it**.

1. **Where content ends and code ends, and whether the code half is content-gated.** Settled: it is not. The code half is designed and planned against an empty seed; content arrives as a data-only PR. The alternative — hold the epic until the list and templates exist — was rejected: it makes our own schedule hostage to two external people for work that is provably independent, and the shipped tier seed already demonstrates the fail-safe posture (no entry → display-only, the safe direction). The mechanism and its data were separated in E4; E13 separates them again.
2. **What the library is stored in.** Settled: a checked-in, PR-reviewed seed file beside `condition-tiers.json`, loaded and validated the way that file is (FR-1/FR-2). Rejected: **Postgres rows seeded by a migration** — it makes a human-owned, review-before-ship artifact mutable at runtime, splits authorship between a UI/SQL path and a PR, and buys a write path nobody is asked for. It also loses the property that makes the guardrail work: a bad template fails the *build*, not the table. Rejected: **a `custom`-lane corpus row per spell** — `custom` is E9's player-editable lane, and a lane Dave's templates must not share with party-editable content. Re-visit trigger recorded in design **D8**.
3. **What the flag actually claims.** Settled: the flag's wording is decided by the engine's real verdict, and the PRD's sample string is not treated as a literal promise. "Would be suppressed" is exactly right for a *lower* same-type bonus; for a same-type **tie** the correct statement is "same value — only one applies", because which one wins there is `(effect_id, ord)` order and the total is identical either way. The PRD sentence ("effects that would be suppressed by a target's active bonuses") is read as naming the class of pre-warning, not enumerating its one wording. The rejected alternative — a flag only on strict-lower — was rejected because a tie is precisely the case where a caster wants to know before casting, and it is the case the PRD's own *Conflicting effects* example (Bless vs Inspire Courage, both +1 status) actually is.
4. **Which effects the flag reasons about.** Settled: the ones the caster is about to apply, against the target's live active effects, decided by `engine::stack`'s own expansion-and-competition over the target's stat instances. Rejected: **a client-side "same type + both positive → warn" shortcut** — it is wrong on penalties (which coexist with bonuses), wrong on blanket expansion, wrong on per-instance stats, and wrong in the direction that teaches casters to ignore flags. Rejected: **a server round-trip per keystroke** — the verdict depends on `(spell, outcome)` and not on what the caster is typing or has selected, so a per-keystroke read re-fetches a payload that was already complete. Chosen instead — and this is the record of the decision, so the decision it records is the one that ships — **one server-side, party-scoped, read-only conflicts read** (`POST /api/parties/{party_id}/spell-conflicts`, design D5), keyed on `(spell, outcome)` and answered roster-wide with the picker filtering locally. The guardrail ("live engine state, not cached chips") is satisfied by the *computation* being the engine's own over live state, not by the bytes' location. D5's reason for rejecting the client-side computation, kept here so the rejection is not lost: `render_base` is not reliably available client-side for every roster card, so making the client compute it is a bigger change than one party-scoped read. *(The read path is named in design D5 so this is checkable, not asserted.)*
5. **Does a library cast spend a slot?** **Filed, unanswered by design, and answered "no" for the build.** The PRD describes the tapper as an effects-composition shortcut and lists slot usage under the import/live-state feature group; it never connects the two. Building the deduction would couple this epic to import-owned state the PRD does not ask it to touch and would make a wrong guess a silent miscount of the party's resources at the table. Building *no* is reversible in one place (design **D7** names it). This is the epic's one product-intent question that is genuinely Josh's — it is listed in the content ask as a one-line decision, and **it does not gate any code**.
6. **Does a library spell go through `custom` content?** Settled: **no, never** (FR-10). Library entries live in seed config; `custom` rows are the party's own editable lane. The two are different authorship regimes and the PRD's provenance paragraph says so.
7. **Is the library per-party or global?** Settled: global to the deployment for the POC (one party, one table, the list is *the* party's list per the PRD's own phrasing). The file's shape keeps a future party scoping additive rather than a rewrite; no per-party UI is built or implied.
8. **What "the importer seed config owned by Josh" means for our loader.** Settled: Josh owns the *content* of the config; this shop owns the *schema and its validation*, and the two meet at a documented contract with an example. Rejected: waiting for Josh to design the file shape — that asks an external person for a design decision, which is the stall this task's own instruction warns against. The ask says "fill this in," not "tell us what to build."

## Notes for the reviewer

Three places where a reader should push hardest, named because they are where an error would hide:

1. **FR-6/FR-17's claim of engine-indistinguishability.** If a library effect is *not* an ordinary effect on the wire and in the engine, everything downstream (provenance, suppression, offline, GM rules) needs re-reasoning and this spec's dependency claims are wrong.
2. **FR-11's "live engine state" reading (Clarify Log #4).** The guardrail is satisfied by the *computation* being the engine's own, running over live state on the server — **not** by a client-side computation, which **D5** rejects and Clarify Log #4 records as rejected. What the reading still owes is freshness and completeness: the conflicts read must be fed the target's live active effects and stat instances at pick time, or the flag goes stale, which SC-4 forbids. D5 names where each input comes from; the one input that is *not* reliably held for every roster card is `EngineOutput.render_base` — the per-character stat-instance list a blanket expansion and a per-instance fan need — and that is the reason the verdict is computed server-side rather than in the picker. If review finds another input that is not live at pick time, the cost is a wider party-scoped read, not a redesign.
3. **SC-6's promise.** The epic merges with an empty library and modifies no P0 test. If that turns out to be false — if a library section cannot be added without touching the freeform path — that is a finding worth stopping on, and it belongs in the PR body rather than in a quietly edited test.
