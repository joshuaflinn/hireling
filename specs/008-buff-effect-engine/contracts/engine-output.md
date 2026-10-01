# Contract: Engine Output (E8) — the sheet's single source of numbers

**Status**: **DRAFT at the specify gate** — shape pinned for E6's parallel
spec to bind to; binding content is ratified at the E8 design gate (same
precedent as E7's contracts). If code and this document disagree, this
document wins until a PR changes it.
**Consumers**: E6 (live sheet — derived values, provenance hover, effect
chips), E10 (party cards — same chips, same state), E13 (P1 conflict
pre-warning — reads live engine state). **No consumer computes; everyone
renders.**

## 0. Position

The engine is a pure function:

```
engine_output = Engine(base_stats, active_effects, condition_mappings)
```

It never sees HTTP, WebSocket, SQL, or the DOM (spec FR-1). This contract
pins the **payload shapes** — `BaseStats` in, `EngineOutput` out. How
`EngineOutput` travels to clients (wire framing vs client-side engine) is
clarify Q3 / design-gate territory; the payload shape holds either way.

## 1. Input: `BaseStats` (per character, plain data)

One resolved base value per stat *instance*. A character has multiple
instances of the per-strike and per-caster stats (Q1 axis):

```jsonc
{
  "schema": "hireling.engine.base.v1",
  "level": 5,                          // base_sheet.identity.level + live level_adjust, clamped 1..20
  "stats": {
    "ac": 18, "fort": 9, "ref": 7, "will": 10,
    "perception": 6, "speed": 25, "class_dc": null,
    "strikes": [                        // one entry per strike the sheet renders
      { "key": "dagger", "label": "Dagger", "attack": 11, "damage": "1d4+3" }
    ],
    "casters": [                        // one entry per base_sheet.spellcasters block
      { "caster_key": "Wizard", "spell_attack": 9, "spell_dc": 22 }
    ],
    "skills": [                         // every skill in the skill set (Q2) — core skills always present
      { "name": "acrobatics", "total": 2 }, { "name": "lore:underworld", "total": 7 }
    ]
  }
}
```

- Damage bases are roll expressions (strings); modifiers adjust the flat
  part. `null` base (no class DC on this character) means the stat is not
  rendered and modifiers to it are suppressed from display but still
  computed (no invention of a base).
- The `base_sheet` → `BaseStats` extraction mapping table is a design-gate
  deliverable recorded here (which export fields feed which base).

## 2. Input: `ActiveEffect[]` (per party)

The E7 snapshot/diff shape for effects, verbatim — E8 adds nothing:

```jsonc
{ "effect_id": 41, "name": "Bless", "source_character_id": 3,
  "targets": [7, 9], "modifiers": [ { "type": "status", "stat": "attack", "value": 1 } ],
  "duration_note": "10 rounds", "active": true, "version": 1042 }
```

Plus, for corpus-applied conditions, the chip metadata of §4 (tier,
tracked_manually) and the resolved signed modifiers (constant or
`condition_value × polarity` resolved at apply time — E4 seed semantics).

## 3. Output: `EngineOutput` (per character) — **the binding shape**

```jsonc
{
  "schema": "hireling.engine.output.v1",
  "character_id": 7,
  "derived": {
    "ac":        { "base": 18, "total": 19,
                   "applied":    [ { "type": "status", "value": 1, "effect_id": 41,
                                     "effect_name": "Bless", "source_character_id": 3 } ],
                   "suppressed": [ { "type": "status", "value": 1, "effect_id": 48,
                                     "effect_name": "Inspire Courage", "source_character_id": 5,
                                     "reason": "same-type-lower-bonus", "suppressed_by_effect_id": 41 } ] },
    "fort":      { "…": "…" }, "ref": { }, "will": { }, "perception": { },
    "speed":     { "base": 25, "total": 25, "applied": [], "suppressed": [] },
    "class_dc":  { "base": null, "total": null, "applied": [], "suppressed": [] },
    "strikes":   [ { "key": "dagger",
                     "attack":     { "base": 11, "total": 12, "applied": [/*…*/], "suppressed": [/*…*/] },
                     "damage_flat": { "base": 3, "total": 4, "applied": [/*…*/], "suppressed": [/*…*/] } } ],
    "casters":   [ { "caster_key": "Wizard",
                     "spell_attack": { "base": 9,  "total": 10, "applied": [/*…*/], "suppressed": [/*…*/] },
                     "spell_dc":     { "base": 22, "total": 23, "applied": [/*…*/], "suppressed": [/*…*/] } } ],
    "skills":    [ { "name": "acrobatics", "total": 3, "applied": [/*…*/], "suppressed": [/*…*/] } ]
  },
  "effects": [   // chips: every active effect targeting this character
    { "effect_id": 41, "name": "Bless", "source_name": "Lorum Ipsum",
      "duration_note": "10 rounds", "active": true, "version": 1042,
      "modifiers": [ { "type": "status", "stat": "attack", "value": 1 } ],
      "tracked_manually": false }   // true ⇒ display-only condition: badge, zero math
  ]
}
```

Rules that make this a contract:

- **`applied`** entries: `{type, value, effect_id, effect_name,
  source_character_id}` — every modifier that contributed.
- **`suppressed`** entries carry `reason` (one of:
  `same-type-lower-bonus`, `same-type-lighter-penalty`, `same-type-tie`)
  and `suppressed_by_effect_id` (for ties, the deterministically-chosen
  stable winner by effect id). Suppressed entries are engine output, not
  UI inference — E6's hover renders them verbatim
  (`+1 status (Bless) — Inspire Courage +1 also active, not stacked`).
- **Ordering is deterministic** (by effect id, then modifier order) —
  consumers may memo on the payload.
- **`damage` modifiers** land in each strike's `damage_flat`; `attack`
  modifiers in each strike's `attack`; `spell_attack`/`spell_dc` in each
  caster entry. Blanket targets expand server-side of this contract —
  consumers never see `all_*` stats, only their expansion.
- **`null` base stats** keep `null` totals (nothing invented); their
  applied/suppressed lists still account for every modifier (SC-4).
- **Versioning**: `schema` strings version these shapes; a breaking change
  is a PR to this file plus a version bump, never a silent drift.

## 4. Chip metadata (corpus conditions)

Chips for corpus-applied conditions add `tracked_manually: true|false`
(from `data.import.tier`) so E6/E10 badge display-only conditions without
reading corpus tables. The picker's data (tier, valued, name) is read from
the corpus rows per spec FR-8 — this contract only carries what the chip
renders.
