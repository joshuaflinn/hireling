# Specification Quality Checklist: Buff/Effect Engine (E8)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-01
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — stack names confined to Constraints per house convention
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain — **all three resolved 2026-10-01 via clarify card `af988c85`** (all recommended defaults chosen):
  - **Q1 → per instance**: engine emits one derived entry per strike and per caster block; blanket targets hit every instance; the closed vocabulary stays closed.
  - **Q2 → core skills + lores**: core skills on every sheet, plus that character's lores; blanket targets hit lores too.
  - **Q3 → server-side recompute**: one Rust engine, one computation path; derived payloads ride E7's effect-commit broadcast and the catch-up snapshot; clients render and never compute.
  - Folded into spec FR-2/FR-5, the vocabulary table, the edge-case list, and the contract's §0/§1.
- [x] Requirements are testable and unambiguous (pending Q1–Q3 folds)
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- **Engine-epic caveat (same intent as E4/E7's)**: E8 ships one internal
  surface (the engine module + effect write paths + the wire extension)
  rather than player screens; its "users" are the table plus E6/E10/E13.
- **Numbers/data this spec settles** (deliberately pushed down from the
  PRD, per the tasking): the expansion sets as data; the named Player Core
  worked examples (WEx-1…12) the test suite asserts; the blast-radius
  definition of "affected"; the provenance payload shape (with suppressed
  entries) pinned in `contracts/engine-output.md`; the corpus read path
  for tiers/mappings (`corpus_entries.data.import.tier`, `modifiers`
  jsonb, `condition_value × polarity`).
- **Seam**: `contracts/engine-output.md` is the ONE contract both E8 and
  E6 (and later E10) bind to — it is drafted with this spec so the
  parallel E6 spec references it from day one, and is ratified at the E8
  design gate.
- Q1–Q3 markers resolved via the human clarify card 2026-10-01; answers
  folded into FR-2/FR-5, the vocabulary table, the edge cases, and the
  contract before the design step ran.
