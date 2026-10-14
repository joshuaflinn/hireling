# Specification Quality Checklist: Go-Live Deploy (E11)

**Purpose**: Validate specification completeness and quality before proceeding to implementation
**Created**: 2026-10-14
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — *scoped: deployment artifacts (Dockerfile, container, hosts) are the feature's subject matter, named as WHAT the deploy consists of; no framework/API leakage beyond the settled stack facts the PRD itself states*
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders — *the handoffs and checklists are; the runbook-facing FRs name exact values by design (operator audience)*
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain (see Clarify Log — all six settled from the corpus; none required Josh)
- [x] Requirements are testable and unambiguous (each FR cites its repo source; each SC names its command + observable)
- [x] Success criteria are measurable (SC-1–SC-8 each state what to run and what result counts)
- [x] Success criteria are technology-agnostic where the feature allows — *a deploy epic's criteria are necessarily container/URL-shaped; they are phrased as checkables, not tooling*
- [x] All acceptance scenarios are defined (US1–US5)
- [x] Edge cases are identified (hall unreachable, tunnel/app split, pre-import boot, cookie rotation, SIGTERM mid-broadcast, rotation missing the hall, host reboot)
- [x] Scope is clearly bounded (Out of scope guarded; bench/host split explicit)
- [x] Dependencies and assumptions identified (Assumptions, incl. the deploy-time-verified asgard-net assumption with its stop-the-line clause)

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows (deploy, first use, monitoring, update, session night)
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification beyond the deployable's own shape

## Task-ask coverage (from MOR-384's definition of done)

- [x] Success criteria checkable against a running container — SC-1/2/3 are curl/log/inspect-shaped; SC-4/5 browser-shaped on the deployed origin
- [x] The three Josh handoffs named as handoffs — H1/H2/H3 in a binding section, restated where they bite (FR-8, SC-3/SC-6 annotations, go-live checklist)
- [x] gh#179's disposition stated — folded as FR-9/SC-5; the issue remains the record for its four observations
- [x] Explicit statement of what is buildable/verifiable without touching Josh's hosts — dedicated section + Assumptions + plan Task 5 sweep

## Notes

- Validation run 2026-10-14 (Korrin): all items pass; spec carried into `design.md` (D1–D7) and `plan.md` (Tasks 1–5) same day.
- Re-validated 2026-10-14 after PR #187 round-1 reject (review 5512531846): F1 fixed — FR-5 now carries the complete env table incl. `HIRELING_ALLOWLIST`/`HIRELING_GM_SUB`, sourced from `Settings::from_env` + `AuthSettings::from_env` together ("testable and unambiguous", "dependencies and assumptions" re-ticked on it); F3 — out-of-scope list now names the probe binary as the one new code artifact ("scope is clearly bounded" re-ticked); F4 — bench/deploy split corrected (bench at implementation PR: SC-7, SC-8; deploy: SC-1–SC-6; spec PR: none), SC-4/SC-5 annotated *(handoff)* ("success criteria are measurable" re-ticked). All items still pass.
