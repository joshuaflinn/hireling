# Party View, GM Seat, PWA (E10) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the party screen (roster cards, GM seat, cross-member drill-in, first-run states) and the installable PWA shell (manifest, icons, release-versioned service worker, offline cold boot).

**Architecture:** Composition epic — one new server read (`GET /api/party/roster`), one shell-owned sync session shared by roster and drill-ins, roster cards composed from E6's components over the E8 engine seam, and a build-plugin-emitted service worker whose cache is keyed to the app release. No second sync path, no client engine, no new write surface.

**Tech Stack:** Rust/axum + sqlx (server), Svelte 5 runes + hand-rolled CSS (web), vite build with a local no-dependency plugin emitting `sw.js`, vitest + @testing-library/svelte (web tests), cargo integration tests through the real router.

**Spec:** `specs/009-party-view-gm-seat-pwa/spec.md` (requirements) · `design.md` (decisions D1–D7) · `contracts/roster-rest.md` + `contracts/sw-shell-cache.md` + `data-model.md`. The plan argues from these; executors read spec + design + this plan.

**Lane/heat proposal (for the sizing read, not binding on executors):** Tasks 1–7 are one lane (they share `App.svelte`, the session module, and the sheet seam — two lanes would spend more time merging than building). Tasks 8–10 (PWA shell) touch only `web/plugins/`, `web/public/`, `web/src/lib/pwa/`, `main.js`, `index.html` and can run as a second lane in parallel with 5–7. Task 11 is the convergence gate.

## Global Constraints

- Branch: `feat/12-party-view-gm-seat-pwa` (one issue, one branch, one PR; never `main`). Claim gh#12 on GitHub before the first code commit.
- Every commit message ends with `Co-Authored-By: Paperclip <noreply@paperclip.ing>` — exactly that line, no agent names.
- **No new dependencies** (Constitution Art. V). No PWA framework, no router library, no icon generator package.
- Reuse, don't re-implement: E6 components (`components.md` is the contract), `derive()` from `lib/engine/index.js`, E5's `ImportPage`, E6's `EmptyState`/`ErrorState`. Chips render from `view.effects` — same component (`EffectsStrip`), same source.
- E7's contracts are binding: the SW never intercepts `/api/*`, never reconciles, never surfaces errors; queue keys and contents unchanged.
- Components never touch the socket; state modules over `createSync` do (`sheet/state.js` pattern; E10's is `party/state.js`).
- Tests drive production paths (AGENTS.md): router tests through the real router; component tests through `@testing-library/svelte` rendering non-default states; imported-field rules proven with two fixtures; every exported helper has a named caller.
- `just ci-local` green before the PR; paste the output in the PR body along with the reachability sweep.
- Desktop layout only; no responsive work, no phone layout (P2).

---

### Task 1: The roster read — `GET /api/party/roster` through the real router

**Files:**
- Create: `src/party/mod.rs` (module doc, resolution, roster assembly, handler)
- Create: `src/pbimport/payload.rs` (extracted per-character payload assembly)
- Modify: `src/pbimport/handlers.rs` (`load_me` becomes owner-lookup + `character_payload`)
- Modify: `src/pbimport/mod.rs`, `src/lib.rs` or `src/main.rs` (module declarations, wherever siblings are declared)
- Modify: `src/http.rs` (route registration + `API_ROUTES` entry)
- Create: `src/tests/party.rs` (register the test module wherever `engine_rest.rs` is registered)

**Interfaces:**
- Consumes: E3 `SessionAccount` middleware, `authz::Role`; E2 tables `parties`, `characters`, `character_vitals`, `character_spell_slots`, `character_inventory_live`; `load_me`'s existing queries (moved, not rewritten).
- Produces: `pub async fn character_payload(pool: &sqlx::PgPool, character_id: i64) -> Result<serde_json::Value, sqlx::Error>` — the `me`-shape payload; `pub async fn roster_read(State(auth), SessionAccount) -> Response` (the handler, 200/401/409/500 per `contracts/roster-rest.md`).

- [ ] **Step 1: Write the failing router tests** (`src/tests/party.rs`). Mirror `engine_rest.rs`'s `get()` helper verbatim. Seed with `crate::testing::{seed_account, seed_session}` and the character-insert helper pattern from `src/tests/sync/helpers.rs` (`INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) VALUES ($1,$2,'{}',$3)` + a `character_vitals` row; check `helpers.rs` for the exact insert it already offers and reuse it if exported).

```rust
//! E10 roster read (plan Task 1): one party-scoped bootstrap through the
//! real router. Reads only; member and GM both see the whole party;
//! outsiders never reach the handler (E3 middleware).

use serde_json::Value;

// (get() helper copied from engine_rest.rs — same session-cookie one-shot)

#[tokio::test]
async fn member_reads_the_whole_roster_through_the_router() {
    let (app, pool) = test_router().await;
    // seed: party 1 (the seeded POC party), josh owns character A, becky owns B
    let session = testing::seed_session(&pool, "dev-sub-josh", now()).await;
    let (status, body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["party_id"], json!(1));
    assert_eq!(body["you"]["role"], json!("player"));
    let chars = body["characters"].as_array().unwrap();
    assert_eq!(chars.len(), 2); // A and B — josh sees becky's too (reads ungated)
    assert_eq!(chars[0]["owner"], json!("dev-sub-josh"));
}

#[tokio::test]
async fn gm_reads_the_roster_without_owning_a_character() {
    // seed GM account (role "gm"); roster 200, you.role == "gm"
}

#[tokio::test]
async fn unauthenticated_get_is_401_before_the_handler() {}

#[tokio::test]
async fn roster_payload_equals_the_me_payload_for_the_same_character() {
    // GET /api/party/roster and GET /api/characters/me as the owner;
    // assert the roster element for that character == the me body, field for field
}

#[tokio::test]
async fn two_imports_two_names_two_summaries() {
    // two fixtures differing only in base_sheet.identity.name:
    // distinct summary.name and distinct portrait source (the name is base data)
}

#[tokio::test]
async fn zero_parties_is_an_honest_409() {
    // DELETE FROM parties (and dependents) in this test's schema; expect 409
}
```

- [ ] **Step 2: Run them and verify they fail**

Run: `cargo test party` — Expected: FAIL (no route → 404/401 paths, module missing).

- [ ] **Step 3: Extract `character_payload`** — move the assembly body of `load_me` (summary build + vitals + slots + inventory queries, verbatim, they are already character-keyed) into `src/pbimport/payload.rs` with the signature above. `load_me` keeps only the owner-sub lookup then delegates. No behavior change.

Run: `cargo test pbimport` and `cargo test` — Expected: PASS (the move is pinned by existing tests).

- [ ] **Step 4: Implement `src/party/mod.rs`**

```rust
//! The party roster read (E10 design D1): every party character's
//! `me`-shape payload, party-scoped, for the session's resolved party.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{Value, json};
use sqlx::{PgPool, Row as _};
use std::sync::Arc;

use crate::auth::AuthState;
use crate::auth::middleware::SessionAccount;
use crate::pbimport::payload::character_payload;

/// Resolution rule (data-model §2): own character's party; else the single
/// POC party; else an honest 409 — never a guess.
async fn resolve_party(pool: &PgPool, caller_sub: &str) -> Result<i64, StatusCode> {
    let own: Option<i64> =
        sqlx::query_scalar("SELECT party_id FROM characters WHERE owner_sub = $1")
            .bind(caller_sub)
            .fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(party_id) = own {
        return Ok(party_id);
    }
    let parties: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM parties ORDER BY id")
            .fetch_all(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match parties[..] {
        [only] => Ok(only),
        [] => Err(StatusCode::CONFLICT),
        _ => Err(StatusCode::CONFLICT),
    }
}

/// `GET /api/party/roster` — see contracts/roster-rest.md.
pub async fn roster_read(
    State(auth): State<Arc<AuthState>>,
    account: SessionAccount,
) -> Response {
    let pool = &auth.pool;
    let party_id = match resolve_party(pool, &account.sub).await {
        Ok(id) => id,
        Err(status) => return (status, "").into_response(),
    };
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM characters WHERE party_id = $1 ORDER BY id")
            .bind(party_id).fetch_all(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR).into_response_on_err(); // keep the house error style: log + 500, no body
    let mut characters = Vec::with_capacity(ids.len());
    for id in ids {
        match character_payload(pool, id).await {
            Ok(payload) => characters.push(payload),
            Err(error) => {
                tracing::error!(error = %error, party_id, "roster payload failed");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    tracing::debug!(party_id, count = characters.len(), sub = %account.sub, "roster read");
    (StatusCode::OK, Json(json!({
        "party_id": party_id,
        "you": { "sub": account.sub, "role": account.role.as_str() },
        "characters": characters,
    }))).into_response()
}
```

(If the codebase has a cleaner `?`-friendly error path for handlers than the pseudo `into_response_on_err` above, follow the existing handler idiom — the contract to keep is: log + 500, no body.)

- [ ] **Step 5: Register route + table entry** — in `http.rs`'s protected nest: `.route("/party/roster", get(crate::party::roster_read))`, and add to `API_ROUTES`: `{ method: "GET", path: "/api/party/roster", writes: false }` (match the struct's exact field names — the router-test suite iterates this table).

- [ ] **Step 6: Run the full task's tests**

Run: `cargo test party && cargo test http` — Expected: PASS, including the table-probe tests that now expect the new route.

- [ ] **Step 7: Commit** — `feat(party): roster bootstrap read — every party character's me-payload through the router`

---

### Task 2: `bootSnapshot` — `createSync` seeds from the persisted wire snapshot

**Files:**
- Modify: `web/src/lib/sync/index.js` (one new option; merge through the existing snapshot path)
- Create: `web/tests/sync/boot.test.js`

**Interfaces:**
- Consumes: the existing `snapshot`-frame merge (the `frame.t === 'snapshot'` branch in `createSync`) and `SnapshotField` wire shape `{field, value, version}`.
- Produces: `createSync({..., bootSnapshot})` where `bootSnapshot` is `{ fields: SnapshotField[] }` (parsed). Later tasks (`party/session.js`) pass the boot cache's snapshot; the merge is strictly-newer, identical to a live snapshot — no new semantics.

- [ ] **Step 1: Failing test**

```js
import { test } from 'vitest';
import assert from 'node:assert/strict';
import { createSync, partySocketUrl } from '../../src/lib/sync/index.js';
import { fakeSocketFactory, openSocket } from '../harness/sync-fakes.js'; // reuse the existing fakes suite if present; else minimal inline fake

const hpField = (character_id, value, version) => ({
  field: { kind: 'vitals', character_id, field: 'hp' }, value, version,
});

test('bootSnapshot seeds the store; versions still arbitrate afterwards', async () => {
  const sync = createSync({
    url: partySocketUrl(1), storage: fakeStorage(), accountSub: 'dev-sub-josh',
    socketFactory: fakeSocketFactory(),
    bootSnapshot: { fields: [hpField(1, 10, 5)] },
  });
  assert.equal(sync.state().get(targetKeyOf(hpField(1)))?.version, 5);

  // a live snapshot with an OLDER hp version changes nothing (strictly-newer)
  deliverSnapshot({ fields: [hpField(1, 99, 4)] });
  assert.equal(hpOf(sync), 10);
  // a NEWER one wins — the seed is not sticky
  deliverSnapshot({ fields: [hpField(1, 12, 6)] });
  assert.equal(hpOf(sync), 12);
});

test('no bootSnapshot behaves exactly as before', () => { /* construct without the option; assert no throw, empty state */ });
```

(Adapt helper names to the real test fakes in `web/tests/` — the sync suite has them; grep `socketFactory` in `web/tests`.)

- [ ] **Step 2: Run** `npm --prefix web test -- tests/sync/boot` — Expected: FAIL (unknown option ignored).

- [ ] **Step 3: Implement** — extract the snapshot-frame merge body into a local `applySnapshotFields(fields)` used by both the `snapshot` frame branch and, once at construction, `options.bootSnapshot?.fields`. Zero other changes.

- [ ] **Step 4: Run the whole sync suite** `npm --prefix web test` — Expected: PASS (no regression; E7 tests untouched).

- [ ] **Step 5: Commit** — `feat(sync): createSync accepts a bootSnapshot seeded through the version merge`

---

### Task 3: Boot cache + persistent-storage grant (`party/boot.js`)

**Files:**
- Create: `web/src/lib/party/boot.js`
- Create: `web/tests/party/boot.test.js`

**Interfaces:**
- Consumes: `sync.snapshotForBoot()` (E7, shipped), the roster response shape (`data-model.md` §3).
- Produces:
  - `bootCacheKey(sub)` → `` `hireling:boot:${sub}` ``
  - `readBootCache(storage, sub)` → `{roster, snapshot, saved_at} | null`
  - `writeBootCache(storage, sub, roster, snapshotJson)` → void (writes `{roster, snapshot: JSON.parse(snapshotJson), saved_at: Date.now()}`; `saved_at` is diagnostics-only)
  - `createThrottledWriter(fn, ms)` → `{call(...args), flush()}` (trailing, timer-injectable via `setTimeout` global — tests use `vi.useFakeTimers()`)
  - `requestPersistentStorage(navigatorRef = navigator)` → `Promise<boolean>` (`storage.persist()`, logged, never throws; absent API → `false`)

- [ ] **Step 1: Failing tests** — fake `storage` (Map-backed), fake timers:
  - key isolation: `bootCacheKey('a') !== bootCacheKey('b')` (the queue's per-account pattern);
  - write→read roundtrip preserves roster + snapshot verbatim;
  - throttled writer coalesces N calls in 2 s into one write, and `flush()` writes pending immediately;
  - `requestPersistentStorage` returns `true` when `persist()` resolves true, `false` when the API is missing, and never rejects.

```js
test('the throttled writer coalesces bursts into one durable write', () => {
  vi.useFakeTimers();
  const storage = fakeStorage();
  const writer = createThrottledWriter(
    (roster) => writeBootCache(storage, 's', roster, '{"fields":[]}'), 2000);
  writer.call(rosterV1); writer.call(rosterV2); writer.call(rosterV3);
  vi.advanceTimersByTime(2100);
  assert.equal(readBootCache(storage, 's').roster, rosterV3); // last wins, one key write
});
```

- [ ] **Step 2: Run** — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** (~50 lines; no deps; `console.info` on grant outcome only).
- [ ] **Step 4: Run** `npm --prefix web test -- tests/party/boot` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(party): per-account boot cache with trailing throttle and persist() grant`

---

### Task 4: The session module — one roster fetch, one sync, boot wiring

**Files:**
- Create: `web/src/lib/party/session.js`
- Create: `web/tests/party/session.test.js`

**Interfaces:**
- Consumes: `createSync`, `partySocketUrl` (Task 2's `bootSnapshot`), Task 3's boot cache fns, `fetch`.
- Produces: `createPartySession({ fetchImpl = fetch, storage = localStorage, account })` → `Promise<{ roster, sync, refresh, offlineColdBoot }>`; throws `RosterUnavailable` (with `.cachedRoster` when a cache exists but the fetch failed — the caller renders it read-only). `refresh()` re-fetches the roster and rewrites the cache.

Behavior (no branching on *why* the link is down — E7's one-path rule): read cache → seed `bootSnapshot` from it (always, when present) → create the session sync with the roster's real `party_id` → fetch roster → on success, write cache + throttle-write `snapshotForBoot()` on every sync event → on failure, fall back to the cached roster with `offlineColdBoot: true`. Call `requestPersistentStorage()` once per account (guard with a storage flag `hireling:persist-asked:{sub}`).

- [ ] **Step 1: Failing tests** (mock `fetchImpl`, fake storage, fake sync socket as in Task 2):
  - online: returns roster, sync created with `partySocketUrl(roster.party_id)` (assert via injected `socketFactory` capturing the URL), boot cache written;
  - offline with cache: returns cached roster, `offlineColdBoot === true`, sync still created and seeded (assert `sync.state()` has a cached field);
  - offline without cache: throws `RosterUnavailable` with no `.cachedRoster`;
  - sync events trigger the throttled snapshot write (advance timers, assert cache snapshot updated);
  - persist() asked exactly once per sub across two sessions.

- [ ] **Step 2: Run** — Expected: FAIL. **Step 3: Implement** (~70 lines). **Step 4: Run** — PASS. **Step 5: Commit** — `feat(party): session module — one roster fetch, one socket, honest cold-boot fallback`

---

### Task 5: `party/state.js` + `RosterCard.svelte` — the card, composed not re-implemented

**Files:**
- Create: `web/src/lib/party/state.js`, `web/src/lib/party/RosterCard.svelte`
- Create: `web/tests/party/roster-card.test.js`

**Interfaces:**
- Consumes: the `Sync` public surface (`state()`, `subscribe()`, `derived()`), roster `CharacterPayload`s, E6 components (`CharacterHeader`, `HpBar`, `EffectsStrip`, `SyncIndicator`).
- Produces: `createRosterState({ sync, roster })` → `{ cards }`, one entry per character: `{ character, hp, tempHp, hpMax, effects, syncing }` — each a svelte store; card values seeded from `roster.vitals` then live from the store's versioned fields (read-only adapter; no write surface — writes belong to the sheet's state layer). `RosterCard` props: `{ card, editable = false, onopen }`.

`RosterCard` markup (whole component — this is the chip-truth guardrail made structural):

```svelte
<script>
  import CharacterHeader from '../sheet/components/CharacterHeader.svelte';
  import HpBar from '../sheet/components/HpBar.svelte';
  import EffectsStrip from '../sheet/components/EffectsStrip.svelte';
  import SyncIndicator from '../sheet/components/SyncIndicator.svelte';

  /** @type {{ card: any, editable?: boolean, onopen: () => void }} */
  let { card, editable = false, onopen } = $props();
  // down/full are renders of store state, not tracked anywhere (data-model §5)
  const down = $derived($card.hp.value === 0);
  const full = $derived($card.hpMax !== null && $card.hp.value === $card.hpMax);
  const initial = $derived((card.character.name?.[0] ?? '?').toUpperCase());
</script>

<button class="card" class:down class:full onclick={onopen}>
  <span class="avatar" aria-hidden="true">{initial}</span>
  <CharacterHeader name={card.character.name} level={card.character.level} editable={false} />
  {#if editable}<SyncIndicator syncing={$card.syncing} />{/if}
  <HpBar hp={$card.hp} temp={$card.tempHp} max={$card.hpMax} editable={false} />
  <EffectsStrip effects={$card.effects ?? []} />
</button>
```

(Verify `CharacterHeader`'s exact prop names against the component file before wiring — `components.md` is the contract; if its view-only header needs `subline`, pass the class string. The card's own `<button>` is navigation — allowed for the GM; edit affordances are what never render.)

- [ ] **Step 1: Failing tests** (render through `@testing-library/svelte`; **non-default states only**):
  - normal card: name, initial, HP text, chips with sources render from a seeded card store;
  - **down state**: hp 0 → `down` class present;
  - **full state**: hp === max → `full` class present;
  - **derived not yet delivered**: `hpMax` null, effects empty → skeleton bar, no placeholder number, no crash on missing name (`'?'` initial);
  - **two fixtures** `identity.name` `Flinn` vs `Becky` → initials `F` / `B` (imported-field rule);
  - chips equal the fixture's `view.effects` verbatim (name + `source_name`), `tracked` badge when `tracked_manually`;
  - clicking the card calls `onopen` (GM navigation is legal — the tap is not an edit).
- [ ] **Step 2: Run** `npm --prefix web test -- tests/party/roster-card` — FAIL. **Step 3: Implement** `state.js` (~50 lines, mirrors `sheet/state.js`'s read half over the public Sync API) + the component. **Step 4: Run** — PASS. **Step 5: Commit** — `feat(party): roster card — E6 units + engine chips over the sync store`

---

### Task 6: `SheetView` accepts the session sync (retire the product path's hardcoded party)

**Files:**
- Modify: `web/src/lib/sheet/SheetView.svelte`
- Create: `web/tests/sheet/sheet-view-sync.test.js`

**Interfaces:**
- Consumes: the `Sync` surface.
- Produces: `SheetView` prop `sync` (optional). Provided → used for `createSheetState` (the shell's single socket); absent → self-constructs exactly as today against the POC fallback party (standalone/tests). The product path (Task 7) always provides it.

- [ ] **Step 1: Failing test** — render `SheetView` with `base_sheet_reference.json`'s character payload and a **fake sync** whose `derived()` returns `engine_output_reference.json` (both fixtures already exist in `web/tests/data/`); assert a rendered number equals the fixture's EngineOutput value — proving the provided sync is the one consumed, not a self-built one. Assert no `sync` prop still renders (existing suites already pin this; one smoke here).
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement**: `let { character, accountSub = '', editable = true, sync: providedSync, onimport, onlogout } = $props();` then `const sync = providedSync ?? createSync({...})` with the fallback construction kept in an explicit `// standalone/test path — the shell always provides the session sync` comment.
- [ ] **Step 4: Run the whole suite** `npm --prefix web test` — PASS (E6 suites untouched prove back-compat). **Step 5: Commit** — `feat(sheet): SheetView consumes the session sync when provided`

---

### Task 7: `PartyView.svelte` + the app shell — party is home

**Files:**
- Create: `web/src/lib/party/PartyView.svelte`
- Modify: `web/src/App.svelte`, `web/src/app.css` (card grid + down/full styles, ~40 lines)
- Create: `web/tests/party/party-view.test.js`, `web/tests/party/app-flow.test.js`

**Interfaces:**
- Consumes: `createPartySession` (T4), `createRosterState` (T5), `SheetView{sync}` (T6), `ImportPage`, `EmptyState`, `ErrorState`.
- Produces: `PartyView` props `{ session, onimport, onopenCharacter(id) }`. `App` view states: `probing | entering | offline | party-loading | party | party-error | party-empty | import | sheet` — `sheet` carries `{payload, editable}`; back affordance returns to `party`. Editable rule (FR-3): `you.role === 'player' && payload.owner === you.sub`.

State machine (App.svelte, replacing the sheet-first boot):
`probe()` → `/api/me` ok → `createPartySession` → `party` (or `party-empty` when `characters.length === 0` — GM's empty roster shows the empty note, no CTA); roster fetch failure with cache → `party` + `offlineColdBoot` styling (read-only); failure without cache → `party-error` (`ErrorState` + retry). Player import affordance: in `PartyView`'s header when `you.role === 'player'` and no own character; empty party → `EmptyState` for players, empty note for GM.

- [ ] **Step 1: Failing tests:**
  - **player, non-empty party**: one card per character, in roster order; tapping card A calls `onopenCharacter` with A's id; own-card presence of `SyncIndicator` wiring (assert via `syncing` store flip);
  - **GM**: every card renders and is tappable, zero edit affordances (`queryByRole('button', { name: /damage|heal|import|new day/i })` → null everywhere; no "Import your character" text);
  - **empty party, player**: `EmptyState` CTA renders; **empty party, GM**: empty note, no CTA (the two-fixture rule applied to roles);
  - **characterless player, non-empty**: header import affordance renders; owner's doesn't;
  - **drill-in**: opening own card → `SheetView` `editable=true` (an edit control present); opening another member's → `editable=false` (zero edit buttons in the sheet body — reuse E6's view-only audit assertions); GM opens any → view-only;
  - **chip truth (SC-2)**: seed one fake sync state for character A; render the card **and** the sheet from the same state; assert the chip label sets are equal;
  - **offline cold boot**: session fixture with `offlineColdBoot: true` → roster renders from cached data read-only (no error modal; `ErrorState` absent);
  - **import flow**: CTA routes to the import view; success → `refresh()` re-fetches and the new card renders.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** `PartyView` (~80 lines) + App rewiring (~120; the existing probe/entry skeleton is kept — `entryAction` untouched). **Step 4: Run** `npm --prefix web test` — PASS. **Step 5: Commit** — `feat(party): the party screen is home — roster, GM seat, drill-in, first-run states`

---

### Task 8: SW strategy — pure functions (`pwa/strategy.js`)

**Files:**
- Create: `web/src/lib/pwa/strategy.js`
- Create: `web/tests/pwa/strategy.test.js`

**Interfaces:**
- Consumes: nothing (pure).
- Produces (consumed by Task 9's emitted `sw.js` and by tests — the named callers):
  - `classifyRequest(pathname, mode)` → `'asset' | 'navigation' | 'bypass'` (`/assets/*`, icons, manifest → asset; `mode === 'navigate'` → navigation; everything else — all `/api/*` — bypass)
  - `shellCacheName(buildId)` → `` `hireling-shell-${buildId}` ``
  - `precacheList(files)` → the files to precache (index.html, `/assets/*`, manifest, icons)
  - `obsoleteCaches(cacheNames, buildId)` → every `hireling-shell-*` name that isn't the current build's

- [ ] **Step 1: Failing tests**: `/api/party/roster` and `/api/ws/party/1` → bypass for every mode; `/assets/index-a1b2c3.js` → asset; `/icon-192.png`, `/manifest.webmanifest` → asset; `('/', 'navigate')` → navigation; `('/api/me', 'navigate')` → bypass (API never cached, even navigations can't be API — assert the rule order); obsolete list keeps current, drops others, ignores foreign caches.
- [ ] **Step 2: FAIL. Step 3: Implement** (~35 lines). **Step 4: PASS. Step 5: Commit** — `feat(pwa): fetch-strategy and cache-naming rules as pure functions`

---

### Task 9: The SW build plugin + emitted worker

**Files:**
- Create: `web/plugins/sw-plugin.mjs` (exports `composeSw({ buildId, precache, strategySource })` pure + the vite plugin)
- Create: `web/tests/pwa/sw-plugin.test.js`
- Modify: `web/vite.config.js` (add the plugin — local import, no dependency)

**Interfaces:**
- Consumes: Task 8's strategy source (read from `web/src/lib/pwa/strategy.js` at build time and inlined — the emitted worker is self-contained at scope `/`).
- Produces: at `closeBundle`, writes `dist/sw.js` containing: the inlined strategy functions, `const BUILD_ID = '<hash>'`, `const PRECACHE = [...]` (final hashed filenames from `dist/`), and the event wiring: `install` → precache atomically → `skipWaiting()`; `activate` → delete `obsoleteCaches` → `clients.claim()`; `fetch` → `classifyRequest`: asset → cache-first; navigation → network-first with cached `index.html` fallback; bypass → return (never `respondWith`). The page-side reload-once policy lives in registration (Task 10), not the worker.

Worker body (the whole shell over tested logic — keep it this thin):

```js
self.addEventListener('install', (event) => {
  event.waitUntil(precacheAll().then(() => self.skipWaiting()));
});
self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(obsoleteCaches(names, BUILD_ID).map((n) => caches.delete(n)));
    await self.clients.claim();
  })());
});
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  const kind = classifyRequest(url.pathname, event.request.mode);
  if (kind === 'bypass') return;                       // /api/* and WS: one path, the app's
  if (kind === 'asset') { event.respondWith(cacheFirst(event.request)); return; }
  event.respondWith(networkFirstShell(event.request)); // navigation
});
```

- [ ] **Step 1: Failing tests** (pure `composeSw` against a fake file list): output contains every passed hashed filename and the build id; output contains `classifyRequest`, `shellCacheName`, `obsoleteCaches` (the strategy is inlined, not linked); output does **not** contain `respondWith` on any bypass path (string-level check that the fetch handler returns early for `/api/`); plugin smoke: `closeBundle` over a temp dist directory writes `sw.js` and nothing else changes.
- [ ] **Step 2: FAIL. Step 3: Implement** (`composeSw` ~40 lines; plugin ~40; `crypto.createHash` for BUILD_ID over the sorted file list). **Step 4: PASS. Step 5: Commit** — `feat(pwa): vite plugin emits a release-versioned sw.js over the strategy module`

---

### Task 10: Manifest, icons, registration — installability

**Files:**
- Create: `web/public/manifest.webmanifest`, `web/public/icon-192.png`, `web/public/icon-512.png`, `web/public/icon-maskable-512.png`
- Create: `web/scripts/gen-icons.mjs` (deterministic, dependency-free PNG writer — `node:zlib` deflate + CRC; committed so the icons are regenerable)
- Create: `web/src/lib/pwa/register.js`
- Modify: `web/src/main.js` (call `registerPwa()`), `web/index.html` (manifest + icon links, `theme-color`, `background-color`)

**Interfaces:**
- Consumes: Task 8/9 artifacts.
- Produces: `registerPwa(env = import.meta.env, reg = navigator.serviceWorker)` — registers `/sw.js` (scope `/`) only when `env.PROD`; on `reg.addEventListener('controllerchange')` reloads once iff a controller existed at register time (the update policy's page half). Icons are a placeholder monogram on the panel background with the gold accent (CSS variables' values) — flagged swappable.

```json
{
  "name": "Hireling",
  "short_name": "Hireling",
  "start_url": "/",
  "scope": "/",
  "display": "standalone",
  "background_color": "#10141b",
  "theme_color": "#10141b",
  "description": "Party-linked Pathfinder 2e character tracker",
  "icons": [
    { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png" },
    { "src": "/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
  ]
}
```

(Match the exact panel/gold hex values from `web/src/app.css` when writing files — take them from `:root`, not from memory.)

- [ ] **Step 1: Failing tests** (`web/tests/pwa/install.test.js`):
  - `registerPwa({ PROD: true }, fakeReg)` registers `'/sw.js'` with `{ scope: '/' }`; `{ PROD: false }` never touches `fakeReg` (the dev-server stays SW-free — two-fixture rule on env);
  - controllerchange with no prior controller → no reload; with prior controller → exactly one (fake `location.reload`);
  - statics contract (fs, node-side): manifest parses; required fields present (`display: "standalone"`, both icon sizes + maskable); each icon file exists, is non-empty, and starts with the PNG magic bytes `\x89PNG`; `index.html` links the manifest and a `theme-color`.
- [ ] **Step 2: FAIL. Step 3: Implement**: `gen-icons.mjs` (run once, commit outputs; script stays for regeneration), manifest, register.js, index.html/main.js wiring. **Step 4: PASS (`npm --prefix web test`), plus `npm --prefix web run build` → assert `dist/sw.js`, `dist/manifest.webmanifest`, icons exist and `dist/index.html` references them. Step 5: Commit** — `feat(pwa): manifest, placeholder icons, PROD-gated registration with reload-once update`

---

### Task 11: Gate, sweep, PR

**Files:** none new (evidence only).

- [ ] **Step 1: Full local gate** — `just ci-local` (json-keys, fmt-check, clippy-deny, tests, cargo-deny, boundary, web-check, web-test, web-build). Paste the tail in the PR body. Fix anything red before proceeding; a red gate is a red PR.
- [ ] **Step 2: Reachability sweep** (AGENTS.md): walk spec FR-1..FR-10 and name each one's production-path test (the suites above); grep callers for every new export — `character_payload` (2 callers: `me` + roster), `createPartySession`/`createRosterState` (App/PartyView), strategy functions (emitted sw + tests), `registerPwa` (main.js), boot-cache fns (session.js). Hits confined to the defining module = unbuilt; fix before the PR.
- [ ] **Step 3: SC-5 manual checklist** (documented honestly in the PR body, not claimed): `npm --prefix web run build && cargo run` → desktop Chrome/Edge: install works, standalone window opens, offline reload (stop the server, reload) renders last-known roster read-only, second build (`touch web/src/App.svelte && rebuild`) serves the new shell and `caches.keys()` shows one `hireling-shell-*`.
- [ ] **Step 4: Open the PR** — branch `feat/12-party-view-gm-seat-pwa`, body carries: `Implements gh#12 (Refs #12 — leave the epic open until review)`, link to the Paperclip task, gate output, sweep table, manual checklist results, and the decisions-with-rejected-alternatives summary (D1–D7 + the SW auto-update ruling). Merge ask goes to Thrane; nobody merges their own PR.

---

## Self-review

**Spec coverage:** FR-1→T5/T7 · FR-2→T5 (chip tests) · FR-3→T6/T7 (editable rule) · FR-4→T7 (GM tests) · FR-5→T1 · FR-6→T7 (state tests) · FR-7→T10 · FR-8→T8/T9 · FR-9→T2/T3/T4/T7 · FR-10→whole plan (no other server work exists). SC-1..SC-4 → the named suites; SC-5 → T11 Step 3. Edge cases from the spec map to test steps (null-derived → T5; portrait initial fallback → T5; offline cold boot → T4/T7; SW update mid-session → T9/T10).

**Placeholder scan:** task bodies above carry real code, real commands, real fixtures (including the two pre-existing reference JSONs); the two "mirror the existing idiom" notes (router error path, sync test fakes) point at named files in-tree, not at unwritten code.

**Type consistency:** `character_payload(pool, i64) -> Value` (T1) is the same name T1's tests and handler use; `bootSnapshot {fields}` shape is identical in T2/T3/T4; `cards` store fields (`hp`, `tempHp`, `hpMax`, `effects`, `syncing`) match `RosterCard`'s props; `classifyRequest/shellCacheName/obsoleteCaches` names are shared by T8/T9.
