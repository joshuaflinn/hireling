# Specification Quality Checklist: Seeded Spell Library + Conflict Pre-Warn (E13)

**Purpose**: validate specification completeness and quality before review.
**Created / validated**: 2026-10-16 (Korrin — specify, then one clarify round, then design sign-off)
**Feature**: [spec.md](../spec.md) · gh#15 · design [design.md](../design.md) · plan
[plan.md](../plan.md) · contracts [spell-library-seed.md](../contracts/spell-library-seed.md),
[content-ask.md](../contracts/content-ask.md)

## Content quality

- [x] No implementation details (languages, frameworks, APIs) — where the spec names a route
      (`GET /api/parties/{party_id}/spells`) or a file path (`data/seed/spell-library.json`) it is
      naming a **shipped contract surface the epic must join**, per the house convention (E7/E12/
      E14 specs name their contract surfaces); no tech choice is made in the spec — the design
      owns those.
- [x] Focused on user value and business needs — US-1 (cast without typing), US-2 (decide with
      the math done), US-3 (a bad template never reaches the table), US-4 (nothing else changes).
- [x] Written for non-technical stakeholders, with engineer-precision where acceptance demands it
      (the four defect classes in FR-13, the four verdict states in FR-11).
- [x] All mandatory sections completed.

## Requirement completeness

- [x] No `[NEEDS CLARIFICATION]` markers remain — the Clarify Log settles eight decisions in
      place, each with its reason; none moves the P0/P1/P2 line. One (#1, the seed list itself)
      generated a human ask, which is precise and filed in
      [`contracts/content-ask.md`](../contracts/content-ask.md), and the code half proceeds without it.
- [x] Requirements are testable and unambiguous — 19 FRs, each with a deciding check; FR-7 names
      what a tap does per degree and what an empty outcome list does; FR-13 enumerates its defect
      classes so "fails loud" is eight assertions, not a feeling.
- [x] Success criteria are measurable — SC-1..SC-10, each mapped to a named suite in design
      **D9**; SC-7 is honestly labelled directional (the E14 SC-6 discipline), SC-10 is owned by
      the content PR.
- [x] Success criteria are technology-agnostic (taps, flags, renders, refuses; suites are named
      in the design, not in the SCs).
- [x] All acceptance scenarios defined (four stories × 4–7 Given/When/Then — 21 scenarios).
- [x] Edge cases identified (17 — including the two this epic is uniquely exposed to: a target
      whose derived state isn't loaded at pick time, and a template whose effects all resolve to
      `no_instance`).
- [x] Scope is clearly bounded (Position + Out of scope, with the E15-curation and slot-deduction
      items named as stop-and-flag rather than silently dropped).
- [x] Dependencies and assumptions identified (E8 and E9 closed; the engine's vocabulary and
      stacking core as consumers; the content half's ownership recorded as a binding split, not a
      TODO).

## Feature readiness

- [x] Every FR has a deciding acceptance criterion, and every **content-gated** FR is marked as
      such — the split is written into the requirement text (FR-3, FR-4) so a reader cannot
      mistake a code deliverable for a content deliverable.
- [x] User scenarios cover the primary flows (tap-cast, pre-warn decision, defect-on-load,
      non-library spells unchanged — US-1..US-4).
- [x] Feature meets the measurable outcomes in the Success Criteria, and the epic's one genuine
      product-intent fork (does a cast spend a slot?) is filed as a one-line decision rather than
      guessed at.
- [x] No implementation detail leaked into the specification: design.md owns the HOW; the spec's
      HOW-shaped sentences quote shipped facts (vocabulary, target kinds, the tier-seed precedent)
      as constraints, which is what they are.

## The one check unique to this epic

- [x] **No seat of this company authored, illustrated, or "examplified" a spell's content.** Two
      claims, each with the command that establishes it.
  1. *The only sample spell-shaped **data** in the pipeline is the labelled illustration.*
     `grep -n '"name"' contracts/spell-library-seed.md` returns exactly one spell name,
     `Example Cantrip`, inside the fenced illustration block (lines 34–67). That command greps
     one file for a JSON key: it establishes the claim about **data** and reaches no prose
     anywhere else — it is not evidence about `spec.md`.
  2. *The file the content would live in ships empty, and this PR touches no data file.*
     `git diff --stat origin/main...HEAD -- data/` returns nothing. (`git status --short data/`
     reads the working tree only — a **committed** data change leaves it clean and still rides in
     the diff, so it cannot establish this claim.)

      Spell **names** in prose are governed by FR-3's quoting exemption: a name carried in
      narrative is legal only when it is quoted from `docs/PRD.md` or `docs/EPICS.md` and cited
      there, and any other name is labelled notional in the sentence that uses it. The names in
      this pipeline are `Bless` and `Inspire Courage` (both cited to PRD/EPICS lines in
      `spec.md`) and nothing else outside the illustration block.

## Notes

Nothing blocking. Two things I would want a reviewer to push on, both restated in design **D9**'s
self-review: whether the conflicts read keyed on `(spell, outcome)` is the right latency at the
table, and
whether the sentinel-id tie-break claim (**D6**) survives contact with the shipped id allocation —
the plan turns the second into an assertion instead of an assumption.
