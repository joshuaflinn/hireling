# Buff/Effect Engine (E8) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pure, exhaustively-tested modifier engine that recomputes every derived stat on affected sheets when effects change, emits full provenance including suppressed sources, extends E7's wire with effect writes and derived broadcasts, and drives the condition picker from corpus data — the numbers E6/E10 render.

**Architecture:** workspace crate `engine/` (serde only) for vocabulary + stacking + output assembly; `src/engine_host/` in the binary for base extraction, DB loading, and recompute; `src/sync/` extended with effect write frames, the `derived` frame, and a snapshot derived array. One small migration. Clients never compute.

**Tech Stack:** Rust (existing), `proptest` as a new dev-dependency of `engine/` (property tests are a spec non-negotiable), sqlx (existing), no new web dependencies.

**Spec:** `specs/008-buff-effect-engine/spec.md` (approved + clarified), `design.md`, `contracts/engine-output.md` — all in this directory; this plan argues from them, executors read all three.

## Global Constraints

- Branch: `feat/10-buff-effect-engine` (exists @ `4c2cfa0`; Task 0 merges `origin/main` first). **Claim gh#10 (self-assign) before the first code commit — sync point 1.** Conventional commits, `(#10)` suffix, imperative.
- Never commit to `main`; never force-push. `just ci-local` green before PR. PR body: `Closes #10` + link to the Paperclip task + decisions-with-alternatives (from `design.md`).
- Lint law unchanged (`unwrap_used`/`expect_used`/`panic`/`indexing_slicing` deny; pedantic warns; no `#[allow]`). Name intermediate variables (grug).
- **Engine bugs are stop-the-line (Constitution Article IV).** Test-first ordering below is not stylistic: every task's tests exist before its implementation compiles green.
- Zero condition names in engine code (review criterion; `grep -riE "frightened|bless|concealed" engine/` must return nothing but comments citing worked examples).
- Vocabulary closed: single stats (11), blankets (3), `skill:<name>` with core skills bare and lores as `lore:<name>`. Expansion sets are data. `all_checks` NEVER contains `damage`/`speed`; `all_dcs` = ac + class_dc + spell_dc.
- Stacking: highest bonus per type once; worst penalty per type once (bonuses and penalties separate); untyped stacks fully both ways; blankets expand before evaluation; ties broken by `(effect_id, ord)`.
- Values ∈ −50..=50; name ≤120 chars; ≤16 modifiers per effect; targets ⊆ party roster; creator-only writes; GM writes nothing (per-frame authz, deny-by-default).
- E7 semantics reused verbatim: `op_id` idempotency (`client_ops`), whole-effect CAS on `base_version`, five outcomes, per-message `authorize()`.
- Derived values are NEVER stored. No cache table. Recompute on commit + on snapshot build.
- Boundary: `cargo tree -p hireling-engine` shows serde-family only — asserted in CI (Task 12) and by review.

## File Structure

| File | Responsibility |
|---|---|
| `engine/Cargo.toml`, `engine/src/lib.rs` | workspace crate, dep: `serde` (+ `proptest` dev) |
| `engine/src/vocab.rs` | `Stat`, `ModifierType`, `CORE_SKILLS`, `BLANKET_EXPANSIONS`, `parse_stat`, validation |
| `engine/src/model.rs` | `Modifier`, `ActiveEffect`, `BaseStats`, `EngineOutput`, `ProvenanceEntry`, `SuppressedEntry`, `Chip` (serde types = contract) |
| `engine/src/stack.rs` | expand → group → select → total; suppressed reasons |
| `engine/src/compute.rs` | `EngineOutput` assembly: per-instance axes, null bases, chips |
| `engine/tests/wex.rs` | WEx-1…12 named tests (one `#[test]` per worked example) |
| `engine/tests/props.rs` | property tests: expansion closure, stacking invariants, determinism |
| `src/engine_host/extract.rs` | `base_sheet` + vitals + lores → `BaseStats` (design math table) |
| `src/engine_host/load.rs` | effects/targets/modifiers + corpus → `ActiveEffect[]` |
| `src/engine_host/apply.rs` | condition mapping resolution (constant / `condition_value × polarity`), `tracked_manually` |
| `src/engine_host/recompute.rs` | party state → `EngineOutput` per character |
| `src/engine_host/tests/extract_golden.rs` | reference-export golden fixture |
| `migrations/20261002000001_effects_corpus.sql` (+ `.down.sql`) | `effects.tracked_manually`, `effects.corpus_entry_id` |
| `src/sync/protocol.rs` (modify) | effect write frames, `derived` frame |
| `src/sync/write.rs` (modify) | effect ops: validation → authz → ledger → CAS → commit |
| `src/sync/session.rs` (modify) | on-commit recompute → diff + `derived` fan-out |
| `src/sync/snapshot.rs` (modify) | `derived` array per roster character |
| `src/http.rs` (modify) | 3 read-only routes + authz matrix rows |
| `src/import/seed.rs` (modify) | constants → re-export engine vocabulary (D2, mechanical) |

### Task 0: Base merge + claim + crate scaffold
- [ ] Merge `origin/main` into `feat/10-buff-effect-engine`; resolve spec-doc conflicts if any.
- [ ] **Self-assign gh#10 and comment the branch name (sync point 1).**
- [ ] Add `engine/` crate to the workspace: `lib.rs` doc comment stating the boundary (no UI/transport/DB; port-not-rewrite), `Cargo.toml` with `serde` only. Workspace builds.
- Done when: `cargo build` green; `cargo tree -p hireling-engine` lists serde-family only; gh#10 assigned to you.

### Task 1: Vocabulary (tests first)
- [ ] Write `engine/tests/wex.rs` skeleton + vocab tests: closed-set rejection (unknown stat errors; `skill:` prefix parses; `skill:` bare rejected), expansion data assertions (`all_checks` members per design incl. NOT damage/speed; `all_dcs` = ac+class_dc+spell_dc; union exact).
- [ ] Implement `vocab.rs` to green: `Stat`, `CORE_SKILLS` (the export's 18), lore naming (`lore:<name>`), `BLANKET_EXPANSIONS` as data consumed by a pure `expand(blanket, stat_instances)` function.
- Done when: vocab tests green; expansion is data + one function, no per-stat match arms in engine code.

### Task 2: Stacking core (WEx first — the stop-the-line tripwire)
- [ ] Write all twelve WEx tests (`WEx-1`…`WEx-12`, names verbatim from spec table, each citing its Player Core rule in a doc comment).
- [ ] Implement `model.rs` + `stack.rs`: expand → group per stat instance → per type select (max bonus / worst penalty) → untyped all → total; emit `ProvenanceEntry` and `SuppressedEntry` with reasons `same-type-lower-bonus` / `same-type-lighter-penalty` / `same-type-tie` + `suppressed_by_effect_id`; ties by `(effect_id, ord)`.
- Done when: WEx-1…12 green; a deliberate mutation (let same-type stack) fails ≥1 WEx test — verify by mutating, running, reverting; record the mutation check in the task comment.

### Task 3: Properties + determinism
- [ ] Add `proptest` dev-dep. Properties: (a) order independence — shuffled modifier lists give byte-identical output; (b) conservation — applied ∪ suppressed over all instances = every expanded modifier, nothing vanishes; (c) total = base + Σ applied values; (d) expansion closure — blanket expansion over generated skill sets matches the data table.
- Done when: property suite green with ≥256 cases per property in CI config.

### Task 4: Output assembly (`compute.rs`)
- [ ] Tests: per-strike/per-caster instances (two-caster fixture from the reference character); null bases (`class_dc: null` keeps null, still lists provenance); zero-value modifiers show applied +0; chips carry `tracked_manually`; empty-effects output = bases with empty lists.
- [ ] Implement `compute.rs` to green. Output serde types match `contracts/engine-output.md` exactly (field names, camel/snake per contract JSON).
- Done when: a golden EngineOutput JSON fixture committed under `engine/tests/fixtures/` round-trips.

### Task 5: BaseStats extraction (golden fixture)
- [ ] Commit the reference `#pbExport` JSON as a fixture. Write `extract_golden.rs` asserting every derived base equals the prototype's rendered values (perception, saves, skills incl. lores, both caster blocks' attack/DC, AC, speed, strikes) — prototype is display truth; mismatches adjust the math table, never a value.
- [ ] Implement `src/engine_host/extract.rs` per the design math table (`eff_level = level + level_adjust` clamped; untrained adds nothing; `spell_dc = spell_attack + 10`; duplicate weapon names `name#2`).
- Done when: golden test green for the fixture at `level_adjust` ∈ {0, +1, −1}.

### Task 6: Migration + DB load + condition apply
- [ ] Write migration (+down): `effects.tracked_manually boolean NOT NULL DEFAULT false`, `effects.corpus_entry_id bigint NULL REFERENCES corpus_entries(id)`. Reversible, E2 conventions.
- [ ] `load.rs`: party effects → `ActiveEffect[]` (targets, ordered modifiers, version).
- [ ] `apply.rs` tests (unit, seeded corpus rows): engine_math valued condition at N → signed resolved modifiers (`frightened 2` → `{status, all_checks_and_dcs, −2}`); constant mapping; display-only tier → zero modifiers + `tracked_manually=true`; invalid corpus stat → loud error at apply, never a silent row.
- [ ] Implement `apply.rs` to green.
- Done when: migration up/down clean on the compose DB; apply tests green.

### Task 7: Wire extension — effect writes (integration, production path)
- [ ] `protocol.rs`: effect write frames (`effect_new` create / `effect` update+end), `derived` frame types; unknown ops dropped+logged (E7 deny-by-default).
- [ ] `write.rs`: effect ops — validation (name ≤120, ≤16 modifiers, stat via engine validator, values −50..=50, targets ⊆ roster, source owned by writer) → per-message authz (creator-only; GM `forbidden`) → ledger dedupe → CAS on `effects.version` → commit; outcomes reuse the five.
- [ ] Integration tests through the real router + real WS (E7 harness patterns): create→applied→diff; retarget CAS supersession; replay op → `already_applied`; GM/non-creator → `forbidden`; invalid stat → `rejected` with human-readable reason; corpus-condition create resolves via `apply.rs`.
- Done when: suite green; E7's existing sync suite untouched and green.

### Task 8: Recompute + broadcast + snapshot
- [ ] `recompute.rs`: affected = old ∪ new targets; compute per character in the same commit path; session fans out `diff` then `derived` frames per affected character; snapshot gains `derived` array for every roster character.
- [ ] Integration tests: apply → diff+derived in order on a second client; retarget moves exactly the delta (departing sheet reverts, arriving sheet rises); end restores priors; display-only condition → chip on `derived`, zero numeric delta; reconnect → snapshot carries current derived; non-targeted character byte-identical.
- Done when: suite green; latency smoke: derived fan-out adds <50 ms p95 at POC load (1 party, 6 clients) alongside E7's 100 ms budget.

### Task 9: REST reads + authz matrix
- [ ] `GET /api/parties/{id}/conditions` (picker: name, tier, valued — from corpus rows, not code), `GET /api/parties/{id}/effects` (active effects, all sources), `GET /api/characters/{id}/derived` (EngineOutput bootstrap). Session-gated, party-scoped, read-only; declarative authz matrix rows in `http.rs` (deny-by-default).
- [ ] Tests: each route drives the real router (PR #30 rule); GM reads succeed, writes nowhere exist; 403 payload shape per E3.
- Done when: routes green + ownership matrix shows the three reads party-readable.

### Task 10: D2 re-export + contract ratification
- [ ] `src/import/seed.rs`: constants replaced by engine re-exports (mechanical; importer tests stay green — they now validate against the engine's vocabulary).
- [ ] PR-amend `specs/007-party-sync/contracts/wire-protocol.md` §3 (effect write row + `derived` frame) and §8 (E8 extension realized, close-code reservations unchanged); flip `contracts/engine-output.md` status to **binding**; verify the E6 spec's reference still resolves.
- Done when: importer suite green; wire-protocol diff reviewed against this plan's frame shapes.

### Task 11: Stop-the-line audit (author self-review before review)
- [ ] `grep -riE "frightened|bless|concealed|inspire" engine/ src/engine_host/ src/sync/` — only worked-example citations allowed.
- [ ] `cargo tree -p hireling-engine` — serde-family only, pasted in the task comment.
- [ ] Mutation check: break one stacking rule, one expansion member, one provenance field — each fails a named test. Paste results.
- Done when: all three pasted on the Paperclip task.

### Task 12: Gate + PR
- [ ] `just ci-local` green (fmt, clippy-deny block, tests, cargo-deny); add the boundary assertion to the recipe or a CI test.
- [ ] Open PR: `Closes #10`, link the Paperclip implementation task, decisions-with-alternatives from design.md, WEx list + mutation evidence in the body. Grizzly-gate must pass; an owner merges.
- Done when: PR open, gate green, Orsik's review task carries the verdict.

## Self-review (run by the plan author)

- Every spec FR maps to tasks: FR-1 (T0/T11), FR-2 (T1), FR-3 (T2/T3), FR-4 (T2/T4), FR-5 (T8), FR-6 (T2/T4 + contract T10), FR-7 (T7), FR-8 (T6/T7/T9), FR-9 (T7/T8/T9), FR-10 (T2/T3/T11 — suite first, mutation-checked).
- Test-first ordering holds: every engine behavior has tests written before its implementation in the same task; integration tasks assert through the production path.
- No P1/P2 leakage: conflict pre-warning, spell library, ability-scoped vocabulary absent.
- Blast radius testable end-to-end: Task 8's retarget delta test is the spec's US-1 acceptance scenario verbatim.
