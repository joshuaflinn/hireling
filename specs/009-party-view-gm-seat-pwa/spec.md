# Feature Specification: Party View, GM Seat, PWA (E10)

**Epic**: E10 — Phase 1, Lane B, P0 · depends on E6 (sheet UI) + E7 (party sync) + E8 (buff/effect engine) · blocks E11 (go-live) · GitHub issue #12
**Created**: 2026-10-08
**Status**: Specify gate run 2026-10-08 (Korrin, front half of the SDD pipeline — specs only, no code in this pass); clarify answers folded same day from the existing corpus (see *Clarify Log*); design signed by Korrin 2026-10-08
**Input**: `docs/EPICS.md` Epic E10 (specify prompt + Constraints + AI Guardrails); PRD v3.6 FG5 (GM view, PWA install) + FG2 (offline rules) + Entry Point & Party Screens sections; `CONSTITUTION.md` Articles I/III/V/VI; E6 `specs/006-live-sheet-ui/components.md` (the reuse contract); E7 `specs/007-party-sync/contracts/degraded-mode.md` + `wire-protocol.md` (the offline build-target, FR-12); E8 `specs/008-buff-effect-engine/contracts/engine-output.md` (the derived view chips render from)

---

## User Scenarios & Testing *(mandatory)*

E10's users are the six party accounts plus one epic downstream: E11
(go-live deploys the PWA this epic installs). E10 ships the surface the
whole product lands on — every account's home screen — and the installable
shell. It adds no sync, no engine, no import semantics of its own; every
number it renders arrives through a seam an earlier epic owns.

### User Story 1 — The table at a glance (Priority: P1) 🎯 MVP

Becky opens Hireling and lands on the party screen: a roster of cards, one
per character — name, portrait initial, HP bar with down/max state, active
effect chips with sources. When Josh's HP drops and Bear's Bless lands on
him, both move on Becky's screen without a refresh. Tapping a card opens
that character's full sheet; Becky's own card opens her sheet editable,
everyone else's read-only.

**Why this priority**: This is the party-sync concept validated — US-9 and
the PRD's Party Screens section. The roster is where the product's promise
("what I see is what the party sees, right now") becomes visible to
everyone at once.

**Independent Test**: With a seeded party, open the party screen as a
player; commit an HP write and an effect write from a second client;
assert the roster card's HP bar and chips update without reload, the
chips' content equals the sheet's chips for that character, and tapping a
non-owned card renders the sheet with zero edit controls.

**Acceptance Scenarios**:

1. **Given** a party with one or more characters, **When** any account
   opens Hireling after login, **Then** the party screen renders the
   roster: one card per party character with name, portrait initial, HP
   bar showing down/max state, and active effect chips with sources.
2. **Given** the party screen is open, **When** another member commits any
   synced write (HP, temp HP, effect change, level adjust), **Then** the
   roster reflects it without a page reload — same WS fan-out the sheet
   consumes, no second sync path.
3. **Given** a roster card, **When** the viewer taps it, **Then** the
   character's full sheet opens, reusing E6's `SheetView` — read-only
   unless the viewer owns the character (ownership gates writes, nothing
   gates reads).
4. **Given** a character at 0 HP, **When** their card renders, **Then**
   the card shows the down state distinctly; at full HP the card shows
   the max state.
5. **Given** a character with active effects, **When** their card renders,
   **Then** the chips and their sources equal what that character's own
   sheet renders for the same state — one engine source, two views.

### User Story 2 — Bruce watches, and the app never asks anything of him (Priority: P1)

Bruce (the GM account) logs in and lands on the party screen. Every card
is tappable into a full read-only sheet, but no edit affordance, no import
CTA, no composer, nothing interactive beyond navigation ever renders for
him. This is enforced server-side by E3 (GM writes are rejected and
audited per message and per request) and reflected in the UI by
construction — the GM seat simply renders everything view-only.

**Why this priority**: The PRD's decided GM-view ruling and Constitution
Article I — the GM seat is read-only by design, zero GM workload. It is
also E3's acceptance made visible: server-enforced and UI-hidden are two
layers of the same rule.

**Independent Test**: Log in as the GM seat (dev seat `bruce`); assert the
party screen renders the roster, every drill-in is view-only (zero
`<button>` elements in edit roles), the import CTA never renders, and a
forged write attempt from that session is rejected server-side with an
audit row.

**Acceptance Scenarios**:

1. **Given** the GM account, **When** it opens Hireling, **Then** it lands
   on the party screen — the roster is its home, identical data to any
   member's view.
2. **Given** the GM account viewing any character's sheet, **When** the
   sheet renders, **Then** no edit control exists — E6's view-only mode
   (`editable: false`), not disabled controls.
3. **Given** the GM account, **When** the party screen renders, **Then**
   no "import your character" CTA or other write affordance renders.
4. **Given** the GM account, **When** it sends any write (REST or WS),
   **Then** E3 rejects and audits it server-side — the UI hiding is
   courtesy, the server is the rule.

### User Story 3 — First run: an empty hall and one obvious door (Priority: P1)

Josh logs in before anyone has imported a character. The party screen
renders empty with the "Import your character" CTA (paste or upload — E5's
import page). Once his import lands, the roster shows his card. A player
whose party has characters but who has none of their own still sees the
roster — plus the same import affordance until they have a character.

**Why this priority**: The PRD's adopted first-run design (no longer
TBD): empty party screen with the import CTA. It is the onboarding path
for five of six accounts.

**Independent Test**: With an empty party, log in as a player; assert the
CTA renders and routes to the import page; after a successful import,
assert the roster shows the character. With a non-empty party, log in as a
characterless player; assert the roster renders plus the import
affordance; as GM, assert no CTA renders.

**Acceptance Scenarios**:

1. **Given** a party with zero characters, **When** a player logs in,
   **Then** the party screen shows the designed empty state with the
   "Import your character" CTA.
2. **Given** a successful import, **When** the roster renders, **Then**
   the imported character's card is present without a manual reload.
3. **Given** a characterless player in a non-empty party, **When** the
   party screen renders, **Then** the roster shows the party's characters
   and the import affordance remains available.
4. **Given** the GM account, **When** any party state renders, **Then**
   no import CTA exists anywhere in the GM view.

### User Story 4 — Installable, and stale is loud to the deployer, silent to the table (Priority: P1)

Josh installs Hireling on his desktop from the browser's install
affordance: it opens as a standalone window with icon and splash per the
browser's PWA treatment. The service worker caches the app shell keyed to
the app release, so an offline cold boot still renders the party screen
and sheets read-only from last-known state (E7's contract), and a new
deploy never leaves a member silently running an old shell against new
wires.

**Why this priority**: The PRD's PWA install ruling plus E7 FR-12's
build-target: E10 owns everything service-worker. The guardrail is
explicit — stale sheets at the table are the product failing silently, so
cache versioning and invalidation must be a stated strategy, not a
default.

**Independent Test**: Build the bundle; assert the manifest and icons are
served with correct types and the SW registers in the built app; with the
server stopped, reload the installed app and assert the party screen
renders from last-known state read-only; ship a second build and assert
the old cache is discarded on activation and the new shell is served.

**Acceptance Scenarios**:

1. **Given** the built app, **When** a desktop browser visits it, **Then**
   it is installable — web manifest with icons, standalone display, theme
   colors — using the standard manifest + service worker, no PWA
   framework.
2. **Given** an installed app and a reachable backend, **When** the user
   opens the app, **Then** it operates normally — the shell cache never
   changes live-data behavior (API and WS traffic bypass the cache).
3. **Given** an installed app and an unreachable backend, **When** the
   user reloads, **Then** the app boots offline: shell from cache,
   last-known roster and sheets rendered read-only per E7's degraded-mode
   contract — one offline path, no new semantics.
4. **Given** a new app release, **When** its service worker activates,
   **Then** caches from prior releases are deleted and the new shell is
   served — invalidation keyed to the release, never to wall clock.
5. **Given** the local write queue and store state in browser storage,
   **When** E10 first runs, **Then** it requests the persistent-storage
   grant so the browser may not evict last-known state or queued writes
   under pressure (E7's sanctioned durability upgrade; the queue's
   contents and keys are unchanged — no migration).

### User Story 5 — Offline at the table, the roster still tells the truth it last knew (Priority: P2)

The tunnel drops mid-session and Jake reloads his installed app out of
habit. The party screen renders from last-known state — cards, bars,
chips, all as of the last merge — read-only, with no error theatre beyond
E7's affordance disabling and silent reconnection when the link returns.

**Why this priority**: P2 within this epic because E7 already owns the
in-session behavior; E10's increment is only that a *cold* boot (reload
during an outage) lands on the same read-only last-known view instead of
a dead shell. The contract for this exists (`snapshotForBoot` is E7's
shipped accessor for exactly this consumer).

**Independent Test**: Load the app online, stop the backend, reload;
assert the roster renders with the pre-outage state read-only and no
modal appears; restart the backend and assert the sheet reconnects and
live-syncs without a reload.

**Acceptance Scenarios**:

1. **Given** a previously-loaded party screen and an unreachable backend,
   **When** the user reloads, **Then** the roster renders from the
   persisted last-known state — read-only, skeleton only where nothing
   was ever known.
2. **Given** a cold offline boot, **When** the backend returns, **Then**
   the app reconnects silently (E7 FR-10) and state converges — E10 adds
   no reconnect behavior of its own.

---

## Edge Cases

- **Characterless viewer of a non-empty party** — the roster renders the
  party; the import affordance stays available to players (US3.3); the GM
  never sees it.
- **A party member re-imports (E5 re-import anchoring) while another
  member is offline** — the offline member's cached base sheet may be
  stale until reconnect; live fields merge by version on reconnect (E7
  FR-7). The roster's live data (HP, chips) comes from the sync store,
  never from the cached base sheet, so cards never render stale numbers.
- **Derived output not yet delivered** — until the snapshot's `derived`
  array or a `derived` frame arrives, `derive(sync, id)` is honestly
  `null`; cards render their loading state (bar skeleton, no placeholder
  numbers) exactly as E6's sheet does. No client-side derivation exists to
  fall back on — by design.
- **Temp HP on a roster card** — rendered as the distinct bar segment E6's
  `HpBar` already owns; the card adds no HP math.
- **Effects with `tracked_manually`** — the `tracked` badge renders on
  roster chips exactly as on sheet chips (same component, same data).
- **Portrait initial** — first character of the identity name; an empty or
  missing name renders a neutral placeholder initial, never a crash.
- **Two accounts, same character owner (multi-device)** — irrelevant to
  the roster: cards are views over shared party state; E7's two-device
  rules need no roster counterpart.
- **SW update mid-session** — a new release's SW precaches in the
  background and takes control per the stated update policy (design
  §SW); the write queue survives any reload (E7 contract §3), so an
  update-driven reload can lose no acknowledged or queued write.
- **Storage pressure** — without the persistent grant, browsers may evict
  local storage; E10 requests the grant at first run and treats a denied
  grant as a logged, non-blocking condition (the POC deployment's six
  trusted installs make denial unlikely; eviction then behaves as
  today's E7 known limit).
- **Deep-linking / browser refresh on a drill-in** — no client router
  exists at POC; a refresh returns to the party screen (the home), and
  the drill-in is re-entered by tap. Accepted POC simplicity; a routable
  shell is not on the P0 line.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1 (landing + roster)**: After login, every account lands on the
  party screen: the roster of the caller's party's characters. Each card
  MUST show name, portrait initial, HP bar with down/max state (0 HP =
  down; HP = max = full), and active effect chips with sources. The
  roster MUST render from the party bootstrap read (FR-5) plus the E7
  sync store — live changes arrive through the existing WS fan-out; E10
  MUST NOT create a second sync or polling path.
- **FR-2 (chip truth)**: Roster effect chips MUST render from the same
  engine state the sheet renders — `derive(sync, characterId)` → the
  EngineOutput `effects` array, via the same component E6's sheet uses
  for chips. No re-implementation of chip rendering, no separate effects
  query.
- **FR-3 (cross-member viewing)**: Tapping a card opens that character's
  full sheet by mounting E6's `SheetView` with the party-bootstrap
  payload for that character and `editable` true only when the viewer's
  account sub equals the character's owner **and** the viewer is a
  player. Every account can open every character; ownership gates
  writes, nothing gates reads — matching E3's server rule (`Read` →
  allow for any authenticated actor).
- **FR-4 (GM seat)**: The GM account lands on the party screen and MUST
  never be rendered an edit affordance, import CTA, or composer —
  view-only everywhere, by rendering with `editable: false`, not by
  disabled controls. Server enforcement is E3's, already shipped; this
  epic only proves the UI layer agrees with it.
- **FR-5 (party bootstrap read)**: The server MUST expose one
  session-protected read returning the caller's party roster: for every
  character in the party, the same payload shape `GET /api/characters/me`
  returns today (identity summary, base sheet, vitals, slot rows,
  inventory rows), plus the party id and the caller's role. Party
  resolution: the caller's own character's party; for a caller with no
  character (GM or characterless player), the single POC party. The read
  is allowed for every authenticated account (E3 `Read` → Allow); it
  MUST NOT expose characters outside the caller's party.
- **FR-6 (first run and empty states)**: With zero characters in the
  party, a player sees the designed empty state with the "Import your
  character" CTA (E5's import page). A characterless player in a
  non-empty party sees the roster plus the import affordance. The GM
  sees the empty roster state with no CTA ever.
- **FR-7 (PWA manifest)**: The built app MUST ship a web manifest —
  name, short name, start URL, standalone display, theme/background
  colors, icons at 192 and 512 px plus a maskable variant — as static
  files served at the app root; no PWA framework. Icon art at POC is
  placeholder (monogram in the design language), swappable as static
  assets without code change.
- **FR-8 (service worker — shell cache)**: A service worker MUST
  precache the app shell (the built `index.html`, hashed assets,
  manifest, icons) keyed to the app release. Fetch handling MUST be:
  hashed assets and icons cache-first (immutable); navigation requests
  network-first with cached-shell fallback; `/api/*` never intercepted.
  Activation MUST delete every cache from other releases — invalidation
  by release, never by wall clock. The update policy MUST be explicit
  in the design doc with its rejected alternative recorded.
- **FR-9 (offline cold boot)**: A reload with the backend unreachable
  MUST boot to the party screen from last-known state, read-only, per
  E7's degraded-mode contract: the app persists the E7 store's
  `snapshotForBoot()` output (plus the roster bootstrap payload) in
  per-account browser storage, seeds the store from it on boot, and
  applies no reconciliation of its own — versions merge by E7's rules.
  E10 MUST request the browser persistent-storage grant at first run;
  the queue's storage keys and contents are unchanged from E7's.
- **FR-10 (no new semantics)**: E10 MUST NOT add sync, engine, import,
  auth, or write-path behavior. Its server surface is exactly FR-5's
  read. Anything else its screens show arrives through an existing seam
  (`derive`, the sync store, E6 components, E5's import page).

### Key Entities

- **Roster card** — the party screen's unit: E6 components (view-only
  `HpBar`, `CharacterHeader` line) + the shared chips strip, fed per
  character by the engine seam.
- **Party bootstrap payload** — FR-5's read: `{party_id, you, characters[]}`,
  each character the `me` shape; the drill-in's `SheetView` input.
- **Boot cache** — the per-account persisted pair (roster payload, wire
  snapshot) that makes FR-9's cold offline boot honest.
- **App shell cache** — the release-keyed SW precache of built static
  assets.
- **Manifest + icons** — static PWA install artifacts at the app root.

### Constraints (settled by the epic — non-negotiable)

- Standard service worker + manifest, hand-rolled, no PWA framework
  (EPICS constraint; Constitution Article V's boring stack).
- Desktop layout only at POC; phone/tablet layout is P2 and not begun
  here (PRD FG6 ruling).
- Reuse E6's sheet components for drill-ins — do not re-implement
  (EPICS guardrail; E6 `components.md` is the contract).
- Service-worker cache versioning/invalidation strategy explicit
  (EPICS guardrail) — FR-8 + design §SW.
- Effect chips render from the same engine state as sheet chips
  (EPICS guardrail) — FR-2.
- E7's contracts are binding inputs: degraded-mode §6 ("what E10's SW may
  assume and must not do") and wire-protocol frame semantics are consumed,
  not amended. The SW MUST NOT invent wire frames, run reconciliation, or
  surface errors the page hasn't surfaced.
- E3's authz is the write gate (server-enforced GM read-only); the UI
  mirrors it — no client-side security claim.
- One party at POC; the schema and WS are already per-party (PRD FG2) —
  FR-5's resolution rule leans on that, it does not build multi-party UI.

### Success Criteria

- **SC-1**: A fresh party session renders the roster for all six account
  types (owner, non-owner player, GM); a scripted write from a second
  client moves HP and chips on the roster without reload, in CI-visible
  component tests and on the live two-client path.
- **SC-2**: For every roster card and its character's sheet, the chips
  and HP figures are equal for the same store state — asserted in tests
  against the same `derive` output both views consume.
- **SC-3**: The GM seat's party screen and all drill-ins contain zero
  edit affordances (automated: no interactive edit controls render), and
  a forged GM write is rejected + audited server-side (E3's existing
  tests; E10 asserts the UI layer never offers the affordance).
- **SC-4**: `just ci-local` passes including the new production-path
  tests (roster read through the router; roster/drill-in/empty/GM states
  through the component harness; SW strategy functions as pure units).
- **SC-5**: The built app passes a manual install + offline-reload
  checklist (documented in the PR): installable on desktop Chrome/Edge,
  standalone window, offline reload renders last-known roster read-only,
  second build discards the first release's cache. Artifacts: manifest +
  icons served, SW registers in the built bundle only.

### Assumptions

- E6, E7, E8 are landed as their specs describe (all merged; the seams
  this spec cites — `SheetView(character, accountSub, editable)`,
  `derive(sync, id)`, `snapshotForBoot()`, EngineOutput `effects` +
  `render_base.hp_max` — exist on `main` today).
- POC deployment is one party; FR-5's characterless-caller resolution is
  exact for that world and no UI exposes party choice.
- Placeholder icon art is acceptable for POC (flagged for Josh/Dave to
  swap as static assets); no designed brand art exists in-repo.
- Desktop install (Chrome/Edge) is the verified install path; iOS/Android
  install "wherever the browser allows" rides the same standard manifest
  and is not separately verified at POC.
- The dev-server workflow (vite on :5173) stays SW-free: registration is
  build-only, so day-to-day development is unchanged.

### Out of scope (guarded)

- Phone/tablet layout (P2, PRD FG6) and any responsive rework beyond
  what the desktop layout already does.
- GM stat density — full stat-block cards, key skills, initiative
  modifier (E14, P1).
- Stash/party bank surfaces and any inventory editing from the roster
  (E12; the roster itself edits nothing).
- Any combat/round/initiative tracking, auto-expiry, aura/positioning —
  Constitution Article I non-goals; "down state" is a data rendering, not
  a tracker.
- Seeded spell library, outcome tappers, conflict pre-warn (E13).
- Curation editing or tooltip prose (E9/E15) — the roster shows chips'
  names/sources only, as the sheet already does.
- Client-side routing/deep links, multi-party UI, push notifications,
  background sync APIs — none are on the P0 line.
- Any change to E7's queue keys, wire frames, or reconciliation — the SW
  consumes the contract, it does not extend it.

## Clarify Log (settled from the existing corpus, 2026-10-08)

No question required Josh — every open point resolved against documents
already in the repo:

1. **Where do players land — sheet or party screen?** PRD *Entry Point*:
  login → lands on the party screen (adopted design); the player's sheet
  is one tap through their own card. The GM ruling (FG5) agrees. Folded
  into FR-1/US1.
2. **Does a characterless player in a non-empty party get the CTA?** PRD:
  the empty-party case is the CTA's designed home; the non-empty-party
  characterless player keeps an import affordance (US3.3) — reasonable
  default, folded into FR-6. GM never (FR-4).
3. **Who may read another character's sheet, and who enforces it?** E3
  authz (landed): `Read` → Allow for any authenticated actor; writes
  owner-only, GM none — per message on the WS and per request on REST.
  E10 renders `editable` accordingly and adds no enforcement (FR-3/FR-4).
4. **Where does the roster's live data come from?** E7's WS snapshot
  already spans every member's fields and the `derived` array; E8's
  EngineOutput carries `effects` (chips) and `render_base.hp_max`. Cards
  consume the same store + seam as the sheet (FR-1/FR-2). The one gap is
  the per-character base-sheet read — FR-5's new roster bootstrap.
5. **How does the GM (who owns no character) resolve a party?** POC has
  exactly one party; FR-5's resolution rule states it. Multi-party is
  config, not UI (PRD FG2).
6. **SW update behavior — auto or prompted?** Guardrail says stale shells
  are silent failure; the queue survives reloads (E7 contract §3), so
  auto-update on release is safe and is the design's choice, with the
  prompted alternative recorded as rejected (design §SW).
7. **Queue durability upgrade — migrate to SW storage?** E7 contract §3
  names persistent grant as the sanctioned upgrade; keeping the queue in
  place (same keys) and requesting `navigator.storage.persist()` is the
  minimal honest realization — no migration, folded into FR-9.
8. **Icon art?** No brand art exists in-repo; placeholder monogram in the
  design language, flagged swappable (FR-7). Not worth an owner decision
  at POC.
