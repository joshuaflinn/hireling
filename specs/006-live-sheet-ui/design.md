# Design — E6: Live Sheet UI (desktop)

**Inputs:** `spec.md` (this directory, clarify rulings §8 encoded) ·
`docs/PRD.md` FG1/FG2 · `specs/007-party-sync/contracts/wire-protocol.md` +
`degraded-mode.md` (binding) · `specs/008-buff-effect-engine/contracts/engine-output.md`
(the E6/E8 seam, referenced not duplicated) · E5's `base_sheet`
(`src/pbimport/transform.rs`) · the post-#33 prototype at `main` `73839aa`.

## 1. Context

E5 imports and stores `characters.base_sheet` (identity, abilities +
breakdowns, hp inputs + `max_hp`, `acTotal` parts, proficiencies, lores,
spellcasters with `per_day`/`known`/`prepared`, equipment with container
names, containers with the `extradimensional` flag, companions). E7 ships the
client sync stack (`web/src/lib/sync/` — `createSync`, store with
strictly-newer merge + optimistic echo, durable queue, connection with silent
backoff) and the two binding contracts. E8's engine ships in parallel; its
output reaches the sheet through the engine-output contract only. E6 renders.

## 2. Architecture

```
web/src/
  App.svelte                  view switch: import | sheet | (E10: party)
  lib/
    sync/                     E7 — untouched except additive reads
    engine/                   THE SEAM (§4)
      index.js                adapter interface + mode switch
      base.js                 base-only derivation from base_sheet (+ level_adjust)
      format.js               signed numbers, bulk text (pure, tested)
    sheet/
      state.js                Svelte stores: character view model over
                              base_sheet + sync store + adapter
      components/             CharacterHeader, HpBar, StatTile, SkillRow,
                              StrikeRow, SpellSlotRow, CasterPanel, StaffPanel,
                              PipRow, MagicPane, StrikesPane, StatsPane,
                              InventoryPanel, CompanionsPanel, FeatsPanel,
                              SyncIndicator, EmptyState, ErrorState, Skeleton,
                              Dialog
                              (planned, not built: EffectsStrip — E8/E10;
                              Tooltip — E9 prose; SkillList/ContainerGroup/
                              CoinBar — folded into StatsPane/InventoryPanel)
    util/
      inert-html.js           setHTML port (DOMParser, no script execution)
      keyboard.js             focus trap, Escape/Enter dialog pattern
  app.css                     prototype design language, ported (§8)
```

**Data flow (read):** auth → bootstrap (account, character, `base_sheet`,
item-bulk map §5) → open party socket → `snapshot` merge → adapter derives →
stores feed components. **Data flow (write):** component → client validation →
`sync.write(target, value)` → optimistic echo (store tags pending) → server
CAS → `ack` settles / `diff` merges → store event → component re-renders.
Components never touch the socket; only `sheet/state.js` subscribes to the
sync store.

**Server:** one migration (§3) + the vitals write path extended to the three
new fields + bootstrap payload gains the item-bulk map (§5). No new routes,
no new services. The static bundle is served by the existing axum shell (E1).

## 3. Field-set extension (Q1 ruling — E6 owns this contract PR)

**Migration** `20261002000001_vitals_spell_economy.sql` (down included),
columns on `character_vitals`, exactly the E2 pattern:

```sql
focus_current    int  NOT NULL DEFAULT 0 CHECK (focus_current >= 0),
focus_version    bigint NOT NULL DEFAULT nextval('field_version_seq'),
hero_points      int  NOT NULL DEFAULT 0 CHECK (hero_points >= 0),
hero_points_version bigint NOT NULL DEFAULT nextval('field_version_seq'),
daily            jsonb NOT NULL DEFAULT '{"staff_charge_rank":0,"staff_spent":0,"drain_used":false}',
daily_version    bigint NOT NULL DEFAULT nextval('field_version_seq'),
```

**Wire protocol §3 addition** (PR to `wire-protocol.md` — the sanctioned
extension path): three new `vitals` field values —

| `field` | value shape | notes |
|---|---|---|
| `focus_current` | int ≥ 0 | max bound is the character's focus max (base_sheet); client clamps, server validates ≥ 0 only |
| `hero_points` | int ≥ 0 | |
| `daily` | `{staff_charge_rank: 0..10, staff_spent: int ≥ 0, drain_used: bool}` | whole-row write, one version unit |

`snapshot`/`diff` carry them like every vitals field. Server write path: the
existing vitals handler pattern (`src/sync/write.rs`) extended field-by-field;
each new field gets a sync-write test (valid, bounds-rejected, CAS-superseded)
in `src/tests/sync/write.rs` style.

**Client semantics:** focus pips write `focus_current`; hero pips write
`hero_points`; Staff Nexus charge-rank select and charge spends write `daily`;
Drain Bonded Item toggle writes `daily`. New Day = N writes (all used slots →
false, focus → 0, daily → zeroed) enqueued in one burst — the queue is FIFO
per account, so the burst is atomic-enough (CAS per field, no cross-field
transaction needed; a partial replay converges because every write is
idempotent and absolutely-valued).

## 4. The engine seam (adapter)

`lib/engine/index.js` exports one function:

```
derive(character, liveState) → DerivedSheet
```

`DerivedSheet` (shape pinned by the E8 engine-output contract): per-stat
derived values keyed by the FG3 vocabulary (`ac`, `fort`, `ref`, `will`,
`perception`, `speed`, `class_dc`, `spell_dc`, `spell_attack`, `attack`,
`skill:<name>`, strike rows), each carrying `{ value, provenance[] }` where
`provenance[]` lists contribution parts and — post-E8 — suppressed sources;
plus `effect_chips[]` and `hp_max`.

- **Now (base-only mode):** `base.js` computes from `base_sheet` inputs +
  `level_adjust`, zero effects, `effect_chips: []`, provenance = base parts
  only (e.g. AC = acTotal ability + prof + item parts; saves/skills = ability
  + proficiency(rank, level); strikes = weapon math + MAP −5/−10; hp_max =
  E5's formula at adjusted level; cantrip/focus rank = ceil(level/2)). The
  prototype's formulas are the reference implementation; each is unit-tested
  against the reference export fixture (`tests/data/pb_export_reference.json`).
- **At E8:** swap `index.js` to forward engine output. `base.js` is deleted,
  not kept in parallel. The effects strip and provenance hovers (suppressed
  sources included) activate with the engine — nothing before it pretends.

**Rejected:** computing derived stats inside components (no seam, E10/E14
would fork the math); running the engine in WASM client-side (transport +
build complexity the contracts don't ask for; E8's recomputation/broadcast
design owns that decision).

## 5. Item Bulk resolution

`base_sheet` carries no Bulk. E4's items corpus does. The bootstrap payload
gains `item_bulk: { "<exact item name>": <tenths-of-bulk int | null> }` for
the character's imported item names, resolved server-side by exact-name match
(case-insensitive, the E12 book-value precedent); unresolved names → `null` →
rendered "—", never blocking. Container rollup: sum member item bulks × qty
in tenths (`L`=1, number=×10, "—"/null=0), **skip entire containers flagged
`extradimensional`**, format via `format.js` (X Bulk + Y L). Character total
excludes extradimensional contents. Unit tests use a fixture map, not the
live corpus.

## 6. Components and the E10 reuse contract

Every component accepts `editable` (default true) and renders view-only when
false — no disabled-control litter, simply no controls. E10's roster cards
compose `HpBar` + an effect-chips unit + `SyncBadge`; the effects unit and
the badge do not exist yet (E8 owns the chips' data, E10 builds both —
components.md states this plainly after MOR-48 finding 9). Its drill-in
renders the full `SheetView` with `editable={owner}`. The component API
(props, events) is part of this epic's review criteria: E10 must consume,
not re-implement (EPICS conflict risk). `Dialog` implements the
focus-trap/Escape/Enter pattern once; every dialog (prep picker, level-down
confirm, New Day confirm)
uses it.

## 7. States

- **Empty (no character):** designed screen — house crest, "Import your
  character" CTA → import view (E5). Not a stub.
- **Loading:** skeletons in pane shape; never a blank flash; the sync store's
  last-known state renders instantly when present (offline cold boot included).
- **Error:** per-pane calm error with retry where one exists (socket
  reconnects itself — no retry button for sync; bootstrap/import errors name
  the failure).
- **Pending (optimistic):** value renders with a subtle pending tint +
  control disabled until ack (degraded-mode §4); `rejected`/`forbidden` →
  inline message at the control, auto-clears on next ack; `superseded` →
  silent revert, nothing shown, ever.
- **Offline:** edit affordances disable via a single `offline` derived store;
  reads stay live; `SyncIndicator` shows iff queue non-empty (not connection
  state) — ≤1 Hz pulse, neutral colour, prototype's chrome placement.

## 8. CSS / design language port

The prototype's stylesheet is ported by hand into `app.css` + per-component
scoped styles: same custom properties (panel/gold/dim palette, Palatino
headings), same three-column grid (`board`/`panel`/`invwrap` structure),
same tile/pip/bar/chip idioms. No component library, no CSS framework. The
prototype file stays the reference; where its CSS fights Svelte scoping, the
component-local copy wins and the delta is noted in review.

## 9. Accessibility

All controls are real buttons/inputs (focusable, operable); drag-to-prepare
has a keyboard path (slot focused → Enter opens the prep `Dialog` → arrow
keys pick a spell); level-down and New Day confirm via dialog, not
`window.confirm`; tooltips open on focus as well as hover and are
dismissable with Escape; colour never carries sole meaning (pips show
values in `aria-label`s).

## 10. Testing

- **Rust (migration + write path):** migration test (up/down, defaults);
  sync-write tests per new field (apply, bounds-reject, supersede);
  bootstrap payload shape test. All drive the production router/store paths
  per the AGENTS observability rule.
- **Web (vitest, E7's pattern + fakes):** `base.js` derivation vs the
  reference fixture (AC, each save, skills, strikes + MAP, hp_max at levels
  1/3/20, cantrip rank, level_adjust re-derives); `format.js`; bulk rollup
  incl. extradimensional skip; `state.js` write flows with E7 fakes (echo →
  ack applied; superseded silent revert; rejected inline error; New Day
  burst); `inert-html.js` (script tags never execute); component tests for
  HpBar clamp/temp-segment and slot cast toggling; empty/loading/error
  renders.
- **Gate:** `just ci-local` (fmt, clippy-deny, tests, cargo-deny, web suite)
  green before PR; grizzly-gate fail-closed; a human merges.

## 11. Decisions (with rejected alternatives)

1. **Three new vitals columns vs one opaque `spell_economy` JSON blob** —
   columns: per-field CAS versions match how the UI writes them (focus pips
   and hero points move independently mid-fight). Rejected: one blob would
   make two players' concurrent pips-tapping fight over one version for no
   gain.
2. **`daily` as one whole-row field** — the three values reset together
   (New Day) and are read together; splitting them buys three versions for
   values that never race.
3. **Bootstrap-resolved Bulk map vs enriching E5's `base_sheet`** — E5 is
   merged; re-touching its transform for a display concern couples the
   importer to corpus availability. Rejected alternative noted for E15-era
   review. The map is additive, per-character, and cheap.
4. **Base-only adapter behind the seam vs waiting for E8** — waiting
   serializes Lane A on Lane B; the adapter debt is one bounded module with
   a named deletion date (E8's adapter swap).
5. **Fixed three-column layout vs porting the prototype's drag-resize
   board** — the PRD names the three-column design; customization is parked
   scope. Porting it would spend P0 budget on P2-adjacent chrome.
6. **qty-0-as-removal vs a deleted-state field** — the restore picker was
   scaffolding for prototype-local state; server truth already encodes
   "gone" as 0 and re-import restores export quantities per E5 anchoring.

## 12. Risks

- **Adapter drift:** base.js math diverging from E8's base path. Mitigation:
  same fixture, same worked numbers; E8's spec lists the reference export as
  its fixture too; the swap review diffs both outputs before deleting base.js.
- **New Day burst under CAS:** a slot superseded mid-burst re-renders to the
  winner — acceptable by contract (silent, convergent); noted so nobody
  "fixes" it with a lock.
- **Bulk map misses** (corpus gaps): degrade to "—", never block rendering;
  misses logged server-side at bootstrap build for E4 follow-up.
- **Scope pull from the prototype's richness:** the scope table is the fence;
  anything not E6 in it gets bounced, cheerfully, with its owner named.
