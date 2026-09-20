# Specification Quality Checklist: Authentik OIDC Auth & Ownership Enforcement (E3)

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

- **Tech-name quarantine (same precedent as E1)**: "Authentik" and "OIDC"
  are settled house constraints and appear only in the title, Constraints,
  and Assumptions (and in the epic/issue citation). FRs and Success Criteria
  are written testably without them ("delegated login at the house identity
  provider", "rejected 100% of the time with the standard forbidden
  payload"). Iteration 1 scrubbed stray "OIDC" mentions from FR-1 and the
  Account key entity; a final grep confirms the remaining mentions are all in
  the quarantined sections (title, Constraints, Assumptions).
- **No [NEEDS CLARIFICATION] markers — deliberate**: every fork had a
  reasonable default, recorded in Assumptions: `sub`-claim account mapping,
  GM-as-config-flag, ~1-week sliding session lifetime, dev-only fake-session
  mechanism (inert in prod), and the E5/E7 boundary lines. The clarify step
  should pressure-test the session-lifetime default and the dev-bypass
  shape, but neither blocks planning.
- **Ownership matrix coverage check (iteration 1)**: enumerated the matrix
  against the PRD's FG2 ownership paragraph — character owner sole writer
  (FR-8), effect creator sole writer even on other sheets (FR-9), creation
  open to any character owner (FR-10), custom-lane creator-ownership
  (FR-11, serving E9), GM writes nothing (FR-12), reads unrestricted
  (FR-13). Each has a matching user story (US-2…US-5) and success criterion
  (SC-1…SC-4). FR-11 is the one row not in the epic prompt verbatim; it
  comes from PRD FG1's "creator is that row's sole writer (FG2 semantics)"
  and is included because E3 is where FG2 semantics are defined.
- **Boundary calls documented in Assumptions**: ownership *claim* mechanics
  live in E5 (E3 defines the semantics, FR-8); realtime transport lives in
  E7 (E3 requires the sync channel to authenticate like any endpoint, FR-6
  and Edge Cases); audit-record storage shape is E2/plan scope (E3 defines
  the events and fields, FR-15).
- Validation iteration 1: all items pass after the FR-1 scrub.
