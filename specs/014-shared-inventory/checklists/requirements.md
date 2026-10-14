# Specification Quality Checklist: Shared Inventory (E12)

**Purpose**: Validate specification completeness and quality before review.
**Created**: 2026-10-14 · **Validated**: 2026-10-14 (Korrin, first iteration)
**Feature**: [spec.md](../spec.md) · gh#14

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — wire field
      names and table names are quoted as *data and shipped facts the epic
      joins*, per the house convention (E7/E14 specs name their contract
      surfaces); no tech choices are made in the spec — the design owns
      those.
- [x] Focused on user value and business needs — US-5, the loot workflow,
      the argument-settling log.
- [x] Written for non-technical stakeholders (Position, user stories,
      FR-2's consequences prose) with engineer-precision where acceptance
      demands it (FR-8's replay table).
- [x] All mandatory sections completed.

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain (Clarify Log: seven
      decisions settled in place with reasoning — five at specify, two
      more at round-1 review; none needs Josh; none moves the P0/P1/P2
      line).
- [x] Requirements are testable and unambiguous — FR-2 names the version
      unit and apply semantics per surface; FR-8's six replay cases each
      carry a numeric criterion; FR-6's match outcomes enumerate to found/
      flagged deterministically.
- [x] Success criteria are measurable (SC-1..SC-8, each mapped to a bench
      suite in design D8).
- [x] Success criteria are technology-agnostic (renders/credits/refuses/
      fan-out language; the suites are named in the design, not the SCs).
- [x] All acceptance scenarios defined (five stories × 3–4 Given/When/Then).
- [x] Edge cases identified (fourteen — duplicate adds, concurrent
      over-draw, mint-on-transfer-in, negative effective quantity, partial
      sells, name collisions, pp conversion, the absent quartermaster
      (round-1 finding 5), QM deletion, renames, log growth with the cap
      recorded as a decision, offline book value, post-delete replay mint,
      solo party).
- [x] Scope is clearly bounded (Position + Out of scope's stop-and-flag
      items, coin transfer first among them + design D9's whole-diff-
      surface file map).
- [x] Dependencies and assumptions identified (E10/E4 closed; E7 contracts
      binding; four assumptions recorded).

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria (FR table
      rows and the SC suite mapping).
- [x] User scenarios cover primary flows (dump, sell, audit, bank,
      offline-replay — US-5 whole).
- [x] Feature meets measurable outcomes defined in Success Criteria.
- [x] No implementation details leak into specification (design.md owns
      the HOW; the spec's HOW-shaped sentences quote shipped facts —
      schema rules, contract vocabulary — as constraints, which they are).

## Notes

- First-iteration pass: all items pass. The two prompt-named traps
  (reconciliation granularity, replay idempotence) are the spec's two
  longest FRs, by design — a weaker criterion there passes a broken build.
- Round-1 revision (MOR-464 verdict, 2026-10-14): findings 1–8 folded —
  authz variant payloads as deciding facts (D4), the stash identity
  unique constraint (D1.5), the bank-adjustment note dropped (Clarify
  #6), R1's fourth asserted surface (counterparty inventory), the
  absent-quartermaster escape (Clarify #7), the real file map
  (`src/book_value.rs` via the tree's `#[path]` pattern), no quoted pp
  rate, the corrected corpus citation. Re-validated against this list;
  all items still pass.
- The plan is TDD-ordered for an implementation seat with zero context;
  the hold-free lane status is stated at its head (checked against the
  board, not assumed).
