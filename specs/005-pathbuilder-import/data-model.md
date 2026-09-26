# Data Model: Pathbuilder Import (E5)

Companion to [`design.md`](design.md); implements [`spec.md`](spec.md).
Field-level truth for the documents and rows E5 writes. Storage is E2's
landed schema — this epic adds no tables and no columns (the two raised
migrations are data/CHECK changes, design §5). Source shapes cited from
[`contracts/pb-export.md`](contracts/pb-export.md).

---

## 1. `characters` (E2 table — E5's writes)

| column | E5 behavior |
|---|---|
| `payload_raw` | the request body **byte-verbatim as received** (text). First import and every re-import replace it. Never re-serialized. |
| `base_sheet` | E5's normalized output (§2), replaced wholesale on every import. |
| `owner_sub` | the session account's `sub`, set once at first import; re-import is keyed on it (SELECT by owner). Never changed by import. |
| `party_id` | the seeded POC party (design §5.1) on first import; never changed. |
| `updated_at` | touched by every import; app-managed per E2 convention. |

## 2. `base_sheet` jsonb — normalized shape (owned by E5)

Camel-vs-snake: sections mirror the export's own key names where the value
passes through unchanged (imported data, not our vocabulary), and use
snake_case where E5 derives a value. Sections:

```jsonc
{
  "schema": "hireling.base_sheet.v1",     // E5's own version for THIS shape
                                          // (the export has none — contracts §0)
  "identity": {                            // contract §3.1, verbatim fields
    "name": "Lorum Ipsum", "class": "Wizard", "dual_class": null,
    "level": 3, "xp": 18, "ancestry": "Gnome", "heritage": "Wellspring Gnome",
    "background": "Charlatan", "alignment": "N", "deity": "Not set",
    "age": "44", "gender": "Male", "size": 1, "size_name": "Small",
    "keyability": "int", "languages": ["Aklo", "…"]
  },
  "abilities": {                           // six scores + breakdown passthrough
    "str": 8, "dex": 12, "con": 14, "int": 18, "wis": 10, "cha": 16,
    "breakdown": { "…": "…" }
  },
  "hp": {                                  // derived inputs, formula contract §3.3
    "ancestryhp": 8, "classhp": 6, "bonushp": 0, "bonushp_per_level": 0,
    "max_hp": 14                           // = ancestry+class+bonus+perLevel×(level−1)
  },
  "ac": { "acTotal": 18, "acAbilityBonus": 4, "acProfBonus": 4,
          "acItemBonus": 0, "shieldBonus": 0 },        // inputs only; E8 recomputes
  "proficiencies": { "fortitude": 2, "…": 0 },         // flat map verbatim
  "specific_proficiencies": { "trained": [], "expert": [], "master": [], "legendary": [] },
  "lores": [ { "name": "Underworld", "rank": 2 } ],    // normalized from pairs
  "spellcasters": [                                    // one per export block, in order
    {
      "caster_key": "Wizard",                          // FR-10 tie-break applied
      "name": "Wizard", "magic_tradition": "arcane",
      "spellcasting_type": "prepared", "ability": "int",
      "proficiency": 2, "innate": false, "focus_points": 0,
      "per_day": [6, 4, 3, 0,0,0,0,0,0,0, 0],
      "known":  [ { "rank": 0, "spells": ["Daze", "…"] } ],
      "prepared": [ { "rank": 1, "spells": ["500 Toads", "…"] } ]
    }
  ],
  "equipment": [                          // contract §3.7 normalized
    { "name": "Backpack", "qty": 1, "container": null, "invested": true },
    { "name": "Bedroll",  "qty": 1, "container": "Backpack", "invested": true }
  ],
  "containers": [
    { "name": "Backpack",         "extradimensional": false, "backpack": true,  "augmentations": false },
    { "name": "Giant body's sack","extradimensional": true,  "backpack": false, "augmentations": false }
  ],
  "weapons": [ { "…": "verbatim object per contract §3.9" } ],
  "armor":   [ { "…": "verbatim object" } ],
  "money": { "cp": 4, "sp": 2, "gp": 24, "pp": 0 },     // as imported; live money is vitals
  "focus":   { "…": "verbatim nested tradition→ability map (§3.10)" },
  "focus_points": 1,
  "companions": [ { "type": "Familiar", "name": "Familiar (Pippin)",
                    "abilities": ["Fast Movement (Land)"], "equipment": [] } ],
  "raw": {                                 // unmodeled-but-known sections, verbatim
    "feats": [["Charming Liar", null, "Awarded Feat", 1]], // + specials, rituals,
    "resistances": [], "rituals": [], "formula": [],       // mods, inventorMods,
    "mods": {}, "inventorMods": []                         // pets
  }
}
```

Rules:

- `raw` holds sections E6 has no typed consumer for yet, keyed by their
  export names, values verbatim. When an epic needs one typed, it graduates
  out of `raw` (a `base_sheet` shape change, noted in `schema`).
- Unknown fields never enter `base_sheet` — class (c) logs them and the
  count rides the response advisory (FR-6).
- A known section whose type drifted is skipped whole into `raw: {}` absent
  + a diff entry (contract §5) — degraded section, never lost character.
- `slot_layout()` (derived, not stored): for each `spellcasters[i]`, for each
  rank `r` where `per_day[r] > 0`: `(caster_key, r, slot_index 0..per_day[r]−1)`.

## 3. Live-state bindings (E2 tables — E5's seed/preserve behavior)

### `character_vitals` — anchor: character identity
First import only: `hp = base_sheet.hp.max_hp`, `temp_hp = 0`,
`money_* = export.money`, `level_adjust = 0`. Re-import: never read, never
written (FR-10). Max-HP changes across re-imports surface in the diff as
`MaxHpChanged{old, new}` information (clamping is E6/E8 logic).

### `character_spell_slots` — anchor: `(caster_key, rank, slot_index)`
- First import: materialize every layout position; `used = false`;
  `prepared_spell` from export prepared lists (FR-12), NULL when unnamed.
- Re-import: matched rows untouched; unmatched rows kept + diffed; new
  positions seeded from export + diffed (design §4). `version` columns move
  only when E5 writes a row — seeded rows take their `nextval` default;
  preserved rows keep their versions (E7's catch-up ordering is untouched by
  import).

### `character_inventory_live` — anchor: exact `item_name`
E5 never writes this table (absent row = zero delta). Re-import matching is
name-existence in the new `equipment` list; unmatched live rows kept +
diffed; negative effective quantities (`new base qty + delta < 0`) diffed,
not corrected (FR-10/11).

### `audit_events` — one row per import attempt (raised CHECK, design §5.2)
`event = 'character_import'`, `actor_sub` = session, `outcome = allowed |
denied`, `target` = `character:{id}` on success else `import:{failure-code}`,
`request_id` propagated. GM rejections additionally produce the middleware's
existing `forbidden_gm_write` row (E3 behavior, unchanged).

## 4. Error envelope (HTTP failures)

E3's standard error payload shape (single source — `src/auth/error.rs`),
carrying: status, `code` (§4 of the contract), `message` (the exact string),
`request_id`. Class (c) is not an error — it rides success's advisory.

## 5. Post-import diff (success response; review surface, not a mutation)

```jsonc
{
  "first_import": false,
  "kept_unmatched": [
    { "kind": "slot",  "caster_key": "Wizard", "rank": 2, "slot_index": 2,
      "used": true, "prepared": "Illusory Creature" },   // vanished slot, kept
    { "kind": "item", "name": "Silver Chunk", "qty_delta": -1 }  // vanished item, kept
  ],
  "seeded": [
    { "kind": "slot", "caster_key": "Wizard", "rank": 3, "slot_index": 0,
      "prepared": null }                                  // new layout position
  ],
  "prep_divergence": [
    { "caster_key": "Wizard", "rank": 1, "slot_index": 0,
      "live": "Fear", "export": "Befuddle" }              // live wins (FR-10)
  ],
  "notices": [
    { "kind": "negative_quantity", "name": "Chalk", "base_qty": 2, "delta": -3 },
    { "kind": "max_hp_changed", "old": 14, "new": 22 },
    { "kind": "section_skipped", "section": "equipment" } // type drift, contract §5
  ]
}
```

Empty arrays when nothing diverged; `first_import: true` responses return
all-empty diff arrays (FR-13 idempotency case: second identical import ⇒
every array empty).
