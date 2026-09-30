# Contract: Pathbuilder 2e Export (E5 import input)

**Status**: Captured — cited from the frozen fixture, not authored.
**Fixture**: `docs/reference/lorum_ipsum_dashboard.html`, the
`<script type="application/json" id="pbExport">` block — a single
5,494-byte JSON document (measured 5,493 bytes of content at E5 spec time,
frozen with the prototype per E2's `data-model.md` §1). Field-level truth for
what this document cites lives there too; this contract adds what E5's parser
must accept, reject, and skip.

The export schema is **unofficial and unversioned** — no version field exists
anywhere in the envelope (verified across all 39 `build` keys). Drift
robustness is failure class (c) (unknown fields: log and continue), never a
version pin.

---

## 1. Envelope

```json
{ "success": true, "build": { … 39 keys at the fixture … } }
```

- Top level: exactly `success` (boolean) + `build` (object) in the fixture.
- `success: false`, or `build` missing/null/not-an-object ⇒ failure class (b).
- Any additional top-level key ⇒ class (c).

## 2. Required shape (failure class (b) detector — the whole rule)

An export is accepted iff **all** of:

| path | requirement |
|---|---|
| `success` | boolean, truthy |
| `build` | object |
| `build.name` | string, non-empty |
| `build.level` | integer, 1–20 |
| `build.abilities` | object with numeric `str dex con int wis cha` |
| `build.proficiencies` | object (any keys — the fixture's flat map) |

Nothing else is required. Notably **not** required: `spellCasters` (martial
characters), `money`, `equipment`, `feats`, `xp`, `acTotal` — a minimal
export carrying only the required shape imports with empty optional sections.
Tolerance for absent optional sections is class (c) behavior; absence of a
*required* path is class (b).

## 3. Section shapes (as captured at the fixture)

### 3.1 Identity — `name`, `class`, `dualClass` (`null` or string), `level`,
`xp` (integer), `ancestry`, `heritage`, `background`, `alignment`, `deity`,
`age` (string "44" — captured as string, imported as-is), `gender`, `size`
(int), `sizeName`, `keyability`. All strings unless noted. Unknown future
identity keys: class (c).

### 3.2 Abilities — `abilities`: `{str,int,wis,dex,con,cha}` numeric scores
plus `abilities.breakdown` (object of boost/flaw lists and
`mapLevelledBoosts`) — breakdown is passthrough data, not interpreted.

### 3.3 HP inputs — `attributes`: `{ancestryhp, classhp, bonushp,
bonushpPerLevel, speed, speedBonus}` numeric. **Max HP formula (first-import
HP seed, FR-9):** `ancestryhp + classhp + bonushp + bonushpPerLevel ×
(level − 1)`. Fixture check: 8 + 6 + 0 + 0×2 = 14 = the prototype's max HP
for Lorum Ipsum at level 3.

### 3.4 AC — `acTotal`: `{acTotal, acAbilityBonus, acProfBonus, acItemBonus,
shieldBonus}` numeric. Stored as the breakdown; E6/E8 recompute live AC from
base + effects, so only the inputs are base-sheet data.

### 3.5 Proficiencies — `proficiencies`: flat string→int map (`classDC`,
`perception`, `fortitude`, `reflex`, `will`, armor categories `heavy/medium/
light/unarmored`, weapon categories `advanced/martial/simple/unarmed`,
casting traditions `castingArcane/castingDivine/castingOccult/castingPrimal`,
every skill lowercase). Plus `specificProficiencies`
`{trained:[],expert:[],master:[],legendary:[]}` and `lores`: positional
`[name, rank]` pairs (`[["Underworld",2],["Mror Holds History",4]]`).

### 3.6 Spellcasting — `spellCasters`: **array of caster blocks** (fixture:
two). Each block: `{name, magicTradition, spellcastingType, ability,
proficiency, innate (bool), focusPoints (int), perDay (int[11] — index 0 =
cantrips, 1–10 = ranks), spells: [{spellLevel, list:[names]}], prepared:
[{spellLevel, list:[names]}], blendedSpells: []}`. Spell identity is plain
strings — no ids anywhere. The fixture's second block (`Wellspring Gnome`,
`innate: true`, `perDay: [1,0,…]`, `prepared: []`) is the innate regression
case: one cantrip slot, seeded unprepared.

- `caster_key` (anchor base): block `name` if unique in the array; else
  `name#2`, `name#3`, … by array position (FR-10 tie-break).
- Slot layout for anchors: for every rank `r` with `perDay[r] > 0`, slots
  `slot_index` 0..`perDay[r]−1`. Prepared seeding (FR-12):
  `prepared_spell` = the `prepared` entry at `spellLevel == r`, position
  `slot_index`, else NULL.

### 3.7 Equipment — `equipment`: **variable-length positional arrays**.
Captured forms:

```
["Backpack", 1, "Invested"]                                  — no container
["Bedroll", 1, "e6b4b905-…", "Invested"]                      — container UUID
```

Elements: `[name, qty, containerUUID?, "Invested"?]`. Container UUID keys
`equipmentContainers`: `{uuid: {containerName, bagOfHolding (bool — the
extradimensional flag), backpack, augmentations}}`. Fixture containers:
"Backpack" (`bagOfHolding: false`) and "Giant body's sack" (`true`).
Normalization resolves UUID → container name; an unresolved UUID = no
container + class (c) notice. Item identity is **name, case-sensitive**.

### 3.8 Money — `money`: `{cp, sp, gp, pp}` non-negative integers. First
import seeds live money (FR-9); re-import never touches it (FR-10).

### 3.9 Weapons/armor — `weapons`: object array (fixture keys: `name, qty,
prof, die, pot, str, mat, display, runes[], damageType, attack, damageBonus,
extraDamage[], increasedDice, isInventor, grade`); `armor`: object array
(`name, qty, prof, pot, res, mat, display, worn, runes[], grade`).

### 3.10 Everything else (passthrough + class (c) tolerance) — `feats`
(positional `[name, sub, type, level, …]`), `specials`, `languages`,
`focus` (nested tradition→ability→`{abilityBonus, proficiency, itemBonus,
focusCantrips[], focusSpells[]}`), `focusPoints`, `familiars`
(`[{type, name, equipment[], specific, abilities[]}]`), `pets`, `resistances`,
`rituals`, `formula`, `mods`, `inventorMods`. These import as normalized
lightweight or verbatim-passthrough sections per `data-model.md`; unknown
keys inside them are class (c).

## 4. Failure classes — exact behavior

| class | detector | HTTP | code | message (verbatim) |
|---|---|---|---|---|
| size | body > 1 MiB | 413 | `payload-too-large` | "That's too large to be a character export (limit 1 MB). Make sure you exported a single character." |
| depth | nesting > 64 | 400 | `payload-too-deep` | "That JSON is nested too deeply to be a character export." |
| (a) invalid JSON | parse error | 400 | `invalid-json` | "That isn't valid JSON. Copy the whole export from Pathbuilder (Share → Export JSON) and paste it again." |
| (b) not Pathbuilder | §2 rule violated | 400 | `not-pathbuilder` | "That's valid JSON, but it doesn't look like a Pathbuilder export — the character sheet fields (name, level, abilities, proficiencies) are missing." |
| (c) unknown fields | shape walk | — | — | non-failure: import succeeds; fields logged, count returned |

Order: size → depth → parse (a) → shape (b). The walk for (c) runs on
accepted documents only.

## 5. Drift policy (the export carries no version)

- New optional keys anywhere ⇒ class (c): log (account, request id, key
  path), skip, continue.
- A known section changing **type** (e.g. `equipment` becoming objects)
  ⇒ treated as unknown for that section: the section is skipped whole,
  logged, and the import continues with that section empty — the diff
  reports the skipped section. This is the "manual re-export" worst case:
  degraded sections, never a lost character.
- Slot-quirk class features (Staff Nexus, school spells, flexible
  spellcasting) are expected to surface here: each real export that
  exercises a quirk lands as a committed importer test case (epic prompt);
  no speculative handling is built.

## 6. Known fixture facts (regression anchors)

- 39 `build` keys (E2's data-model cites 40; recounted 39 at E5 spec time —
  non-load-bearing discrepancy, recorded not reconciled).
- Envelope bytes 5,493 / nesting depth 8 (caps sit at 1 MiB / 64 — §4).
- Homebrew present and honored: "500 Toads" prepared at rank 1.
- Two caster blocks, overlapping ranks (rank 0 in both) — the reason
  `caster_key` exists (E2 FR-9 extension).
- 16 equipment entries, 2 containers, 1 extradimensional container.
