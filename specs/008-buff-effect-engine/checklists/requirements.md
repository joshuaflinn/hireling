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

- [ ] No [NEEDS CLARIFICATION] markers remain — **three open, posted on clarify card (Q1–Q3)**:
  - **Q1 — per-strike / per-caster stat instances**: `attack`/`damage` per strike and `spell_attack`/`spell_dc` per caster block (the reference character has two caster blocks). Recommended default: engine emits per-instance derived entries; blanket targets hit every instance; the closed vocabulary stays closed (instances are an axis, not new stats).
  - **Q2 — the skill set**: `skill:<name>` and `all_checks` expansion = core PF2e skills only, or core + the character's lore skills? Recommended default: core skills always present on every sheet + the character's lores (a lore check is a skill check; otherwise *frightened* under-counts).
  - **Q3 — where `EngineOutput` is computed for clients**: server-side recompute riding E7's effect diff/snapshot (clients render only — one engine, one path) vs a client-side engine shared with the server. Recommended default: server-side; extends E7's snapshot with derived payloads.
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
- Q1–Q3 markers resolve via the human clarify card; answers fold back
  into FR-2/FR-5, the vocabulary table, and the contract before the
  design step runs.
