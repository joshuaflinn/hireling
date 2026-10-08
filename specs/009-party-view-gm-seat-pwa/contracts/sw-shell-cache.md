# Contract: Service-Worker Shell Cache (E10)

**Status**: authored 2026-10-08 (design D5/D6); builds strictly on
[`specs/007-party-sync/contracts/degraded-mode.md`](../../007-party-sync/contracts/degraded-mode.md) §6
("what E10's service worker may assume and must not do") — that document is
binding input; this one is E10's half of the bargain. One stated deviation
is recorded in §5.

## 1. Ownership line

The SW owns **the shell** — built static assets. The app owns **the data** —
roster payloads and store state in per-account localStorage (the boot cache,
`data-model.md` §3). Neither crosses into the other's territory. The SW
never sees a wire frame, a version number, or an account.

## 2. Cache naming and precache

- Cache name: `hireling-shell-{BUILD_ID}`. `BUILD_ID` is a content hash of
  the built bundle, computed and injected at build time by
  `web/plugins/sw-plugin.mjs` (which also emits the asset list) — never
  hand-maintained.
- Precache (installed atomically before activation): `index.html`, every
  emitted `/assets/*`, `manifest.webmanifest`, `icon-192.png`,
  `icon-512.png`, `icon-maskable-512.png`.
- Nothing else is ever cached: no `/api/*` response, no WS traffic, no data.

## 3. Fetch strategy

| request class | rule | rationale |
|---|---|---|
| `/assets/*`, icons, manifest | **cache-first** | content-hashed / static ⇒ immutable |
| navigation (`request.mode === 'navigate'`) | **network-first**, fallback cached `index.html` | online users always get the fresh shell; offline boots get the cached one |
| everything else (`/api/*` included) | **not intercepted** | live data has one path: the app + E7's contract. The SW adds no offline API theatre |

Classification and cache-name math live in `web/src/lib/pwa/strategy.js` as
pure functions; the emitted `sw.js` imports them. Testable without a SW
runtime; the emitted worker is a thin shell over tested logic.

## 4. Versioning, invalidation, update policy (the guardrail, explicit)

- **Versioning**: by release — `BUILD_ID` in the cache name. Not wall clock,
  not TTL, not "check sometimes".
- **Invalidation**: on activation, `caches.keys()` → delete every cache not
  matching the current `BUILD_ID`. One release ⇒ one cache, always.
- **Update flow**: new SW → `install` (precache new release) →
  `skipWaiting()` → `activate` (delete old caches, `clients.claim()`) → the
  page reloads **once** on `controllerchange` iff a controller existed
  before (first install never reloads). Reload safety is E7's guarantee:
  the write queue and boot cache are reload-durable.
- **Rejected alternative** (recorded per house rule): a "new version
  available" prompt. Six trusted users, a queue that survives reload, and a
  guardrail that names stale shells as silent product failure — the prompt
  trades a real failure mode for ceremony.

## 5. Relation to degraded-mode §6, including one deviation

Honored verbatim: the SW serves the shell for cold offline boot; it invents
no wire frames; it runs no reconciliation or version math; it surfaces no
errors the page hasn't surfaced; `localStorage` queue contents are treated
as first-run input (here: untouched keys under a `persist()` grant — no
migration, which is the minimal honest treatment).

**Deviation**: §6 suggests the SW caches the store snapshot "versioned,
invalidated by app release". This design keeps the snapshot in app-owned
localStorage under the persistent-storage grant (design D6): the snapshot is
per-account data the page must version-merge anyway, and a SW-held second
copy adds a copy to invalidate without adding safety — stale snapshots are
made safe by the merge rule, not by release invalidation. The contract's
intent (durable, honest cold-boot state) is met. If the reviewer rules for
the letter of §6, the change is localized to `web/src/lib/party/boot.js`.

## 6. Registration and dev behavior

- `web/src/lib/pwa/register.js` registers `sw.js` at scope `/` **only** in
  production builds (`import.meta.env.PROD`); the vite dev server stays
  SW-free so day-to-day development is unchanged.
- Registration failure is logged and non-blocking — the app remains a
  plain web page; installability is an enhancement, never a gate.
- The manifest (`web/public/manifest.webmanifest`): name "Hireling", short
  name, `start_url: "/"`, `display: "standalone"`, theme/background colors
  from the app's CSS variables, icons 192/512 + maskable. Placeholder
  monogram art, swappable as static files without code change (spec FR-7).
