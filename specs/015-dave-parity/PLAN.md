# 015 — Dave parity: every prototype panel, Foundry data

**Owner:** Vex (direct build — Josh, 2026-10-15: "You. We need to get this out so Dave will stop
working on his html alone and hardcoding everything.")
**Rulings (Josh, 2026-10-15):** (1) ALL of Dave's panels ship. Supersedes E6 spec §3/§9 parking —
of those cuts only Notes (Q3) was ever put to Josh (paperclip 348fd82b, 2026-10-01); the rest were
agent scope calls. (2) ALL game data from the Foundry pf2e pack — no hardcoded tables in the product.
**Baseline:** `docs/reference/lorum_ipsum_dashboard.html` (Dave's prototype, frozen cb0f397 + #33).
**Verification law:** every slice is screenshotted in real Chrome against the prototype with Dave's
own export (Lorum Ipsum) loaded. No slice is "done" on a green test alone — 349 green tests shipped
an app that crashed in every browser (PR #297).

## Data map — every prototype table → its Foundry source

| Prototype hardcode | Foundry pack | Notes |
|---|---|---|
| `CONDITIONS`, `COND_GROUPS` | conditions.json (imported) | groups = derive from pack/journal GM Screen |
| `ACTIONS`, `ACTION_SUM` | actions.json (574) | basic / skill / exploration / encounter via traits+category |
| `FEATURE_TEXT` | class-features.json (874), feats.json (6284) | detail prose |
| feat details, `SK_FEATS` | feats.json | |
| spell details | spells.json (1994) | |
| `CURRICULA` | class-features.json — school docs carry the curriculum as @UUID spell links in description | parse links, no hardcode |
| `UNDEAD_SUMMONS` | pathfinder-monster-core*.json, pathfinder-bestiary*.json (npc docs; publication at system.details.publication) | |
| `WEAPONS`/`ARMOR`/`ITEMS`/`ITEM_DB_BASE`/`TRAIT_DB`/`CRIT_SPEC` | equipment.json (imported) + lang/en.json trait descriptions | |
| `PET_ABILITIES` | class-features / feats (familiar abilities) | |
| `SKILL_INFO` | actions.json skill actions + journals GM Screen | |
| Rules tab (Lorum's curated notes) | journals.json → GM Screen pages + per-character notes | character-specific notes become the Notes pane |

## Slices (ship order — each one deployed when green in a browser)

1. **Layout** — sheet full-width (App.svelte `<main>` 72rem bug from f4dbfe9); names not truncated. ✅ in progress
2. **Importer v2** — add spells, feats, actions, class-features, npcs (core bestiaries), journals.
   Bump IMPORTER_VERSION. Same contract discipline as v1 (validate shapes, fail closed).
3. **Reference column** — Actions / Conditions / Rules / Encounter / Skill Actions / Exploration tabs,
   strikes group, from corpus.
4. **Detail popups** — spell, feat, feature, action, item, trait text from corpus.
5. **Notes pane** — rich text, named pages, #tags, synced per character (new table + sync field).
6. **Curriculum slots** — from the school class-feature's links.
7. **Pet & Minions** — summons browser (bestiary corpus), current minions + HP + dismiss (synced).
8. **Inventory depth** — tabs (Equipped/Magic/Gems/Trade), container editing, restore picker, bulk bar.
9. **Reset Layout / drag-resize board** — persisted per account.
10. **Header parity** — Import Pathbuilder button in sheet header, sync status.
