# Data Model: Database Schema (E2)

Companion to [`design.md`](design.md); implements [`spec.md`](spec.md).
Field-level truth for every table E2's migrations create. Types are Postgres
16. All tables live in the `hireling` database, default schema.

**Conventions applying to every table unless noted:**

- `id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY` — **except
  `accounts`, which is natural-keyed on `sub`** (see §2; the E3 keying
  contract). Other natural/composite PKs noted per table where used.
- `created_at timestamptz NOT NULL DEFAULT now()` on every entity table.
- `updated_at timestamptz NOT NULL DEFAULT now()` on every entity table
  except `claim_history` (append-only: nothing ever updates). App-managed —
  every UPDATE sets `updated_at = now()`.
- Every `*_version` column is `bigint NOT NULL DEFAULT
  nextval('field_version_seq')`, server-assigned at write time, strictly
  increasing (see design §4).
- FK indexes: every FK column below is indexed (FR-5); composite indexes
  noted where the access path needs more than the bare FK.

---

## 1. Import-contract fixture: the `#pbExport` block

**Fixture:** `docs/reference/lorum_ipsum_dashboard.html`, the
`<script type="application/json" id="pbExport">` block (single 5,494-byte
JSON document, frozen with the prototype at merge `cb0f397`). This is the
contract shape of imported character data (PRD FG1; spec Constraints).
Structure cited from the extracted block, not from memory:

- **Envelope:** `{"success": true, "build": {…}}` — `build` carries 40 keys.
- **Identity:** `name`, `class`, `dualClass` (null), `level`, `xp`,
  `ancestry`, `heritage`, `background`, `alignment`, `deity`, `age`,
  `gender`, `size`/`sizeName`, `keyability`.
- **Stats:** `abilities` (six scores + `breakdown`); `attributes`
  (`ancestryhp`, `classhp`, `bonushp`, `bonushpPerLevel`, `speed`,
  `speedBonus`); `acTotal` (breakdown + total); `proficiencies` (flat map:
  saves, Perception, every skill, armor/weapon/casting classes, `classDC`);
  `specificProficiencies`; `lores` (positional `[name, rank]` pairs).
- **Spellcasters — an array of TWO blocks:** `Wizard` (arcane, prepared,
  `innate: false`) and `Wellspring Gnome` (occult, prepared, **`innate:
  true`**, `prepared: []`). Each block: `name`, `magicTradition`,
  `spellcastingType`, `ability`, `proficiency`, `innate`, `focusPoints`,
  **`perDay` — an array of exactly 11 integers** (index 0 = cantrip,
  1–10 = spell ranks), `spells` (known) and `prepared` as per-level lists of
  spell **names — plain strings, no IDs**. Hence: prep and slot anchors are
  positional (rank + index), spell identity is by name, and `caster_key` must
  disambiguate blocks.
- **Equipment:** `equipment` entries are **variable-length positional
  arrays** — `["Backpack", 1, "Invested"]`, `["Bedroll", 1,
  "e6b4b905-236b-479e-92c4-36b9854f14b4", "Invested"]` — where the optional
  third/fourth element is a **container UUID** keying
  `equipmentContainers`, whose entries carry `containerName`,
  **`bagOfHolding: true|false`** (the extradimensional flag; the fixture's
  "Giant body's sack" is `true`), `backpack`, `augmentations`. Item identity
  is by **name**; container membership is base-sheet data.
- **Money:** `{"cp": 4, "sp": 2, "gp": 24, "pp": 0}`.
- **Everything else:** `weapons`/`armor` (object arrays), `feats`
  (positional arrays: `[name, sub, type, level, …]`), `specials`,
  `languages`, `focus` (per-tradition pools with spell name lists),
  `focusPoints`, `familiars`/`pets`, `resistances`, `rituals`, `formula`,
  `mods`, `inventorMods`.
- **Homebrew is in the fixture:** "500 Toads" appears prepared at level 1 —
  the custom-lane proof of need.
- **No version field anywhere.** Drift robustness is E5's failure-class (c)
  — unknown fields log and continue. The schema's answer to this shape is
  `payload_raw` text + `base_sheet` jsonb, not relational modeling: this
  contract relationally modeled is a migration magnet.

---

## 2. Identity & scope

### `accounts` — PK `sub`

One person known to the app. Keyed by the **E3 cross-epic contract (human
amendment, settled)**: the provider's `sub` claim is the primary key and
every ownership binding in the schema FKs to it — no surrogate id, no
separate `idp_subject` column. Rows are **upserted at login by E3**; E2
seeds none. Authentication and sessions remain E3.

| column | type | constraints | notes |
|---|---|---|---|
| `sub` | text | **PRIMARY KEY** | the external IdP's stable `sub` claim (a stable user UUID) — the identity anchor everything else binds to |
| `username` | text | NOT NULL | login username, from the OIDC profile |
| `display_name` | text | NOT NULL | display only; refreshed by E3's login upsert |
| `role` | text | NOT NULL, CHECK IN (`'player'`,`'gm'`) | data for E3's authorization enforcement. The GM's read-only nature stays **server-enforced (E3), not schema-enforced** — this column carries no schema write bans, per the spec assumption |

FK behavior on `sub`: ownership bindings (`characters.owner_sub`,
`claim_history.actor_sub`, `corpus_entries.created_by_sub`) use ON DELETE
RESTRICT — accounts are never deleted in normal operation, and a silent
cascade through ownership chains must not exist. `sub` is a stable provider
UUID; it does not change, so no ON UPDATE clause is needed.

### `parties`

One campaign; the scoping root for nearly everything (FR-4).

| column | type | constraints | notes |
|---|---|---|---|
| `name` | text | NOT NULL | |
| `quartermaster_character_id` | bigint | NULL, FK → `characters(id)` ON DELETE SET NULL | the quartermaster designation as data — present now, activated by E12, no schema change then (spec edge case) |

### `characters`

A roster member of exactly one party, owned by exactly one account. Holds
two of the three storage concerns (raw payload, base sheet); live state is
separate tables (FR-8).

| column | type | constraints | notes |
|---|---|---|---|
| `party_id` | bigint | NOT NULL, FK → `parties(id)` | indexed; scoping root |
| `owner_sub` | text | NOT NULL, UNIQUE, FK → `accounts(sub)` ON DELETE RESTRICT | the ownership binding (E3 keying contract); one character per account; no accountless character — both rejections spec-mandated (FR-7) |
| `payload_raw` | text | NOT NULL | the Pathbuilder export **byte-verbatim as received** (text, not jsonb — jsonb reorders keys and re-serializes; US3.1 says verbatim). Latest import only; no history (spec assumption) |
| `base_sheet` | jsonb | NOT NULL | E5's normalized output: identity, stats, proficiencies, feats/specials/lores/languages, slot layouts (`perDay` shape), item lists with container membership + extradimensional flags, weapons/armor, money-as-imported, spellcasters incl. known/prepared lists, focus, AC breakdown, companions. Replaced **wholesale** on re-import. Shape owned by E5's parser; the schema holds it opaque |

Relationships: `parties 1—N characters`; `accounts 1—0..1 characters`
(bound by `owner_sub` → `accounts.sub`).
The `parties.quartermaster_character_id` ↔ `characters.party_id` pair is a
deliberate nullable cycle (insert party → characters → set quartermaster).

State transitions: **imported** (row + vitals created by E5) → **live**
(normal operation) → **re-imported** (`payload_raw` + `base_sheet` replaced;
live tables reconciled by anchor; not a status column — a write pattern) →
**removed** (hard delete, rare and administrative, FR-15). Deleting a
character cascades its vitals/slots/inventory-live rows and its
`effect_targets` links; effects it *created* are RESTRICT-protected (§3).

---

## 3. Live state (anchor-keyed; survives re-import)

No FK from any live table into base-sheet contents — that absence is the
SC-5 mechanism: wholesale base replacement cannot cascade-delete live state.
Orphaned anchors (renamed item, vanished slot, renamed caster block) are
kept and surfaced by E5's post-import diff, never dropped.

### `character_vitals` — PK `character_id` (FK → `characters(id)` ON DELETE CASCADE, 1:1)

| column | type | constraints | notes |
|---|---|---|---|
| `hp` | int | NOT NULL DEFAULT 0, CHECK ≥ 0 | current HP; max lives in `base_sheet`; clamp [0, max] is app logic |
| `hp_version` | bigint | versioned field | |
| `temp_hp` | int | NOT NULL DEFAULT 0, CHECK ≥ 0 | absorbs damage first (PF2e order; app logic) |
| `temp_hp_version` | bigint | versioned field | |
| `money_gp` / `money_sp` / `money_cp` / `money_pp` | int | NOT NULL DEFAULT 0, CHECK ≥ 0 | the export's `money` shape; live state (spec assumption: spending syncs like HP) |
| `money_version` | bigint | versioned field | one version for the four denominations — money is not in FR-14's granularity list; unit-versioned |
| `level_adjust` | int | NOT NULL DEFAULT 0, CHECK BETWEEN -19 AND 19 | **delta** over the base-sheet level, not an absolute — a re-import at a new Pathbuilder level then preserves the mid-session adjustment (spec assumption). Effective level = base + delta, clamped 1–20 app-side (PRD FG1 math-rescale-only) |
| `level_adjust_version` | bigint | versioned field | uniformity; not in FR-14's list |

### `character_spell_slots` — PK (`character_id`, `caster_key`, `rank`, `slot_index`)

One row per **individual** slot (FR-14 granularity: "each spell slot
individually"; FR-9 anchor: slot rank + index — extended by caster because
the fixture's two caster blocks share ranks).

| column | type | constraints | notes |
|---|---|---|---|
| `character_id` | bigint | FK → `characters(id)` ON DELETE CASCADE | |
| `caster_key` | text | NOT NULL | the spellCasters block's `name` from the export (`"Wizard"`, `"Wellspring Gnome"`). Block matching across re-imports is E5's; a renamed block orphans rows under the keep-and-surface rule |
| `rank` | int | NOT NULL, CHECK BETWEEN 0 AND 10 | matches the fixture's 11-element `perDay` (0 = cantrip) |
| `slot_index` | int | NOT NULL, CHECK ≥ 0 | position within the rank |
| `used` | boolean | NOT NULL DEFAULT false | expenditure |
| `prepared_spell` | text | NULL | preparation rides the same anchor (spec: prep selections by slot rank + index). NULL = unprepared/none |
| `version` | bigint | versioned field | per-slot row |

The base layout (how many slots exist per rank — `perDay`) lives in
`base_sheet`; these rows hold only live usage/prep. A re-import shrinking a
rank orphans the excess rows → kept, surfaced.

### `character_inventory_live` — PK (`character_id`, `item_name`)

| column | type | constraints | notes |
|---|---|---|---|
| `character_id` | bigint | FK → `characters(id)` ON DELETE CASCADE | |
| `item_name` | text | NOT NULL | the FR-9 anchor — items have no IDs in the export, only names. Matching semantics (case etc.) are E5's |
| `qty_delta` | int | NOT NULL DEFAULT 0 | live quantity adjustment against the base-sheet quantity (consumed chalk, added loot). Negative allowed — that's the point. Delta-vs-override arithmetic across re-import is E5/E6's; the schema stores the signed delta |
| `version` | bigint | versioned field | per-item quantity granularity (FR-14) |

Container membership and the extradimensional flag stay in `base_sheet`
(the fixture's container-UUID → `equipmentContainers.bagOfHolding`
structure) — layout is base data, replaced on re-import; only quantity is
settled-granularity live state. Bulk queries excluding extradimensional
contents read base-sheet data (E6/E8).

---

## 4. Effects

### `effects`

| column | type | constraints | notes |
|---|---|---|---|
| `party_id` | bigint | NOT NULL, FK → `parties(id)` | FR-4 scoping (denormalized from source character so party queries never join through) |
| `source_character_id` | bigint | NOT NULL, FK → `characters(id)` ON DELETE RESTRICT | the creator/owner; RESTRICT forces a human decision before a creator's removal destroys effects (spec edge case: the effect is not silently destroyed) |
| `name` | text | NOT NULL | |
| `duration_note` | text | NOT NULL DEFAULT '' | free text ("10 rounds", "while in aura") — displayed, never enforced (PRD: no countdown automation) |
| `active` | boolean | NOT NULL DEFAULT true | ended = `false`, kept queryable — state, not deletion (FR-15) |
| `version` | bigint | versioned field | **whole-effect** granularity (FR-14): any target/modifier/active change bumps it in the same transaction |

State transitions: **active** → **ended** (`active = false`, creator action;
the only designed transition). Reactivation (`false` → `true`) is not a
product flow at P0 but the schema does not forbid it — it is a version-bumped
update, and E8/E9 may use it if a spec ever asks. Hard delete exists only as
an administrative escape hatch (FR-15).

### `effect_targets` — PK (`effect_id`, `character_id`)

| column | type | constraints | notes |
|---|---|---|---|
| `effect_id` | bigint | FK → `effects(id)` ON DELETE CASCADE | |
| `character_id` | bigint | FK → `characters(id)` ON DELETE CASCADE | the **link** may die with a removed roster character; the effect row may not (spec edge case). Roster characters only — companions/minions are not targetable (PRD FG3) |

### `effect_modifiers` — PK (`effect_id`, `ord`)

| column | type | constraints | notes |
|---|---|---|---|
| `effect_id` | bigint | FK → `effects(id)` ON DELETE CASCADE | |
| `ord` | int | NOT NULL, CHECK ≥ 0 | ordered modifier list (FR-10) |
| `type` | text | NOT NULL, CHECK IN (`circumstance`,`status`,`item`,`untyped`) | the stacking types; penalties are negative values, not a type |
| `stat` | text | NOT NULL, CHECK (length > 0) | the closed engine vocabulary — single stats (`ac`, `fort`, …), blanket targets (`all_checks`, `all_dcs`, `all_checks_and_dcs`), and `skill:<name>`. The vocabulary is closed but `skill:<name>` is open-ended per skill, so the schema stores text and E8 owns enforcement — a partial CHECK would lie |
| `value` | int | NOT NULL | signed; negative = penalty |

---

## 5. Corpus — E2 owns the DDL; E4 writes ROWS ONLY

### `corpus_entries`

One open-kinded table: conditions and items now; spells/feats/bestiary land
later as **data, not schema change** (spec assumption).

| column | type | constraints | notes |
|---|---|---|---|
| `kind` | text | NOT NULL, CHECK (length > 0) | `condition`, `item`, … — deliberately not an enum; open-ended |
| `name` | text | NOT NULL | display/lookup name |
| `lane` | text | NOT NULL, CHECK IN (`core`,`imported`,`custom`) | the lane split; a fourth value is unstorable (US5.1) |
| `data` | jsonb | NOT NULL | the structured row (item stats, condition detail, …). Shape per kind is the writer's contract (E4 for imported, E9 for custom/core) |
| `modifiers` | jsonb | NULL | structured condition→modifier mappings in the FG3 shape (`[{type, stat, value}]`). **`NULL` = display-only row** — the tier test needs no prose parsing (US5.4); the importer never fabricates mappings (E4 guardrail) |
| `source_id` | text | NULL | upstream pack entry id (Foundry pack slug); the idempotency key |
| `pack_version` | text | NULL | upstream pack release — required by convention for `imported` rows (US5.2) |
| `imported_at` | timestamptz | NULL | when the producing import ran |
| `created_by_sub` | text | NULL, FK → `accounts(sub)` ON DELETE RESTRICT | creator for `custom` rows (US5.3); custom rows are first-class — nothing about their storage is second-class |

**Indexes/constraints:**

- Partial UNIQUE on `(kind, source_id)` WHERE `source_id IS NOT NULL` —
  E4's re-run upsert key ("same pack, same row" reconciles idempotently).
  `custom` rows carry no `source_id` and are structurally never in any pack's
  upsert scope — re-runs cannot overwrite them (spec edge case, by
  construction).
- Index on `(kind, lane)` for picker queries; index on `name` for
  case-insensitive exact-name lookups (E12 book-value resolution).

Lane-vs-provenance coupling (imported ⇒ pack fields present, custom ⇒
creator present) is writer discipline owned by E4/E9, not a CHECK — the
schema guarantees the columns exist and the lane is constrained, and stays
out of cross-column business rules it would only half-express.

---

## 6. P1 shared inventory (present, unused at P0 — FR-12)

Fully consistent with this epic's party-scoping, timestamps, hard-delete,
and versioning rules; no P0 code path writes them; E12 activates them with
zero migrations for the core structures (its spec owns adding stash fields
to the versioning scheme).

### `party_stash`

| column | type | constraints | notes |
|---|---|---|---|
| `party_id` | bigint | NOT NULL, FK → `parties(id)` | |
| `item_name` | text | NOT NULL | |
| `quantity` | int | NOT NULL, CHECK > 0 | a stash row with zero quantity is deleted, not kept (E12's write rule; schema backs it) |
| `bulk` | numeric(6,1) | NULL | per-item Bulk; PF2e "L" = 0.1 by convention, NULL = negligible/unknown |
| `notes` | text | NOT NULL DEFAULT '' | |

### `claim_history` — append-only by construction

| column | type | constraints | notes |
|---|---|---|---|
| `party_id` | bigint | NOT NULL, FK → `parties(id)` | |
| `item_name` | text | NOT NULL | |
| `quantity` | int | NOT NULL, CHECK > 0 | |
| `origin` | text | NOT NULL | where it moved from — a label (`"character:Lorum Ipsum"`, `"stash"`); format is E12's spec, the schema is deliberately dumb |
| `destination` | text | NOT NULL | where it moved to (`"stash"`, `"party bank"`, `"character:…"`) — sales append with `to: party bank` (PRD FG4) |
| `actor_sub` | text | NOT NULL, FK → `accounts(sub)` ON DELETE RESTRICT | who did it — settles "who took what" (US6.3) |
| `created_at` | timestamptz | NOT NULL DEFAULT now() | the only timestamp; **no `updated_at`** |

**Append-only enforcement:** a `BEFORE UPDATE OR DELETE` trigger raises an
exception — "the schema provides no such path" (US6.2, FR-15). Trigger, not
REVOKE, because the app connects as the table-owning role (app-boot
migrations), and REVOKE is toothless against the owner. The trigger is
role-independent, self-documenting, and drops cleanly in the down migration.

### `party_bank` — PK `party_id` (FK → `parties(id)`, one row per party)

| column | type | constraints | notes |
|---|---|---|---|
| `gp` / `sp` / `cp` | int | NOT NULL DEFAULT 0, CHECK ≥ 0 | the FG4 ledger denominations |
| `version` | bigint | versioned field | single balance row — the movements are already in `claim_history`; a second ledger is complexity, not audit |

---

## 7. Machinery

- **`field_version_seq`** — one global sequence feeding every `*_version`
  column (design §4: per-field monotonicity plus a total order for E7
  catch-up).
- **Append-only trigger** on `claim_history` (§6).
- No other triggers, no LISTEN/NOTIFY, no extensions beyond the Postgres 16
  baseline (`citext` was considered for item-name matching and rejected —
  matching semantics are E5's; adding an extension to pre-answer another
  epic's question is speculative).

---

## 8. Relationship summary

```
accounts(sub) 1──0..1 characters N──1 parties   (owner_sub → accounts.sub)
parties  0..1─── characters            (quartermaster_character_id, nullable)
characters 1──1 character_vitals
characters 1──N character_spell_slots  (per caster_key × rank × slot_index)
characters 1──N character_inventory_live (per item_name)
characters 1──N effects (source)       N──M characters (via effect_targets)
effects    1──N effect_modifiers
parties    1──N effects / party_stash / claim_history
parties    1──1 party_bank
accounts   1──N claim_history (actor_sub) / corpus_entries (created_by_sub)
corpus_entries: standalone (no FK into character/party state)
```

Delete behavior (hard delete policy, FR-15): character delete cascades its
vitals, slots, inventory-live rows, and `effect_targets` links — but a
character that *created* effects cannot be deleted while they exist:
`effects.source_character_id` is RESTRICT (creator removal is a human
decision — end or reassign the effects first). Effect delete cascades its
targets and modifiers. Party delete cascades everything party-scoped except
`accounts` and `corpus_entries`, which are not party-scoped. Ended effects
are never deleted by the product — `active = false` keeps provenance
queryable for historical breakdowns (spec edge case).
