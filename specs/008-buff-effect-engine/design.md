# Design: Buff/Effect Engine (E8)

**Status**: for the consolidated design+plan gate · spec approved + clarified (cards `8d4b455d`, `af988c85`)
**Inputs**: `spec.md` (this dir, approved); E2 `data-model.md` §4 (effects DDL, landed); E4 `src/import/seed.rs` + `data/seed/condition-tiers.json` (landed); E5 `specs/005-pathbuilder-import/data-model.md` + `contracts/pb-export.md` (landed); E7 `contracts/wire-protocol.md` + `contracts/degraded-mode.md` + `src/sync/` (landed). This design argues from those, never re-designs them.

## Decisions at a glance

| # | Decision | Alternative rejected |
|---|---|---|
| D1 | Engine is a **workspace member crate** `engine/` (lib `hireling-engine`), deps: `serde` only | `src/engine/` module in the binary crate — boundary would be convention, not mechanics |
| D2 | **One vocabulary source**: engine crate owns `Stat`/expansion data; E4's `src/import/seed.rs` constants switch to re-exporting engine types | duplicated lists in import + engine (drift) |
| D3 | BaseStats extraction is a **module in the binary** (`src/engine_host/extract.rs`) — it reads `base_sheet` jsonb; only the *math* is pure-tested via engine fixtures | extraction inside the engine crate (would pull serde_json shapes of E5 into the "portable" crate) |
| D4 | Derived output is **computed on effect commit and on snapshot build**, broadcast with the effect diff; **never stored** | derived cache table (settled against), client-side compute (Q3 rejected) |
| D5 | Effect writes are **WS frames** (`kind:"effect"`), whole-effect CAS, reusing E7's `client_ops` ledger and outcomes verbatim | REST writes (E7 settled: live-state writes enter through the WS) |
| D6 | New `derived` server→client frame + `derived` array in snapshot; no versioning of derived (pure function of versioned state; WS send order + snapshot atomicity cover it) | derived as a versioned field kind (implies storage — settled against) |
| D7 | Condition-sourced effects: **one `effects` row**, corpus-resolved modifiers frozen at apply; migration adds `tracked_manually boolean` + `corpus_entry_id bigint NULL` | separate condition-tracking table (two mechanisms — PRD says one) |
| D8 | Suppression ties broken by stable order `(effect_id, ord)`; emitted reason `same-type-tie` | first-write-wins (nondeterministic across replay orders) |
| D9 | `skill:<name>` names: core skills bare (`acrobatics`), lores as `lore:<name>` — matching E5's normalized lores | free-form skill strings (no closed set to expand against) |

## Architecture

```
                    ┌──────────────────────────── engine/ (crate: hireling-engine) ─────────┐
                    │  vocab.rs      Stat enum, CORE_SKILLS, BLANKET_EXPANSIONS, validation │
                    │  model.rs      ModifierType, Modifier, ActiveEffect, BaseStats,        │
                    │                EngineOutput, ProvenanceEntry, SuppressedEntry, Chip    │
                    │  stack.rs      expand → group → select → total; provenance + suppressed│
                    │  compute.rs    EngineOutput assembly (per-instance axes, null bases)   │
                    │  (serde only; no axum/sqlx/tokio — enforced by cargo tree in CI task)  │
                    └───────────▲───────────────────────────────▲───────────────────────────┘
                                │ types                          │ types
┌────────────────────────── src/engine_host/ (binary crate) ─────┴───────────────────────────┐
│  extract.rs   base_sheet + vitals(level_adjust) + lores → BaseStats (math table below)     │
│  load.rs      effects + effect_targets + effect_modifiers + corpus rows → ActiveEffect[]    │
│  apply.rs     condition mapping resolution (constant | condition_value × polarity)          │
│  recompute.rs party state → EngineOutput per character (calls engine crate)                 │
└───────────▲───────────────────────────────▲───────────────────────────────────────────────┘
            │                               │
┌───────────┴──────── src/sync/ (E7, extended) ────────────┐   ┌──────────── src/http.rs ────────┐
│  protocol.rs: effect write frames + `derived` frame      │   │ GET /api/parties/{id}/conditions│
│  write.rs:     effect ops → CAS on effects.version       │   │ GET /api/parties/{id}/effects   │
│  session.rs:   on-commit → recompute → diff + derived    │   │ GET /api/characters/{id}/derived│
│  snapshot.rs:  + derived array per character             │   │ (read-only; authz matrix rows)  │
└──────────────────────────────────────────────────────────┘   └─────────────────────────────────┘
```

The engine crate is the RPGMastermind-portable core (FR-1): harvesting it
means pointing a new host at `engine/` and writing a new `engine_host`. The
review criterion is mechanical: `cargo tree -p hireling-engine` shows
serde-family deps only.

## The BaseStats math table (D3 — the export carries ranks, not totals)

Verified against the reference export (`#pbExport`): precomputed totals
exist ONLY for AC (`acTotal.acTotal`), strikes (`weapons[].attack`,
`weapons[].damageBonus`), and speed (`attributes.speed +
attributes.speedBonus`). Everything else is rank + ability, and the
extractor derives it:

| Stat | Base formula | Source fields |
|---|---|---|
| `ac` | verbatim total | `ac.acTotal` |
| `speed` | verbatim | `attributes.speed + attributes.speedBonus` |
| `fort`/`ref`/`will` | `eff_level·(rank≥1) + abil_mod + prof_bonus(rank)` | `proficiencies.{fortitude,reflex,will}`, `abilities` (con/dex/wis) |
| `perception` | same, wis | `proficiencies.perception` |
| `class_dc` | `10 + eff_level·(rank≥1) + keyabil_mod + prof_bonus(rank)`; `null` if rank = 0 | `proficiencies.classDC`, `keyability` |
| `skill:<core>` | save formula with the skill's fixed ability | `proficiencies.<skill>` (18 core keys) |
| `skill:lore:<name>` | save formula, int for lore skills | `lores[]` |
| `strikes[].attack` / `.damage_flat` | verbatim | `weapons[].attack`, `.damageBonus` |
| `casters[].spell_attack` | `eff_level·(rank≥1) + abil_mod + prof_bonus(rank)` per block | `spellcasters[].{ability,proficiency}` |
| `casters[].spell_dc` | `spell_attack + 10` per block | same |

Where `eff_level = identity.level + vitals.level_adjust` (clamped 1..20),
`abil_mod = ⌊(score − 10) / 2⌋`, `prof_bonus: 0→+0 (no level), 1→+2,
2→+4, 3→+6, 4→+8` (level added only at rank ≥ 1 — untrained adds neither
level nor bonus). **Golden test**: the reference export's derived values
must equal the prototype's rendered numbers for Lorum Ipsum; any mismatch
is resolved in favor of the prototype (it is Dave's display truth) by
adjusting the table, never by patching a value.

Damage bases: `weapons[].die` + `damageBonus` → the sheet renders the roll
string; the engine output carries `damage_flat` (the number a flat damage
modifier adjusts), keyed by strike `key = name` (E5's verbatim weapon
objects; duplicate weapon names get a stable `name#2` suffix rule in the
extractor, tested).

## Recompute and the wire (D4–D6)

On an applied effect write, inside the same commit path as the version
bump: load party effects → for each affected character (old ∪ new targets)
→ `recompute(character)` → enqueue, in order: the `diff` (effect field,
unchanged E7 shape) then one `derived` frame per affected character. WS
send order is TCP-ordered per connection, so every client applies the
effect change and its derived consequence in the right sequence. The
catch-up snapshot gains `"derived": [ EngineOutput per roster character ]`.
A `derived` frame is a pure function of already-versioned state — a client
that reloads cold gets the snapshot and loses nothing by skipping frames.

Effect write frames (amending `wire-protocol.md` §3/§8 by PR, per its own
extension rule):

```jsonc
// client → server
{"t":"write","op_id":"<uuid4>","target":{"kind":"effect","effect_id":41},
 "base_version":1042,"value":{"op":"update","targets":[7,9]}}        // retarget
{"t":"write","op_id":"<uuid4>","target":{"kind":"effect_new","party_id":1},
 "value":{"op":"create","name":"Bless","source_character_id":3,
          "targets":[7,9],"modifiers":[{"type":"status","stat":"attack","value":1}],
          "duration_note":"10 rounds","corpus_entry_id":null,"condition_value":null}}
{"t":"write","op_id":"<uuid4>","target":{"kind":"effect","effect_id":41},
 "base_version":1045,"value":{"op":"end"}}
// server → client (new)
{"t":"derived","character_id":7,"output":{ …EngineOutput… }}
```

Semantics reused verbatim from E7: `op_id` idempotency via `client_ops`,
CAS on `base_version` (`superseded` on miss), outcomes
`applied|superseded|already_applied|rejected|forbidden`, per-message authz
(creator's account only; GM never). `create` validates: name non-empty ≤120
chars, ≤16 modifiers, each modifier's `stat` against the closed vocabulary
(engine crate's validator — one source, D2), values ∈ −50..=50, targets ⊆
party roster, `source_character_id` owned by the writer, corpus-sourced
creates resolve modifiers via `apply.rs` (D7: `condition_value × polarity`,
stored signed; `tracked_manually=true` for display-only tier, zero
modifiers). `end` sets `active=false`. `rejected` carries a human-readable
reason, always.

## Storage (D7) — one migration, E2-conventioned

`migrations/20261002000001_effects_corpus.sql` (+ `.down.sql`):

- `effects.tracked_manually boolean NOT NULL DEFAULT false` — frozen at
  apply; a seed-tier flip mid-session does not rewrite history (the
  effect's math was resolved at apply and stays).
- `effects.corpus_entry_id bigint NULL REFERENCES corpus_entries(id)` —
  provenance link; E9's tooltips key off it later. Display-only chips
  carry `tracked_manually` for the badge without a corpus join.

No other DDL. Derived values are never stored (settled); effects tables
and `field_version_seq` are reused as landed.

## Testing strategy (FR-10 — stop-the-line ordering)

1. **Engine crate, test-first**: WEx-1…12 as named unit tests written
   before `stack.rs` compiles green; expansion-membership tests before
   `vocab.rs`; determinism (byte-identical output under input permutation)
   before `compute.rs`. Property suite (`proptest` dev-dep): expansion
   closure over generated stat sets; stacking invariants (order
   independence; applied ∪ suppressed = every expanded modifier; total =
   base + Σ applied).
2. **Extraction golden**: reference export fixture → expected BaseStats
   matching the prototype's rendered sheet.
3. **Integration (production path, PR #30 rule)**: real WS through the
   real router — apply/retarget/end produce diff+derived in order; CAS
   supersession; op replay `already_applied`; display-only condition =
   zero derived delta; GM write → `forbidden`; non-creator write →
   `forbidden`; snapshot catch-up carries derived after reconnect.
4. **Boundary check in `just ci-local`** (added to the recipe or a test):
   `cargo tree -p hireling-engine` asserts no axum/sqlx/tokio — the
   port-not-rewrite criterion, mechanical.

## Alternatives considered

- **Client-side engine (TS/WASM)**: rejected at clarify (Q3) — second
  computation path, exactly what the PRD's one-source-of-truth rule
  forbids; PWA payload bloat; drift risk between two stacks.
- **Derived-values cache table**: rejected — settled non-negotiable
  (derived never stored); POC scale (≤6 sheets) makes recompute-on-commit
  cheap and always-correct.
- **REST effect writes**: rejected — E7's settled ruling (live-state
  writes enter through the party WS); REST stays read-only here.
- **Effects-by-condition tracking table**: rejected — PRD demands one
  mechanism both directions; a display-only condition is an effect with
  zero modifiers and a badge flag.
- **Stat as bare strings end-to-end**: rejected — enum + `skill:<name>`
  parse in one validator; strings everywhere would let typos become
  silent no-ops (the exact class of engine bug stop-the-line fears).

## Coordination points (handed onward)

- **E6 (Brynn, MOR-45)**: renders strictly from
  `contracts/engine-output.md`; the `derived` frame + snapshot section are
  her sync-store's new input; effect chips carry `tracked_manually`.
- **E10**: renders party cards from the same `derived` payloads; no second
  path exists to build.
- **E9**: `corpus_entry_id` on condition-sourced effects is the tooltip
  join key.
- **E13 (P1)**: conflict pre-warning reads the engine's suppressed data —
  the provenance shape already carries what it needs.
- **E4 (Vidda)**: seed validation switches to engine vocabulary types
  (D2) — mechanical re-export, no behavior change; lands in this epic's
  PR with Vidda's nod in review.

## Risks

- **Base-math mismatch with the prototype** (extractor derives what the
  export doesn't precompute): mitigated by the golden test against the
  prototype's rendered numbers — mismatches adjust the table, never patch
  values.
- **Wire-extension regressions in E7 paths**: mitigated by amending
  `wire-protocol.md` in the same PR and running E7's integration suite
  untouched (the plan's gate task).
- **Condition seed growth changes tier verdicts mid-flight**: frozen at
  apply (D7) — effects keep their resolved math; new applies follow the
  new seed; documented in the picker contract.
- **Engine bug at the table**: stop-the-line (Constitution Article IV);
  the named-example suite + property suite are the tripwire, ordered
  first in the plan.
