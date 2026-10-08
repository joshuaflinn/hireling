# Design: Party View, GM Seat, PWA (E10)

**Spec**: [spec.md](spec.md) · **Epic**: gh#12 · **Signed**: Korrin, 2026-10-08
(stays inside one epic's P0 scope — per the pipeline's gate rules this design
is mine to approve; the PR is the board's review surface)

**Verified against** `main` @ be4b08c. Every seam this design cites was read
in the tree, not assumed: `SheetView.svelte` (props + the `POC_PARTY_ID = 1`
constant whose comment says "revisited with E10"), `sync/index.js`
(`snapshotForBoot()` is documented "E10's cold-boot input"),
`authz.rs` (`Read → Allow`), `http.rs` (route table + `ServeDir` root),
`engine/index.js` (`derive` forwards wire EngineOutput), `EffectsStrip`
(`$view.effects`).

---

## 1. Context and shape of the problem

E10 is a composition epic. Sync, engine, auth, import, and the sheet's
component library all exist and are merged. What is missing is exactly three
things:

1. A **read** that gives the client every party character's base data (the
   WS snapshot already carries every character's *live* fields and derived
   output — but not base sheets or names).
2. A **party home screen** (roster cards + drill-in + first-run states) that
   renders off that read plus the existing sync store, reusing E6 units.
3. A **PWA shell** (manifest, icons, service worker) that installs and
   survives offline cold boots per E7's published contract.

The design risk is not building these — it is accidentally building a second
copy of something E5–E8 already own. The guardrails (component reuse, chip
truth, explicit cache strategy) exist because the failure mode here is
duplication, not incapacity.

## 2. Decisions, with alternatives

### D1 — The roster read: one self-scoped party endpoint

**Chosen**: `GET /api/party/roster` inside the session-protected nest,
returning `{party_id, you, characters[]}`, each character exactly the shape
`GET /api/characters/me` returns today. Party resolution: the caller's own
character's party; a caller with no character gets the single POC party
(the seed guarantees exactly one; more than one for a characterless caller
is a 409 — explicit, not guessed).

| Option | Verdict |
|---|---|
| **A. Self-scoped party roster read** (chosen) | One request boots the whole surface; `me`-shape payloads feed `SheetView` unchanged; REST stays the bootstrap channel per the PRD's REST/WS split; characterless callers (GM) still resolve. |
| B. `GET /api/characters/{id}` per character | N requests; no party-resolution path for the GM (owns no id); invites a "fetch on tap" UX where drill-ins stall. Rejected. |
| C. Base sheets on the WS hello/snapshot | Puts bootstrap data on the reconnect path — every reconnect re-ships every sheet; violates the REST=bootstrap/WS=live split E7 pinned. Rejected. |

Implementation: extract the per-character payload assembly from
`pbimport::handlers::load_me` into `pbimport::payload::character_payload(pool, character_id)`
(summary + base_sheet + vitals + slots + inventory) and call it from both
handlers — `me` becomes the one-character special case of roster. New module
`src/party/` owns the roster query (characters of the party) and the
resolution rule; the handler is three lines over it. Route joins `ROUTES`
(the router-test table demands it) with `mutates: false`.

### D2 — One socket per tab: the session sync, owned by the shell

**Chosen**: `web/src/lib/party/session.js` creates **one** `createSync` per
logged-in tab, using the real `party_id` from the roster read. The party
view and every `SheetView` mount consume that instance. `SheetView` gains an
optional `sync` prop — present, use it; absent, build its own exactly as
today (all existing callers and tests unchanged). The hardcoded
`POC_PARTY_ID = 1` constant retires here; its comment said E10 would.

| Option | Verdict |
|---|---|
| **A. Shell-owned session sync** (chosen) | One WS per tab; roster and sheet see literally the same store (chips/HP equality is by construction); connection state is one truth for the whole app. |
| B. Each surface creates its own sync | Two sockets per tab, two snapshot merges, two reconnect state machines that can disagree — and the roster would drift from the sheet between merges. Rejected. |
| C. SharedWorker single-socket | Explicitly out of P0 scope (E7 contract §7). Not entertained. |

The party module never writes. `sync.write` stays reachable only through the
sheet state layer — the party surfaces render view-only or delegate editing
to `SheetView`, which already owns the write surface and its
`opErrors`/pending semantics.

### D3 — Roster card: compose E6 units, add only the card chrome

**Chosen**: `RosterCard.svelte` (new, `web/src/lib/party/`) composes:

- `CharacterHeader` view-only — name + level pill (its stated E10 consumer
  role in E6's components.md),
- `HpBar` with `editable={false}` — renders the bar with temp-HP segment,
  no controls (its stated E10 consumer role),
- `EffectsStrip` with `effects={view.effects}` — the *same component and
  the same prop source* the sheet uses (`$view.effects`), which is the
  chip-truth guardrail made structural,
- a portrait-initial avatar element and the card's down/max state class —
  card chrome, no existing unit renders these, nothing is re-implemented.

Down/max are pure renders of store state: `hp === 0` → `down`, and
`hp === max` (max = `view.render_base.hp_max`) → `full`; `max` null (derived
not yet delivered) → skeleton bar, no placeholder numbers — the same honest
null E6's sheet renders. `SyncIndicator` renders on the owner's own card
only (the queue is account-local; other members' cards have no queue to
show).

`PartyView.svelte` renders the card grid from the roster payload + a
per-character `derive(session.sync, id)` subscription; a tap opens the
drill-in (`SheetView`, `editable` per FR-3) as an in-shell view state with a
back affordance. No router library — the app keeps its `view` state machine,
now rooted at `party`.

### D4 — First-run and GM states

- Empty party (zero characters, player viewer): E6's `EmptyState` with its
  `onimport` → E5's `ImportPage` (both exist; nothing new is designed).
- Characterless player, non-empty party: roster plus an "Import your
  character" affordance in the party header (`CharacterHeader`-adjacent
  placement, player-only).
- GM: never sees either affordance; the empty party for the GM renders the
  roster grid with an empty note, no CTA (`role` rides the roster read's
  `you`).
- Import success returns to the roster; the roster refetches (one fetch —
  no invalidation machinery).

### D5 — PWA shell: manifest + icons + hand-rolled SW with a build plugin

**Chosen**: `web/public/` (new — vite copies it to the dist root, which the
axum `ServeDir` already serves) holds `manifest.webmanifest`, `icon-192.png`,
`icon-512.png`, `icon-maskable-512.png` (placeholder monogram, flagged
swappable), and the SW is *emitted by a small vite plugin*
(`web/plugins/sw-plugin.mjs`): at `closeBundle` it writes `dist/sw.js` with
the final hashed asset list and a `BUILD_ID` (content hash of the bundle).
Registration lives in `web/src/lib/pwa/register.js`, called from `main.js`
only when `import.meta.env.PROD` — dev stays SW-free.

Fetch strategy (pure functions in `web/src/lib/pwa/strategy.js`, imported by
the emitted `sw.js` — unit-testable without a SW runtime):

| request | strategy |
|---|---|
| hashed `/assets/*`, icons, manifest | cache-first (immutable — content-hashed or static) |
| navigation (mode `navigate`) | network-first; on failure serve cached `index.html` |
| `/api/*`, WS | **never intercepted** (fall through to network; offline behavior is the app's, per E7) |

Caches are named `hireling-shell-{BUILD_ID}`. On activation the SW deletes
every cache whose name isn't the current build's — **invalidation keyed to
release, never wall clock** (the guardrail, answered explicitly). Update
policy: `skipWaiting()` + `clients.claim()`; the page reloads once on
`controllerchange` when a controller already existed (first install does not
reload). Rejected alternative — a "new version available" prompt: six
trusted users, a queue that survives reload, and a guardrail that calls
stale shells silent failure; the prompt optimizes for ceremony. The reload
is safe *because* E7's queue and boot cache are reload-durable (D6).

| Option | Verdict |
|---|---|
| **A. Build-plugin-emitted SW, hand-rolled** (chosen) | Precache list can never drift from the bundle (generated, not maintained); zero new dependencies (Article V); ~60 lines total; strategy logic is pure and testable. |
| B. Workbox | New dependency needing written justification; generates more machinery than a six-user shell needs; its precache manifest would still need the plugin wiring. Rejected. |
| C. Hand-maintained `public/sw.js` with a hardcoded asset list | The list rots on every hashed rebuild — the exact silent-stale failure the guardrail names. Rejected. |

### D6 — Offline cold boot: the app owns the data, the SW owns the shell

**Chosen**: the division E7's contract §6 draws, taken literally: the SW
serves the shell; the *app* persists and seeds last-known state.

- `web/src/lib/party/boot.js`: after each successful roster fetch, and
  throttled (2 s trailing) on store merges, write
  `hireling:boot:{account_sub}` = `{ roster, snapshot: JSON.parse(sync.snapshotForBoot()) }`.
  Keyed per account — the queue's isolation rule, same pattern.
- `createSync` gains one optional input, `bootSnapshot` — a wire-shaped
  snapshot applied through the **existing** merge path (strictly-newer, same
  code the `snapshot` frame uses). No new reconciliation exists anywhere;
  this is the contract's sanctioned inverse of `snapshotForBoot`.
- Cold boot, backend unreachable: shell from SW cache; roster read fails;
  boot cache renders the roster read-only (offline state from the sync
  surface); sheets open view-only off cached base sheets + seeded store.
- First run after login: `navigator.storage.persist()` requested once per
  account (logged result; a denial is non-blocking — eviction then behaves
  as E7's documented known limit). The queue's keys and contents are
  untouched: no migration exists to do, which *is* the migration answer the
  contract asked E10 to treat as a first-run concern.

**Deviation, stated**: contract §6 says "the SW caches it [the store
snapshot] versioned and invalidates by app release". This design keeps the
snapshot in app-owned localStorage under a persistent grant instead of a SW
cache. Reasons: the snapshot is per-account data the page must merge by
version anyway (a SW-held copy would be a second copy with its own
invalidation bug surface); localStorage is already the queue's per-account
home; and version-merge — not release invalidation — is what makes stale
snapshots safe (§2's merge rule). The contract's *intent* (cold offline boot
renders last-known state, durably, never silently stale) is met more simply.
Recorded here for the reviewer; if the reviewer wants the letter of §6, the
change is localized to `boot.js`.

### D7 — Server additions are exactly one read

`src/party/mod.rs` (resolution + query + handler), the `character_payload`
extraction in `pbimport`, the route registration, and the `ROUTES` table
entry. No migrations (roster reads existing tables), no new write paths, no
engine changes, no WS protocol changes. Observability: the handler logs
request id + party id + character count at `debug`, joins the existing
request-id middleware for free, and needs no new audit event (it is a read
by authenticated actors; E3's audit surface is unchanged).

## 3. File map

**Server (Rust)**
- `src/party/mod.rs` — new: party resolution rule, roster query, handler.
- `src/pbimport/payload.rs` — new: `character_payload` extracted from
  `load_me`; `handlers.rs` `me` refactored onto it (behavior identical).
- `src/http.rs` — route + `ROUTES` entry (`mutates: false`).
- `src/party/tests.rs` / `src/tests/` — router-driven roster tests (see §6).

**Web (Svelte)**
- `web/src/lib/party/session.js` — new: the shell's single `createSync`
  (+ `bootSnapshot` wiring), role/account context, roster fetch + boot-cache
  write.
- `web/src/lib/party/boot.js` — new: boot-cache read/write/throttle,
  `persist()` grant.
- `web/src/lib/party/PartyView.svelte` — new: roster grid, header
  affordances (import CTA per D4), empty states.
- `web/src/lib/party/RosterCard.svelte` — new: card chrome per D3.
- `web/src/lib/sheet/SheetView.svelte` — modified: optional `sync` prop;
  retire `POC_PARTY_ID`.
- `web/src/lib/sync/index.js` — modified: `createSync({..., bootSnapshot})`.
- `web/src/App.svelte` — modified: party home + drill-in view states.
- `web/src/lib/pwa/strategy.js` — new: pure fetch-strategy + cache-name
  functions (consumed by the emitted SW and by tests).
- `web/src/lib/pwa/register.js` — new: PROD-gated registration.
- `web/plugins/sw-plugin.mjs` — new: vite plugin emitting `dist/sw.js`.
- `web/public/manifest.webmanifest`, `web/public/icon-{192,512,maskable-512}.png`
  — new statics.
- `web/src/main.js` — modified: register call.
- `web/index.html` — modified: manifest link, theme-color, icon links.

## 4. Data flow (one direction, no forks)

```
login → /api/me (E3) → /api/party/roster (FR-5)
  → session.js: one createSync(party_id, bootSnapshot?) ── WS (E7)
  → PartyView: cards ← roster payload + sync store + derive(sync, id) (E8)
  → tap → SheetView(character, accountSub, editable, sync) — E6, unchanged
  offline cold boot → SW shell cache + boot.js seed → same render path
```

Cards and sheets read the same stores through the same functions; there is
no second derivation, no second socket, no polling. The only new producer
is the roster read; the only new persistence is the boot cache.

## 5. Error handling

- Roster fetch fails, boot cache present → render cached roster read-only +
  a retry affordance (`ErrorState`'s pattern); no modal.
- Roster fetch fails, no cache → `ErrorState` with retry (existing unit).
- Derived null → card bar skeleton; chips hidden (empty array); name/initial
  still render from the roster payload (identity is base data, not derived).
- Import failure → E5's page behavior, untouched.
- SW registration failure → logged, non-blocking: the app is a web page
  first; installability is the enhancement.
- `persist()` denial → logged once; no user-facing state.

No new error UI is designed anywhere. E7's silence rule covers every sync
edge; input errors surface at the sheet's controls exactly as E6 built.

## 6. Testing strategy (production-path rule, per AGENTS.md)

- **Roster read** — through the axum router (`src/tests` pattern, like the
  observability rule): member sees all party characters; GM sees the roster;
  unauthenticated 401; `me`-shape equality asserted field-by-field against
  the `me` handler's output for the same character (the shape contract);
  two fixtures differing in `identity.name` assert two different summaries
  + portrait initials (imported-field rule).
- **Resolution rule** — party tests: characterless caller with exactly one
  party → that party; with zero parties → honest 409/404, never a guess.
- **Cards/roster/GM/empty states** — vitest + `@testing-library/svelte`
  (jsdom), rendering the *non-default* states the spec names: GM roster
  (zero edit buttons, no CTA), down-state card (hp 0), full-state card,
  non-owner drill-in (`editable=false` → view-only), characterless-player
  header CTA, empty-party CTA, offline cold boot from seeded boot cache.
  Chip-truth test: render card and sheet for the same seeded store; assert
  equal chips markup/labels.
- **Sync seam** — `createSync` with `bootSnapshot`: seeded fields merge by
  version; a live `snapshot` frame with older versions changes nothing
  (strictly-newer proof, two fixtures).
- **SW strategy** — pure-function tests: classification of hashed asset /
  navigation / `/api` (never cache) requests; cache-name versioning; old-
  cache deletion list. Plugin test: emitted `sw.js` contains the final
  hashed filenames and the build id (node-level test over the plugin's
  output).
- **Registration** — `register(env)` unit: PROD registers, DEV does not.
- **SC-5 manual checklist** — documented in the PR (install, standalone,
  offline reload, second-build cache discard); not CI-automated at POC and
  said so honestly.

## 7. Self-review

Placeholder scan: none (every file named, every strategy tabulated).
Consistency: FR-1..FR-10 each map to D1–D7 and the file map; the three
guardrails map to D3 (reuse), D5 (cache strategy), D3/§4 (chip truth).
Scope: one epic, no migrations, one new route — nothing here moves the
P0/P1/P2 line. Ambiguity: down/max defined numerically; resolution rule
defined for every caller class; SW policy stated with its rejected
alternative. Ready for `sdd-plan`.
