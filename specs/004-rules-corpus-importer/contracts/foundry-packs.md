# Contract: Foundry VTT pf2e packs (upstream source for E4)

**Status**: Captured fixtures · **Probed**: 2026-09-20, read-only, public GitHub only
**Repo**: `github.com/foundryvtt/pf2e`

> Fixtures below are VERBATIM captures from the live upstream. Anything not
> yet captured is marked `assumed-until-probed`. Test fixtures and importer
> expectations MUST be built from these captures, never from memory of the
> pack format.

## 1. Release pinning

**Captured** — `GET https://api.github.com/repos/foundryvtt/pf2e/releases/latest`
returned tag `sf2e-1.5.1` (Starfinder!). The repo ships **two systems from one
repo** with tag prefixes `pf2e-*` and `sf2e-*`. Consequences:

- FR-14's pin must be an explicit `pf2e-*` tag; the GitHub "latest" endpoint
  is not a pf2e selector and must never be used.
- The importer rejects tags not matching `^pf2e-\d+\.\d+\.\d+$`.

**Captured** — the latest pf2e-tagged release at probe time:

- Tag: **`pf2e-8.5.1`**, name `8.5.1 (pf2e)`, published `2026-09-16T01:54:04Z`,
  target commitish `v14-dev` (also the repo's default branch).
- Release assets (verbatim from the API response):

  | asset | size (bytes) | digest (verbatim) |
  |---|---|---|
  | `json-assets.zip` | 36,400,547 | `sha256:194fbfe586a3a05c7f866f2043f1dc73dc7bffe7f6f94b6bcb37242a0253b75c` |
  | `system.json` | 51,384 | `sha256:7f949b6abc19de9ec8deac367420c0f10cba35beee6df1c62876edafe140b7a6` |
  | `system.zip` | 112,797,392 | `sha256:543f55aeab713a5304161c8ead0134632bfa01421ca81e553eb9341b83e838da` |

  Download URL pattern:
  `https://github.com/foundryvtt/pf2e/releases/download/<tag>/<asset>`.
  The `digest` field in the API response is the integrity check the importer
  verifies after download (design.md stage 1b).

- `assumed-until-probed`: the **internal layout of `json-assets.zip`** (that
  it contains the pack JSON documents under a `packs/pf2e/…` tree mirroring
  the repo). Fallback if false: GitHub git-tree API + per-file raw fetches.
- `assumed-until-probed`: GitHub API/rate-limit behavior for the fallback
  path (unauthenticated raw fetches of ~thousands of pack files).

## 2. Pack layout in the repo tree

**Captured** — `GET /repos/foundryvtt/pf2e/contents/packs?ref=pf2e-8.5.1`:
two directories, `packs/pf2e` and `packs/sf2e`.

**Captured** — `packs/pf2e/` contains (among ~100 pack dirs, mostly
`*-bestiary`): `conditions`, `equipment`, `equipment-effects`, `spells`,
`spell-effects`, `actions`, `classes`, `ancestries`, `vehicles`, etc.
E4 reads **only** `conditions` and `equipment` (FR-1).

- `assumed-until-probed`: the full file list of `packs/pf2e/conditions/`
  (count and filenames; Player Core condition coverage is verified by the
  seed-mapping PR, not by this contract).
- `assumed-until-probed`: whether `packs/pf2e/equipment/` is fully flat or
  has nested subdirectories beyond the one probed file (design handles
  `**/*.json` recursion defensively).

## 3. Representative condition document (CAPTURED VERBATIM)

`https://raw.githubusercontent.com/foundryvtt/pf2e/pf2e-8.5.1/packs/pf2e/conditions/frightened.json`:

```json
{
    "_id": "TBSHQspnbcqxsmjL",
    "img": "systems/pf2e/icons/conditions/frightened.webp",
    "name": "Frightened",
    "system": {
        "active": false,
        "description": {
            "value": "<p>You're gripped by fear and struggle to control your nerves. The frightened condition always includes a value. You take a status penalty equal to this value to all your checks and DCs. Unless specified otherwise, at the end of each of your turns, the value of your frightened condition decreases by 1.</p>"
        },
        "duration": {
            "expiry": null,
            "perpetual": false,
            "text": "",
            "unit": "unlimited",
            "value": -1
        },
        "group": null,
        "overrides": [],
        "publication": {
            "license": "ORC",
            "remaster": true,
            "title": "Pathfinder Player Core"
        },
        "references": {
            "children": [],
            "immunityFrom": [],
            "overriddenBy": [],
            "overrides": []
        },
        "removable": false,
        "rules": [
            {
                "key": "FlatModifier",
                "selector": "all",
                "slug": "frightened",
                "type": "status",
                "value": "-@item.badge.value"
            }
        ],
        "traits": {
            "value": []
        },
        "value": {
            "immutable": false,
            "isValued": true,
            "value": 1
        }
    },
    "type": "condition"
}
```

**Required paths the importer validates (conditions)** — presence of:
`_id`, `name`, `type == "condition"`, `system.description.value`,
`system.publication.license`, `system.publication.remaster`,
`system.publication.title`, `system.value.isValued`.

**Notes:**

- No top-level `slug` field on this document; the slug appears inside
  `system.rules[].slug`. Identity is `_id`, per design.
- `system.rules` carries Foundry-engine rule elements. **The importer does
  not read them for math** (design.md, alternative C — rejected). They are
  preserved inside the stored raw `content` only.
- `system.value.isValued: true` marks valued conditions upstream; the
  authoritative valued/mapping decision is still the human tier seed
  (FR-11), but a mismatch between seed and upstream `isValued` is worth a
  run-report warning.

## 4. Representative item document (CAPTURED VERBATIM)

`https://raw.githubusercontent.com/foundryvtt/pf2e/pf2e-8.5.1/packs/pf2e/equipment/wayfinder.json`
(description HTML elided here for length — it was captured in full during
the probe; the verbatim document lands as a test fixture at plan stage;
structure verbatim):

```json
{
    "_id": "gbwr57aT9ou8yKWT",
    "img": "systems/pf2e/icons/equipment/worn-items/other-worn-items/wayfinder.webp",
    "name": "Wayfinder",
    "system": {
        "baseItem": null,
        "bulk": { "value": 0 },
        "containerId": null,
        "description": { "value": "<p><strong>Access</strong> member of the Pathfinder Society</p> …" },
        "hardness": 0,
        "hp": { "max": 0, "value": 0 },
        "level": { "value": 2 },
        "material": { "grade": null, "type": null },
        "price": { "value": { "gp": 28 } },
        "publication": {
            "license": "ORC",
            "remaster": true,
            "title": "Pathfinder GM Core"
        },
        "quantity": 1,
        "rules": [
            {
                "key": "AdjustModifier",
                "requiresEquipped": false,
                "selector": "survival",
                "slug": "no-compass",
                "suppress": true
            }
        ],
        "size": "med",
        "traits": { "rarity": "uncommon", "value": ["invested", "magical"] },
        "usage": { "value": "worn" }
    },
    "type": "equipment"
}
```

**Required paths the importer validates (items)** — presence of:
`_id`, `name`, `type == "equipment"`, `system.level.value`,
`system.price.value`, `system.publication.license`,
`system.publication.title`.

**Notes:**

- `system.price.value` is a coin object (`{"gp": 28}` — may carry
  `gp/sp/cp` keys). E12's book value reads from here via the stored
  `content` jsonb; the importer extracts no price column (E4 rows-only
  discipline: no column without a POC consumer's demand).
- `assumed-until-probed`: shape variance across equipment subtypes
  (weapons/armor carry `damage`, `acBonus`, etc.). Mitigation: validation
  requires only the paths above; everything else rides in `content`.
- `assumed-until-probed`: whether any equipment docs carry
  `publication.license` values other than `ORC` (e.g. `OGL` for
  pre-remaster items) inside a `pf2e-8.x` release. The license verdict
  (design.md) reads the DISTINCT set from imported rows, so a surprise
  value flips the verdict red and names it — by design, not by crash.

## 5. License notices (CAPTURED)

**Repo root `LICENSE`** at tag `pf2e-8.5.1` (captured verbatim, full text):
**Apache License 2.0**, copyright `2019 Hooking`. This is the *code* license,
not the content license — the notice file must not conflate them.

**`static/licenses/` at tag `pf2e-8.5.1`** (captured listing):

| file | size | sha (git blob) |
|---|---|---|
| `ORCLicense.md` | 11,030 | `748d2ec9dc519286a1ab7ae8301049526789aced` |
| `OpenGameLicense.md` | 64,082 | `e17650b82f60f76b1274e50d16b6e9f2651f8693` |
| `ambientcg.txt`, `iconics-assets.txt`, `rexard-game-dev-assets-eula.txt`, `shoony-icon.txt`, `svg-icons.txt` | — | asset-license files, not game content |

Raw URLs (the archive step fetches exactly these at the pinned tag):
`https://raw.githubusercontent.com/foundryvtt/pf2e/<tag>/static/licenses/ORCLicense.md`
`https://raw.githubusercontent.com/foundryvtt/pf2e/<tag>/static/licenses/OpenGameLicense.md`

**`ORCLicense.md` head (captured verbatim):**

> 1. ORC NOTICE
> This product is licensed under the ORC License located at the Library of
> Congress at TX 9-307-067 and available online at various locations. All
> warranties are disclaimed as set forth therein.
>
> 2. ATTRIBUTION NOTICE
> * Core Books
>   * Pathfinder Player Core © 2023, Paizo Inc.; Designers: Logan Bonner,
>     Jason Bulmahn, Stephen Radney-MacFarland, and Mark Seifter. …
>   * Pathfinder GM Core © 2023, Paizo Inc.; …
>
> 3. RESERVED MATERIAL
> Reserved Material: … All trademarks, registered trademarks, proper nouns
> (characters, deities, locations, etc. …), artworks, characters, dialogue,
> locations, organizations, plots, storylines, and trade dress.
>
> 4. EXPRESSLY DESIGNATED LICENSED MATERIAL
> Expressly Designated Licensed Material: This product contains no
> Expressly Designated Licensed Material.

**`README.md` licensing section (captured verbatim):**

> **Project Licensing:**
> - All HTML, CSS and Javascript in this project is licensed under the
>   Apache License v2.
>
> **Content Usage and Licensing:**
> - Any Pathfinder Second Edition information used with permission granted
>   by the license agreement between Paizo. Inc and Foundry Gaming LLC
> - Game system information and mechanics are licensed under the Open Game
>   License (OPEN GAME LICENSE Version 1.0a).
> - License information for the art used in this project is in
>   [./static/licenses/](./static/licenses/) alongside other project
>   licenses; release builds include them in `licenses/`.

**Consequences for the notice file (FR-16):**

- Pack *data* documents declare their own license per row
  (`system.publication.license` — `ORC` in both captured fixtures). The
  verdict's lane-coverage check reads these from the DB.
- The README asserts OGL 1.0a for game mechanics project-wide; both ORC and
  OGL texts are archived (FR-17) so a future legacy (pre-remaster, OGL)
  pack import is a data decision, not a legal one (spec assumption).
- The Foundry/Paizo *partnership* permission cited in the README is theirs,
  not ours — the notice file must rely on ORC/OGL/CUP, not on that
  partnership language.

**`assumed-until-probed`:** full verbatim text of `OpenGameLicense.md`
(64 KB — existence, size, and blob sha captured; the archive step pins its
sha256 in `licenses/foundry-pf2e/SOURCE.md` at archive time).

## 6. Identity & drift assumptions (explicit)

- `assumed-until-probed`: `_id` stability **across releases**. Foundry
  compendium IDs are treated as stable by the ecosystem (UUID references
  like `Compendium.pf2e.spells-srd.Item.Light` depend on it), but E4 has
  not diffed two releases. Mitigation: the first A→B re-import's stale
  report makes an ID-churn event visible immediately — a flood of
  stale+new pairs is the signature, caught before anything is deleted
  (nothing is ever auto-deleted).
- `assumed-until-probed`: per-release pack *schema* version marker. No
  explicit pack-schema field was found in the captured docs; the
  importer's "declared schema version" (FR-13) is therefore its own
  required-paths contract (sections 3–4), enforced per document, plus the
  pinned release tag. Drift shows up as validation failure → new importer
  version, exactly as the spec intends.

---

## 7. Probe at implementation (2026-09-24, importer build)

The `json-assets.zip` asset was downloaded and inspected at `pf2e-8.5.1`
(sha256 verified against the API digest — it matches §1). Several
`assumed-until-probed` marks above are now **resolved**, two of them
corrections:

- **Zip layout (was assumed, now captured):** the archive does NOT carry a
  `packs/pf2e/<pack>/*.json` tree. It carries **one JSON file per compendium
  pack, each file a JSON ARRAY of documents**: `packs/conditions.json` (43
  docs), `packs/equipment.json` (5869 docs), plus ~160 other entries (lang/,
  other packs, `_folders` metadata, sf2e packs) the importer ignores. The
  `_id` values match the per-file fixtures in §3–§4 exactly
  (`TBSHQspnbcqxsmjL` frightened, `gbwr57aT9ou8yKWT` wayfinder), so §3–§4
  remain the document-shape truth. The one-GET zip strategy stands.
- **Document-shape delta:** pack-array documents carry two extra top-level
  fields vs the repo-tree files (`_stats`, `effects`). The importer's
  required-path validation ignores them; the parsed document (including
  them) is stored as the row's upstream content, and the content hash is
  computed over the canonical (compact, key-sorted) re-serialization —
  stable across runs, independent of upstream whitespace.
- **Item `type` (corrects §4):** the equipment pack is NOT all
  `type == "equipment"`. Captured distribution at `pf2e-8.5.1`:
  equipment 2394, consumable 1703, weapon 1018, treasure 153, armor 211,
  ammo 216, shield 126, backpack 46, kit 2. Validation accepts exactly
  these nine item types; anything else is drift and fails the run.
- **Required paths (corrects §4):** all 5869 equipment docs carry
  `_id`, `name`, `img`, `type`, `system.description.value`,
  `system.publication.license/title/remaster` — but 3 do NOT carry
  `system.level.value` (the two kits) and one carries an empty
  `price.value` (a legacy OGL doc). `level`/`price` are therefore consumer
  concerns (E12), not import invariants, and are NOT required. 199 docs
  carry an EMPTY `system.description.value` (precious materials, variants) —
  presence is the invariant, emptiness is legal. All 43
  condition docs satisfy the §3 required paths unchanged.
- **Licenses present in the packs:** conditions are 43/43 ORC; equipment
  mixes ORC (4135) and OGL (1734) — pre-remaster content ships in remaster
  releases, and the NOTICE covers both lanes (the license verdict's
  distinct-value check depends on this).
