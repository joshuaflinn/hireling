# Spec — E6: Live Sheet UI (desktop)

**Epic:** E6 · gh#8 · Lane A · Depends on E5 (merged, PR #28 + #32) · rides E7's
client stack (merged, PR #34) · converges with E8 (in flight)
**Status:** clarify gate answered 4/4 by Josh, 2026-10-01 (§8) · design +
plan at the board gate
**Spec source:** `docs/EPICS.md` → E6 (specify prompt, settled constraints, AI
guardrails); `docs/PRD.md` v3.6 FG1.

## 1. Visual baseline — the post-#33 prototype

The visual and interaction baseline is `docs/reference/lorum_ipsum_dashboard.html`
**as rewritten by PR #33**, read at `main` commit **`73839aa`** (merge of
`update-reference-prototype`, 2026-10-01). Josh's ruling: spec against the new
prototype, not the pre-#33 one. Any E6 artifact citing the pre-#33 prototype is
wrong. Where the new prototype and the PRD disagree, the PRD wins and the
deviation is named in this spec (§9).

The prototype is a standalone HTML file: its save/template/cloud machinery is
test scaffolding for a backend-less file, not product behavior. Its *rendered
surfaces, layout, CSS design language, and interaction patterns* are the
baseline. Its persistence model (localStorage, Firestore stub, save slots) is
explicitly not.

## 2. What this epic builds

The desktop live character sheet: a Svelte (Vite) single-page app, static
bundle served by the axum binary (E1 shell), rendering the player's full
character from E5's `base_sheet` and E7's synced live state, with every write
going through E7's optimistic sync stack. The sheet renders; it does not
compute effect math — every derived number arrives through the engine-output
contract (§5), authored on the E8 side at
`specs/008-buff-effect-engine/contracts/engine-output.md` (referenced here,
never duplicated; both specs are written by the same PM so the seam is one
contract, not two).

### Sheet surface (enumerated)

1. **Header** — character name, ancestry/class/heritage/background line
   (rendered from `base_sheet.identity`), manual level-adjust control (§4),
   `syncing…` indicator (E7 contract), New Day action (FR-5).
2. **Stats pane** — HP bar (0–max clamped, temp-HP as a distinct bar segment
   that absorbs damage first, ±1/±5/Full buttons, temp input), stat tiles (AC,
   Fort/Ref/Will, Perception, Speed, Class DC, Spell DC, Size), focus pips
   (tracked — Q1 ruling), hero-point pips (tracked — Q1 ruling), attributes
   with key-ability marker, skills and lores
   with proficiency-rank letter and modifier, ancestry/background/alignment/
   languages/deity meta lines.
3. **Magic pane** — tabs: Spells · Pet & Minions. Spells: per-caster header
   (tradition, type, ability, spell attack/DC), prepared slots by rank with
   cast tracking, cantrips (heightened rank display), innate spells, focus
   cantrips, spellbook/"not prepared" collapsible, prep reset to export,
   curriculum-slot display, Staff Nexus panel (charge-rank pick and charge
   spend tracked — Q1 ruling), Drain Bonded Item (used flag tracked — Q1).
4. **Pet & Minions tab** — pet/familiar panel derived from
   `base_sheet.companions` (HP/AC/saves/speed lines, abilities, command note).
   Summons browsing and minion HP tracking are parked (Q4 ruling, §8).
5. **Inventory pane** — items grouped by container (from `base_sheet`), Bulk
   rollup per container with the **extradimensional exclusion**
   (`containers.extradimensional` — contents never count toward Bulk),
   quantity editing, coin counts (editable, syncs as `vitals.money`), tab
   filter chips rendered from corpus traits. Interactive container editing,
   splits, equipped sync: parked (Q2 ruling, §8).
6. **Feats & Features pane** — feats list (names, level-grouped) and features
   tab (class features, ancestry & heritage) from `base_sheet`. Names and
   structure only at P0: detail popups need a feats corpus (E4 deferred —
   "importer-capable but deferred (no consumer)"); parked.
7. **Strikes surface** — strike rows from `base_sheet.weapons` + unarmed
   (Fist): name, attack modifier, MAP (−5/−10 agile-adjusted), damage dice,
   traits. Lives in its own sheet section (the prototype renders strikes under
   Reference→Actions; E6 pulls them into the sheet proper — the strikes are
   *the character's*, the actions encyclopedia is not, §3).
8. **Effects strip** — chips for active effects affecting this character with
   sources, rendered from engine-output state (E8). Before E8 ships: the strip
   renders empty with the affordance hidden — no placeholder theatre.

## 3. Prototype surface → owner map (nothing unassigned)

Post-#33 prototype surfaces, each mapped to its owner. "Parked" = beyond the
P0/P1 line as written, no owning epic yet — disposition recorded, never
silently dropped. Rulings below are Josh's, 2026-10-01, via the clarify card.

| Prototype surface | Owner |
|---|---|
| Header name/sub-line, stats pane, skills, attributes, meta lines | **E6** |
| HP bar, temp-HP segment, buttons, temp input | **E6** |
| Stat tiles (AC/saves/Perception/Speed/DCs/Size) — rendering | **E6** (values via E8 contract) |
| Skill popups (rank/ability breakdown) | **E6** (provenance contract shape) |
| Level adjust (removed by #33 via `delete S.level`; PRD FG1 still requires it) | **E6** — PRD wins, deviation §9 |
| Focus pips, Hero Points pips, Staff Nexus charges, Drain Bonded Item | **E6** — tracked via new sync fields (Q1 ruling: extend) |
| `syncStatus` span | **E6** (renders E7 `isSyncing()` + degraded states) |
| Save button / Templates menu & dialog (named whole-sheet save slots) | **Out** — prototype-local save-slot scaffolding; superseded by E7's server-authoritative persistence. Not E13 (E13 = Dave-authored spell outcome templates, P1). |
| "Reset Layout" (drag-resize widget layout, persisted positions) | **Parked** — E6 ships the fixed three-column layout the PRD names; layout customization has no P0/P1 owner |
| New Day (clear cast slots, refill focus, reset drain) | **E6** — full reset incl. focus/drain/staff (Q1 ruling) |
| Import Pathbuilder dialog | **E5** (shipped); E6 links its entry point |
| Spell slots, prep (drag-to-prepare), cast tracking, cantrips, innate, spellbook, reset-to-export, curriculum slots | **E6** |
| Spell detail popups (full spell text, heightened notes) | **Parked** — needs spells corpus (E4 deferred) + E9 prose lane |
| Pet/familiar panel | **E6** |
| Summons browser (hardcoded `UNDEAD_SUMMONS` bestiary) | **Parked** — bestiary corpus deferred at E4; hardcoded fixture data never enters the product |
| Current minions + HP + dismiss | **Parked** (Q4 ruling: companions-only) — returns when a bestiary/minions epic owns it |
| Inventory: container display, Bulk math, extradimensional exclusion, qty edit | **E6** |
| Coins (pp/gp/sp/cp) + reset | **E6** (`vitals.money`) |
| Inventory tabs: Equipped / Magic / Gems / Trade filters | Tag *chips* render from corpus traits (**E6**); Equipped tab, tag *editing* and the interactive container system **parked** (Q2 ruling) |
| Add item dialog (corpus search + custom-item form) | **E9** (custom entry at point of use is E9's named P0 lane); corpus-search add rides E9's lane |
| Container editing: drag between containers, create/delete container, mark-as-container, capacity warnings ("Container is full"), split stack, soft-delete/restore picker | **Parked** (Q2 ruling: display + qty at P0) |
| Feats/Features lists | **E6** |
| Feat/feature/spell detail prose | **Parked** — E9 prose + deferred corpora |
| Reference→Actions: Strikes group | **E6** (moved into the sheet, §2.7) |
| Reference→Actions: basic-actions encyclopedia (Step, Create a Diversion, …) | **Parked** — E9's POC fence is Player Core conditions; other game terms on demand |
| Reference→Conditions tab (42-condition map, sort by category/A–Z) | **E9** |
| Reference→Rules tab (Lorum's curated rules notes) | **Parked** — character-specific curated content; may seed E9/E15 someday, not a P0 surface |
| Notes pane (rich text, named pages, tags, autocomplete) | **Parked** (Q3 ruling: out of E6) — named future surface, no owner yet |
| Tooltip infrastructure (`#tip`/`#ctip`, `setHTML` DOMParser discipline) | **E6** ships the shared inert-HTML primitive; **E9** consumes it for prose |
| Trait chips (names) / trait-text popups (full Paizo text) | names **E6**; text popups **E9** (license + prose lane) |
| Phone/tablet reflow (780px single column) | **P2** (gh#18) — out of scope |
| Cloud sync stub (Firestore `sheet/state`, writeCloud) | **Out** — prototype test scaffolding; E7 is the product sync |
| Minion/summon stat cards, mcard popups | **Parked** with the bestiary (above) |

## 4. Level adjust (PRD FG1 — math rescale only)

Header control beside the level display: bounds **1–20**, **confirm on
level-down**, and every level-derived stat **visibly re-derives on tap**.
Persisted as `vitals.level_adjust` (int −19..19 relative to the export's
level — the field E2 versioned and E7's wire protocol already carries; not a
new field).

- **Re-derives:** proficiency bonus, max HP (ancestry + class + per-level
  bonuses × level), **AC (the worn armor's proficiency rank rides the level —
  the export's frozen `acProfBonus` is an input at the export level only;
  amended by E6's review, MOR-48 finding 8)**, class DC / spell DC / spell
  attack scaling, cantrip/focus heightened rank, skill and save modifiers
  whose proficiency ranks are level-scaled.
- **Does not re-derive (comes only from a Pathbuilder re-export):** ability
  boosts, feats, skill increases, skill ranks, spell repertoire, per-day slot
  counts, strikes' weapon math.
- The prototype deleted this control in #33 (`delete S.level`, line 1138 of
  the baseline file). The PRD (FG1 and "Advanced Features & Edge Cases")
  still mandates it, and the sync field exists. **PRD wins** — deviation
  recorded, §9.

## 5. Where the numbers come from (the E6/E8 seam)

Every derived number on the sheet — AC, saves, Perception, skill modifiers,
strike attack/damage, spell attack/DC, Class DC, speed — is consumed through
**one internal module boundary**, `web/src/lib/engine/` (adapter), whose
interface is the engine-output contract at
`specs/008-buff-effect-engine/contracts/engine-output.md` (derived values
keyed by the PRD FG3 stat vocabulary, provenance breakdowns *including
suppressed sources*, effect-chip state). E6 renders that output; it never
computes effect math, and no second computation path may exist (E10 and E14
reuse these same components against the same state).

- **After E8 ships:** the adapter forwards engine output. Provenance hover
  shows the full breakdown including suppressed sources, per the contract.
- **Before E8 ships (E6 merges first):** the adapter runs in **base-only
  mode** — derived values computed from `base_sheet` inputs (the export's own
  breakdowns: `acTotal` parts, ability scores + proficiency ranks, weapon
  math), with zero effects and the effects strip hidden. This is bounded,
  named technical debt: the base math lives behind the adapter interface and
  is deleted when the engine adapter lands, not parallel to it. (The export
  contract itself says "stored as the breakdown; E6/E8 recompute live AC" —
  some summation must exist somewhere; E6's interim owns it behind the seam.)
- Level adjust (§4) applies inside the adapter in both modes.

## 6. Optimistic writes (consume E7's stack — never invent a parallel one)

The sheet's write path is E7's shipped client stack
(`web/src/lib/sync/`: `createSync`, store, queue, connection) against the
contracts `specs/007-party-sync/contracts/wire-protocol.md` (CAS writes,
per-field versions, ack outcomes) and
`specs/007-party-sync/contracts/degraded-mode.md` (one path, queue rules,
indicator). E6 obligations on top:

- **Write surface at P0:** `vitals.hp`, `vitals.temp_hp`, `vitals.money`,
  `vitals.level_adjust`, `slot.{used,prepared}`, `inv.qty_delta` (exact-name
  keys, E5's matching semantics). The Q1 ruling adds `vitals.focus_current`,
  `vitals.hero_points`, and `vitals.daily` — new fields join wire-protocol
  §3's `vitals` field enum via the contract's sanctioned extension path (PR to
  `wire-protocol.md` + migration; E6's implementation owns that PR —
  design.md §3).
- **Local echo** renders immediately, tagged pending; control disabled until
  ack per degraded-mode §4's local-echo rule.
- **`superseded`** — silent revert to server truth. No error, ever.
- **`rejected`/`forbidden`** — revert the echo and surface a calm inline
  message at the control (degraded-mode §4 makes this E6's choice; this spec
  chooses inline, non-modal, auto-clearing on next successful ack). These are
  user-input/authz faults, not sync events.
- **`syncing…` indicator** — visible iff the queue is non-empty
  (`isSyncing()`), subtle chrome affordance in the prototype's design
  language, nothing faster than 1 Hz, no colour semantics.
- **Offline read-only:** when the store reports `offline`, edit affordances
  disable but the sheet stays fully readable (last-known state, never a
  spinner over nothing). No modal, toast, or banner — the contracts forbid
  error theatre.
- **Client-side validation before writes:** HP clamped to [0, max], temp ≥ 0,
  level bounds 1–20, qty ≥ 0 — invalid input never leaves the component (the
  server re-validates anyway; the client gate is for the user, not the
  server).

## 7. States, access, and non-functional requirements

- **Empty state is a real screen.** First run with no imported character:
  the sheet route renders a deliberate empty state with the "Import your
  character" CTA (linking E5's import flow) — designed, not a fallback. Same
  discipline for every async surface: loading (skeleton, not blank), error
  (calm, named, retry affordance where a retry exists).
- **Read-only mode:** every sheet component accepts an `editable` prop;
  owner-editable when true, view-only render when false. E10's roster drill-in
  reuses these components read-only for non-owners and the GM — this is the
  named reuse contract for E10 (EPICS conflict risk: "Reuse E6's sheet
  components for card drill-ins — do not re-implement").
- **Reusable units named for E10:** `CharacterHeader`, `HpBar` (with temp
  segment), `StatTile`, `SkillList`/`SkillRow`, `StrikeRow`, `SpellSlotRow`,
  `FocusPips`, `InventoryPanel`, `EffectChips`, `SyncIndicator`, `Tooltip`
  (inert-HTML), `EmptyState`, `SyncBadge` (pending-tag render). E10's roster
  cards compose `HpBar` + `EffectChips` + `SyncBadge`; E14's stat blocks
  compose `StatTile` rows from the same engine state.
- **Keyboard accessibility:** every interactive element reachable and
  operable by keyboard; drag-to-prepare has a keyboard equivalent
  (slot focus → spell picker dialog); dialogs trap focus and restore it on
  close; `Escape` closes, Enter commits — the prototype's pattern.
- **Inert HTML rendering:** all prose/HTML injection goes through a port of
  the prototype's `setHTML` discipline (DOMParser parse, node adoption, no
  script execution). One shared util; no component innerHTML-assigns.
- **Stack discipline:** Svelte via Vite, no SvelteKit, static bundle served
  by the axum binary. Hand-rolled CSS following the prototype's design
  language (dark fantasy, Palatino headers, gold accents, three-column
  desktop grid). No component library.
- **Desktop only:** build target is the desktop layout. No responsive work
  beyond what the three-column grid needs at desktop widths; phone/tablet is
  P2 (gh#18).
- **Verification:** `just ci-local` green before any PR opens (includes the
  web test suite — vitest, the E7 pattern); grizzly-gate fail-closed on the
  PR; merge ask goes to Thrane. Claim gh#8 before any code (AGENTS.md sync point 1).

## 8. Clarify rulings (answered — Josh, 2026-10-01, card `348fd82b` on MOR-45)

Four surfaces where the PRD was silent or the sync field set was absent
while the post-#33 prototype ships them. Asked, not guessed; answered 4/4,
all recommendations accepted. These rulings are binding for design and plan:

- **Q1 — Spell-economy trackers → EXTEND.** Track focus points, hero points,
  Staff Nexus charges and Drain Bonded Item as synced live state. New vitals
  fields (one migration + wire-protocol §3 rows, E6 owns the contract PR):
  `focus_current` (int ≥ 0, clamped client-side to the character's focus
  max), `hero_points` (int ≥ 0), `daily` (whole-row JSON
  `{staff_charge_rank: 0..10, staff_spent: int ≥ 0, drain_used: bool}`),
  each with its own `*_version` column, exactly the E2 pattern. New Day
  resets: all slots `used=false`, `focus_current` **refilled to the
  character's focus max** (amended by E6's review, MOR-48 finding 2 — the
  ruling's original "`focus_current=0`" text read as points *spent*, but the
  field and the UI both mean points *available*; PF2e daily preparations
  regain the whole pool, and the header's own tooltip promised the refill),
  `daily` reset to `{0,0,false}`.
- **Q2 — Inventory editing depth → DISPLAY + QTY.** Containers render with
  Bulk math (extradimensional exclusion) and quantities sync. The interactive
  container system (drag-between, create/delete, mark-as-container, capacity
  warnings, split stacks, equipped sync, tag editing) is parked — no field-set
  extension in E6. "Remove item" at P0 = quantity write to 0 (the restore
  picker was prototype-local-state UX; with server truth, qty 0 is the
  record).
- **Q3 — Notes pane → OUT.** Parked as a named future surface with no owner.
  Not dropped silently — it is in the scope table and returns when an epic
  owns it.
- **Q4 — Minions → COMPANIONS ONLY.** The pet/familiar panel is E6
  (§2.4). Summons browsing (hardcoded bestiary) and minion HP tracking are
  parked until a bestiary corpus + field-set epic owns them.

## 9. Named deviations from the post-#33 prototype

1. **Level adjust retained** though #33 deleted it — PRD FG1 is law (§4).
2. **Save/Templates/Cloud machinery dropped** — prototype test scaffolding;
   E7's server-authoritative persistence replaces it (§3).
3. **Strikes move into the sheet** (from Reference→Actions) — strikes are the
   character's data; the actions encyclopedia is parked reference prose.
4. **Fixed three-column layout** — the PRD names the three-column desktop
   design; widget drag-resize customization is parked.
5. **Spell/feat detail popups absent at P0** — deferred corpora (E4 scope
   fence), named in §3 rather than absorbed.
6. **Curriculum-slot display not built (E6 review, MOR-48)** — spec §2.3
   names it, but curriculum *content* exists nowhere in `base_sheet` or the
   E4 corpus: only as hardcoded tables in the post-#33 prototype, the same
   banned class as fixture data (§3's own discipline: hardcoded game data
   never enters the product). Disposition: cut, not silently dropped;
   returns when an epic owns carrying curriculum data in the corpus.

## 10. Non-goals (binding)

No character builder, no combat/round/initiative tracking, no GM tooling, no
dice roller, no auto-expiry or aura automation (PRD non-goals). No mobile
layout (P2, gh#18). No tooltip prose or custom-entry forms (E9). No shared
stash/bank (E12). No spell outcome templates (E13, Dave-authored, P1). No
effect math (E8 owns computation; E6 renders). Nothing in this spec moves the
P0/P1/P2 line.
