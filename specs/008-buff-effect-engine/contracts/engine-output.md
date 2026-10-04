# Contract: Engine Output (E8) — the sheet's single source of numbers

**Status**: **binding** (realized by the E8 implementation, PR gh#10, 2026-10-01; clarify answers folded same day, card `af988c85`: Q1 per-instance, Q2 core+lores, Q3 server-side). If code and this document disagree, this document wins until a PR changes it.
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
pins the **payload shapes** — `BaseStats` in, `EngineOutput` out. Q3 is
settled: the **server** computes; `EngineOutput` travels on E7's wire
(effect-commit broadcast + catch-up snapshot); clients render, never
compute.

## 1. Input: `BaseStats` (per character, plain data)

One resolved base value per stat *instance*. A character has multiple
instances of the per-strike and per-caster stats (Q1: **per instance**):

```jsonc
{
  "schema": "hireling.engine.base.v1",
  "level": 5,                          // base_sheet.identity.level + live level_adjust, clamped 1..20
  "stats": {
    "ac": 18, "fort": 9, "ref": 7, "will": 10,
    "perception": 6, "speed": 25, "class_dc": null,
    "strikes": [                        // one entry per strike the sheet renders —
                                        //   the export's weapons PLUS the appended unarmed
                                        //   Fist row (the sheet renders it; design D3)
      { "key": "dagger", "label": "Dagger", "attack": 11, "damage": "1d4+3" }
    ],
    "casters": [                        // one entry per base_sheet.spellcasters block
      { "caster_key": "Wizard", "spell_attack": 9, "spell_dc": 22 }
    ],
    "skills": [                         // every skill in the character's set (Q2: core skills + lores)
      { "name": "acrobatics", "total": 2, "rank": 0, "label": null }, { "name": "lore:underworld", "total": 7, "rank": 2, "label": "Underworld" }
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
                     "label": "Dagger", "map": 5,
                     "damage_expr": "d4+3", "damage_type": "P",
                     "damage_type_name": "piercing", "traits": ["Agile", "Finesse"],
                     "attack":     { "base": 11, "total": 12, "applied": [/*…*/], "suppressed": [/*…*/] },
                     "damage_flat": { "base": 3, "total": 4, "applied": [/*…*/], "suppressed": [/*…*/] } } ],
    "casters":   [ { "caster_key": "Wizard", "innate": false,
                     "spell_attack": { "base": 9,  "total": 10, "applied": [/*…*/], "suppressed": [/*…*/] },
                     "spell_dc":     { "base": 22, "total": 23, "applied": [/*…*/], "suppressed": [/*…*/] } } ],
    "skills":    [ { "name": "acrobatics", "rank": 2, "total": 3, "applied": [/*…*/], "suppressed": [/*…*/] },
                   { "name": "lore:underworld", "rank": 2, "label": "Underworld", "total": 7, "applied": [/*…*/], "suppressed": [/*…*/] } ]
  },
  "effects": [   // chips: every active effect targeting this character
    { "effect_id": 41, "name": "Bless", "source_name": "Lorum Ipsum",
      "duration_note": "10 rounds", "active": true, "version": 1042,
      "modifiers": [ { "type": "status", "stat": "attack", "value": 1 } ],
      "tracked_manually": false }   // true ⇒ display-only condition: badge, zero math
  ],
  "render_base": {   // render inputs with no modifier math — see the rules below
    "level": 5,          // eff_level, clamped 1..20
    "hp_max": 48,
    "focus_max": 2,
    "hero_max": 3,       // constant 3 at POC
    "cantrip_rank": 3,   // ⌈level / 2⌉
    "attributes": { "str": 0, "dex": 3, "con": 2, "int": 4, "wis": 1, "cha": 0 }
  }
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
- **`render_base`** carries the render inputs that have no modifier math:
  `level` (eff_level, clamped 1..20), `hp_max`, `focus_max`, `hero_max`
  (constant 3 at POC), `cantrip_rank` (⌈level/2⌉), and `attributes` (the six
  ability modifiers). They sit **outside `derived` deliberately**: `StatName`
  is a closed enum and no member — nor blanket target — addresses any of
  them, so no modifier can ever target a `render_base` value. There is no
  provenance to carry and no hover to back; a `StatOutput` slot for these
  would hold permanently empty `applied`/`suppressed` arrays — a shape that
  lies about itself. Computed server-side by the extractor (design D3); the
  design's BaseStats math table owns the formulas.
- **Strike display fields ride the strike object**: `label`, `map` (the MAP
  step: 4 for agile strikes, 5 otherwise), `damage_expr` (the roll string as
  rendered), `damage_type` (the export's letter code), `damage_type_name`
  (its display name), and `traits` (the chip names). They are passthrough
  render inputs alongside the modifier-bearing `attack`/`damage_flat`; there
  is deliberately **no** parallel `render_base.strikes[]` keyed by `key` —
  two arrays describing one strike row drift. The unarmed `Fist` row carries
  the same fields (traits from the POC weapon-trait map — the two-entry
  table the reference sheet renders; the post-POC successor is the corpus
  `item_traits` path the prototype's `base.js` sourced, tracked in
  joshuaflinn/hireling#62 so the narrowing cannot happen silently).
- **`casters[].innate`** flags innate caster blocks. A consumer picking
  "the" caster for its stat tiles takes the first entry with `innate: false`.
- **`skills[].rank`** is the row's raw proficiency rank (untrained 0,
  trained 2, expert 4, master 6, legendary 8 — after any class progression
  the extractor applies at adjusted levels, design D3). It is render input —
  the rank letter and untrained dimming (`StatsPane`), lores included — and
  **not derivable from `total`** (total = ability + rank + level +
  modifiers). Reading it back from `base_sheet.proficiencies` at render
  would fork one skill row across two sources — the same drift the strike
  display-field ruling rejects. Additive, so no schema bump; a consumer
  that ignores `rank` is unaffected.
- **`skills[].label`** (lore rows only) is the lore's display name,
  verbatim from the export (`"Underworld"`, `"Mror Holds History"`); core
  rows omit the field. The canonical `lore:` key lowercases and
  underscores the name to fit the `StatName` charset, so the display case
  cannot be recovered from the key — the label rides the row instead,
  exactly like the strike display fields (MOR-50's derive-the-label
  ruling presumed the key carried the display form; the realized charset
  closed that door). Consumers render it verbatim; additive, no bump.
- **Versioning**: `schema` strings version these shapes; a breaking change
  is a PR to this file plus a version bump, never a silent drift.
  `render_base`, the strike display fields, `casters[].innate`,
  `skills[].rank`, and `skills[].label` were added **without** a bump:
  purely additive — no
  existing field changed
  meaning or shape, and a consumer that ignores them is unaffected.

## 4. Chip metadata (corpus conditions)

Chips for corpus-applied conditions add `tracked_manually: true|false`
(from `data.import.tier`) so E6/E10 badge display-only conditions without
reading corpus tables. The picker's data (tier, valued, name) is read from
the corpus rows per spec FR-8 — this contract only carries what the chip
renders.
