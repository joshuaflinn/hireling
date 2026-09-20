# Specification Quality Checklist: Database Schema (E2)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-20
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
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

- **Data-schema epic caveat (passes with intent, same shape as E1's)**: E2
  *is* an infrastructure epic — its "users" are the downstream epics
  (E4/E5/E7/E8/E12) and the operator, and the settled stack (Postgres 16 on
  Asgard, `sqlx migrate`, the `hireling` hall, the reference Pathbuilder
  export as contract) is part of the *requirement*, fixed by the epic
  decomposition and Constitution Article V. Those names are confined to the
  Constraints/Assumptions sections; FRs and Success Criteria are written
  testably without them (e.g. "versioned, ordered migrations with a tested
  reverse path", "second campaign creatable via data inserts alone, zero
  schema changes"). This is the maximum tech-agnosticism a schema spec can
  honestly carry.
- **No [NEEDS CLARIFICATION] markers**: every ambiguous point had a
  reasonable default; the defaults are recorded in Assumptions. The three
  most consequential: (1) **hard delete** as the single delete policy
  (ended effects are state, not deletion; claim history append-only
  regardless) — the epic guardrail demanded one decision applied
  consistently, and hard-delete-plus-state is the grug answer; (2)
  **per-field versions modeled conceptually** (column-vs-sidecar left to
  brainstorm/plan — both satisfy FR-14, and picking one here would be
  design, not specification); (3) **P1 stash/bank = present-but-unused**
  tables shipped in these migrations.
- **FR → scenario/SC traceability spot-check**: FR-1→US1/SC-1 · FR-2,FR-3→
  US1-sc4/SC-8 · FR-4,FR-5→US2/SC-2,SC-3 · FR-6→SC-4 · FR-7→US3-sc3 ·
  FR-8,FR-9→US3/SC-5 · FR-10→US2 (effects party-scoped) · FR-11→US3 ·
  FR-12,FR-15→US6/SC-7 · FR-13→US5 · FR-14→US4/SC-6 · FR-16→Constraints
  (E8's "never stores derived numbers as editable state"). Every FR maps to
  at least one acceptance scenario or success criterion.
- **Validation iteration 1**: all items pass.
