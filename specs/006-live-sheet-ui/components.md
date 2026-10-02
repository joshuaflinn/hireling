# Components — the E10/E14 reuse contract (E6 Task 13)

**Consumers:** E10 (party roster + drill-in), E14 (GM stat blocks). The
component API below is part of E6's review criteria: E10 composes these
units; it must not re-implement them (EPICS conflict risk). Every
interactive component accepts `editable` (default `true`) and renders
**view-only when false — no disabled-control litter, simply no controls**
(design §6). The audit at the bottom pins where each one is tested.

All components are Svelte 5 runes-mode, callback props for events (no
custom event classes), and take plain data or svelte stores as props. The
sheet's numbers arrive exclusively through the engine seam
(`lib/engine/index.js` → the `view` prop); components never compute.

## Composed units E10 consumes

| Unit | Props | Events (callbacks) | E10 consumer |
|---|---|---|---|
| `CharacterHeader` | `name, subline, level, editable, offline, syncing` | `onadjustlevel(level), onnewday()` | roster card header line (view-only: name + level pill) |
| `HpBar` | `hp {value,pending}, temp {value,pending}, max, editable, offline` | `ondamage(n), onheal(n), onfull(), ontemp(n)` | **roster card HP bar** (view-only renders bar only) |
| `PipRow` | `label, current, max, editable, disabled` | `onset(count)` | hero-point display on cards |
| `StatTile` | `label, value, note, cls` | — | **E14 stat blocks** (tile rows from engine state) |
| `SkillRow` | `name, rankLetter, rank, modifier, untrained, wide` | — | drill-in skill list |
| `SyncIndicator` | `syncing` | — | card corner (queue-truth, ≤1 Hz) |
| `EffectsStrip` | *(activates with E8 — renders from `view.effects`)* | — | chips on cards; before E8 the strip is hidden, no placeholder theatre |

## Sheet sections (E10's drill-in renders these view-only)

| Section | Props | Events | Notes |
|---|---|---|---|
| `StatsPane` | `view, baseSheet, hp, temp, money, focusCurrent, focusMax, heroPoints, heroMax, editable, offline` | `ondamage, onheal, onfull, ontemp, onfocus, onhero` | composes HpBar + tiles + PipRows + skills + meta |
| `MagicPane` | `baseSheet, slots, view, daily, editable, offline` | `oncast(caster,row,used), onprepare(caster,row,spell), onreset(caster), ondaily(daily)` + `companionsSlot` snippet | composes CasterPanel + StaffPanel + CompanionsPanel |
| `CasterPanel` | `caster, slots, numbers, cantripRank, known, editable, offline` | `oncast(row,used), onprepare(row,spell), onreset()` | header pills from adapter numbers |
| `StaffPanel` | `daily {value,pending}, editable, offline` | `onchange(daily)` | whole-row `vitals.daily` writes; math in `sheet/staff.js` |
| `CompanionsPanel` | `baseSheet, view` | — | read-only by nature (Q4 ruling) |
| `InventoryPanel` | `baseSheet, itemBulk, itemTraits, qtyMap, money, editable, offline` | `onqty(name,qty), onmoney(money)` | `qtyMap` is a plain object per render (SSR-safe); math in `sheet/bulk.js` |
| `StrikesPane` / `StrikeRow` | `view` / `strike` | — | read-only; trait *text* is E9 |
| `FeatsPanel` | `baseSheet, editable` | — | view-only renders feats + features stacked (no tabs = no controls) |
| `SheetView` | `character (bootstrap payload), accountSub, editable` | — | **the drill-in**: E10 renders it with `editable={owner}` |

## Primitives

| Unit | Props | Events | Notes |
|---|---|---|---|
| `Dialog` | `open, title, children, footer` (snippets) | `onclose()` | native `<dialog>` + `util/keyboard.js` once: focus trap, Escape cancels, Enter commits, focus restores |
| `EmptyState` | `onimport()` | — | the designed no-character screen |
| `ErrorState` | `message, detail, onretry` | — | retry affordance only where a retry exists |
| `Skeleton` | `panes` | — | pane-shaped loading, never blank |
| `SpellSlotRow` | `row, editable, offline` | `oncast(used), onprepare()` | spent strike-through; pending tint |

## `editable` audit (all green — each pinned by a test)

- `CharacterHeader` view-only: level pill, no New Day / adjust buttons — `tests/shell/components.test.js`
- `StatsPane` view-only: **zero `<button>` elements** — `tests/shell/components.test.js`
- `PipRow` view-only: pips render as `<span>`, no buttons — `tests/shell/components.test.js`
- `CasterPanel` view-only: zero buttons, no reset — `tests/sheet/magic.test.js`
- `InventoryPanel` view-only: zero inputs, quantities as text — `tests/sheet/inventory.test.js`
- `FeatsPanel` view-only: no tabs, both lists stacked — `tests/sheet/strikes-feats.test.js`
- `HpBar`/`SpellSlotRow`/`StaffPanel`: controls gated on `editable` with disabled-when-busy (pending/offline) semantics — `degraded-mode` §4

## Data-flow rules E10 must keep

1. Components never touch the socket — `sheet/state.js` is the only module
   over `createSync`; E10 mounts its own `SheetView` with `editable={owner}`.
2. Numbers come from `derive(character, liveState)` only (the engine seam);
   when E8 lands the same components render engine output unchanged.
3. Writes go through the state layer's write surface (client bounds live
   there); `rejected`/`forbidden` surface on `sheet.opErrors` keyed by field
   target — a card should show them at the same control.
4. `superseded` reverts silently — never render anything for it.
