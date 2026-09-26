# E3 Design: Authentik OIDC Auth & Ownership Enforcement

**Status**: Draft for human review · **Spec**: `specs/003-authentik-oidc/spec.md` · **Contracts**: `contracts/oidc.md` · **Data model**: `data-model.md`

Decisions here implement the spec's Assumptions as settled. Where the spec left a
fork open, the approach chosen is the one presented and recommended at brainstorm;
rejected alternatives are noted inline with the why.

## 1. Architecture overview

Three structural guarantees, each enforced by construction rather than by
per-endpoint discipline:

1. **Authentication is mount-position deny-by-default.** Every route under `/api/**`
   is behind the `require_auth` layer unless deliberately mounted public. The public
   set is exactly: `/healthz`, the static frontend bundle, `/api/auth/login`,
   `/api/auth/callback`, `/api/auth/logout` (session-aware but not auth-requiring),
   and — in debug builds only — `/api/dev/session`.
2. **Authorization is un-forgettable at the type level.** Write handlers receive
   their target resource only through a resource-scoped axum extractor
   (`OwnedCharacter`, `AuthoredEffect`, …) that loads current ownership from the DB
   and rejects non-writers before the handler body runs. A handler that skips the
   guarded extractor has no resource to write.
3. **The GM-writes-nothing rule is one transport-layer check**, applied to every
   mutating method on the protected router, independent of any handler. It cannot be
   forgotten per-endpoint because no endpoint is named in it.

All verdict logic lives in a **pure `authz` module** (no I/O, no framework imports),
so the entire ownership matrix is testable as data. Shell modules (middleware,
extractors, OIDC legs) call into it.

Rejected alternatives:

- **Middleware-everywhere (path-convention authZ).** A middleware that parses
  `/api/characters/:id/...` to find the resource duplicates routing knowledge and
  fails *open* silently when a route shape changes. Rejected: it is the
  per-endpoint-memory failure inverted.
- **Per-handler `authz::require(...)` guard calls.** A missing guard compiles fine.
  Rejected: deny-by-default must be structural (spec FR-7).
- **Signed-cookie sessions + revocation list.** A revocation table is a session
  store with worse ergonomics; signed cookies cannot slide expiry without reissue
  on every request. Rejected in favor of a server-side store (spec FR-4 requires
  server-side invalidation; US6.3 requires a replayed post-logout cookie to fail).

## 2. OIDC flow

Confidential client, authorization-code flow with PKCE (S256). Contract details —
endpoint URLs, algorithms, claims — are in `contracts/oidc.md`, citing the probed
house-Authentik discovery fixture.

**No runtime discovery fetch.** The three endpoint URLs (authorize, token, JWKS)
are configured, not discovered at runtime: the contract is a captured fixture, and
fewer moving parts means the app boots and serves existing sessions even when the
IdP is down (spec edge case: provider unreachable).

1. `GET /api/auth/login` (public). Generate `state`, `nonce`, and a PKCE
   `code_verifier` (each 256-bit random). Store all three in a short-lived
   (10-minute) HMAC-signed `__Host-hireling_oidc` transaction cookie (signing key
   from `HIRELING_COOKIE_KEY`). 302 to the configured authorize endpoint with
   `response_type=code`, `scope=openid profile`, `client_id`,
   `redirect_uri`, `state`, `nonce`, `code_challenge` (S256),
   `code_challenge_method=S256`. The captured provider advertises both scopes
   and the full claim set (contracts/oidc.md §1, §4).
2. Authentik authenticates the user (house accounts; Hireling never renders a
   password field — FR-1) and redirects to `/api/auth/callback?code=…&state=…`.
3. `GET /api/auth/callback` (public).
   - Verify `state` against the transaction cookie; mismatch → 400, no session.
   - Exchange the code at the token endpoint, authenticating with
     `client_secret_basic` (per the probed
     `token_endpoint_auth_methods_supported`), sending the PKCE `code_verifier`.
   - Validate the ID token with `jsonwebtoken`: signature against the configured
     JWKS (cached in memory; fetched lazily on first login and refetched on
     unknown `kid`), **algorithm pinned to RS256** (the probed fixture supports
     RS256 only — pinning kills the alg-confusion class), `iss` equal to the
     configured issuer, `aud` equal to `client_id`, `exp` unexpired, `nonce`
     equal to the transaction cookie.
   - Map claims: `sub` → account key; `preferred_username` → username;
     `name` (falling back to `preferred_username`) → display name. (If a
     profile claim is ever absent, the seat name paired with that `sub` in the
     allowlist config is the fallback — identity (`sub`) never depends on
     profile claims.)
   - **Allowlist check**: `sub` must appear in `HIRELING_ALLOWLIST` (spec
     Assumption: the allowlist keys on `sub`). Denial → human-readable "not on
     the list" page, `login_allowlist_denied` audit record, **no session** (FR-2).
   - Upsert the `accounts` row (`role = gm` iff `sub == HIRELING_GM_SUB`,
     else `player`; GM designation is application config per spec Assumption).
   - Insert a `sessions` row, set the session cookie, 302 to `/`.
4. IdP unreachable or any exchange/validation failure → human-readable error
   page, audit/log entry, no session. There is no local credential fallback —
   there isn't one (spec edge case).

**Logout is local-only** (`POST /api/auth/logout`): delete the session row, clear
the cookie. The probed `end_session_endpoint` is deliberately not called — the
house IdP session is shared with other applications, and logging out of Hireling
must not log the user out of everything.

## 3. Session lifecycle

Server-side store in Postgres (`sessions` table, `data-model.md`). The cookie
`__Host-hireling_session` carries an opaque 256-bit random id (base64url),
`HttpOnly; Secure; SameSite=Lax; Path=/`. The cookie is a lookup key, not a token
— there is nothing to forge and nothing to verify offline.

- **Creation**: at callback success (above), and in debug builds via the
  dev-session route (§6).
- **Sliding renewal**: on each authenticated request, if `last_seen_at` is older
  than 5 minutes, update it and set `expires_at = now + idle_window`. The
  5-minute amortization avoids a write per request; expiry correctness is
  unaffected because the check reads `expires_at` regardless.
- **Bounded lifetime**: requests past `expires_at` (idle) or
  `absolute_expires_at` (hard cap) are rejected as unauthenticated. Defaults:
  idle window 24 h sliding, absolute cap 7 days (spec Assumption: sliding,
  hard-capped on the order of a week; both are env-tunable via
  `HIRELING_SESSION_IDLE_SECS` / `HIRELING_SESSION_ABSOLUTE_SECS`).
- **Invalidation (FR-4)**: logout deletes the row — a replayed cookie misses the
  lookup and is rejected as unauthenticated (US6.3). Administrative invalidation
  is the same row delete.
- **Allowlist withdrawal**: `require_auth` checks the session's account `sub`
  against the in-memory allowlist on every request; removing a `sub` from config
  denies that account on its next request with no session restart (spec edge
  case). Deprovisioning at the IdP is bounded by the absolute cap.
- **Response distinction (FR-5)**: expired/invalid/absent session →
  `401 {"error":{"code":"unauthenticated","message":"authentication required"}}`.
  Authorization failures → `403` with `code:"forbidden"`. The SPA routes 401 to
  re-login and reports 403 as a permission error.

Why Postgres and not in-memory: E11 deploys a stateless container where the
Asgard hall is the only persistent state, and restarts/deploys during game night
must not log the table out. At six users the per-request lookup is noise.

## 4. Middleware placement in the E1 stack

The E1 stack (`src/http.rs`) today, request path order: **SetRequestId →
PropagateRequestId → TraceLayer → routes** (`.layer()` calls wrap outward, so the
last call in code runs first on the request path).

Auth slots **innermost, after TraceLayer** — so the trace span and every auth log
line carry the request id — and **scoped to the `/api` nest**, so `/healthz` and
the static bundle stay dependency-free. Concretely, in `router()`:

```
Router::new()
    .route("/healthz", get(health))                 // public, untouched
    .nest("/api", api_router)                        // see below
    .fallback_service(frontend)                      // public static bundle
    .layer(TraceLayer …)                             // unchanged, outer
    .layer(PropagateRequestIdLayer …)                // unchanged
    .layer(SetRequestIdLayer …)                      // unchanged
```

`api_router` is built from a **declarative route list** (path, method, handler) —
the list is the single source the SC-1 matrix test iterates, so "zero untested
write endpoints" cannot drift. Layers on the nest:

```
api_router
    .layer(gm_read_only)        // innermost: mutating method + role gm → 403
    .layer(require_auth)        // 401 when no valid session or not allowlisted
    .layer(session_resolve)     // cookie → Option<SessionAccount> extension
```

Public auth legs (`login`, `callback`, `logout`, debug `dev/session`) are merged
into the `/api` nest **outside** the `require_auth`/`gm_read_only` wrap but inside
`session_resolve` (logout uses the session if one exists and is idempotent when
none does). Resulting request path for a protected endpoint:

**request-id → propagate → trace → session-resolve → require-auth → gm-read-only
→ extractor authZ → handler.**

- `session_resolve`: reads the session cookie, loads the row, attaches
  `Option<SessionAccount>` (sub, username, display name, role) as a request
  extension. Never rejects.
- `require_auth`: no session, expired session, or account no longer allowlisted →
  the standard 401 payload.
- `gm_read_only`: method ∈ {POST, PUT, PATCH, DELETE} and session role `gm` → the
  standard 403 payload plus a `forbidden_gm_write` audit record. This is the
  Article-I backstop: it names no endpoints and covers every write path that ever
  lands inside the nest.
- **Realtime (E7)**: the WebSocket upgrade endpoint mounts inside `require_auth`
  like any other route (unauthenticated connections refused). Write messages on
  an established socket are evaluated by the same pure `authz::authorize` before
  application — a GM socket reads and never writes (spec edge case).

## 5. Ownership-matrix enforcement model

Pure module `authz` (shell/pure split per toolkit conventions — no I/O, no axum
imports):

```
Actor   = { sub, role }                       // from the session, never the request
Resource = Character { owner_sub }
         | Effect    { creator_sub }
         | CustomRow { creator_sub }
Action  = Read | Write
authorize(actor, action, resource) -> Verdict::{Allow, Deny}
```

The function **is** the matrix, whole:

- `Read` → `Allow` for any authenticated, allowlisted actor (FR-13). Ownership
  gates writes; nothing gates reads.
- `Write` by role `gm` → `Deny` (FR-12), independent of resource.
- `Write` → `Allow` iff `actor.sub` equals the resource's sole-writer key:
  `owner_sub` for characters (FR-8), `creator_sub` for effects (FR-9) and custom
  rows (FR-11). Otherwise `Deny`.
- **Effect creation** (FR-10) is a precondition checked at the creation endpoint,
  not a `Resource` case: the actor must own at least one character (the caster is
  a character at the table). Any character owner may create effects targeting any
  roster characters.

Handlers never call `authorize` directly. Resource-scoped **extractors** do:
`OwnedCharacter(Path<…>)`, `AuthoredEffect(Path<…>)`, `AuthoredCustomRow(Path<…>)`
load the row's ownership fact from Postgres, call `authorize(actor, Write, …)`,
and yield the row — or short-circuit the standard 403. Reads use a plain
`SessionAccount` extractor. Because ownership is loaded fresh per request, an
out-of-band reassignment takes effect on the next request with no session action
(FR-16). Client-supplied identity in headers/params/body is never consulted for
the actor (FR-3); a request body naming another account is simply a non-owner
write and is rejected accordingly (spec edge case).

Standard payloads (FR-14), identical on every endpoint:

```
401 {"error":{"code":"unauthenticated","message":"authentication required"}}
403 {"error":{"code":"forbidden","message":"you do not have permission to do that"}}
```

No ownership internals beyond what any party member can already read; no stack
traces.

**E2 keying contract** (E2 is parallel; its accounts are minimal/identity-only):
`accounts.sub TEXT PRIMARY KEY` — the provider `sub` is the stable identifier the
allowlist and every ownership binding key on (spec Assumption). Ownership columns
(`characters.owner_sub`, effect/custom `creator_sub`) FK to `accounts.sub`. No
surrogate UUID: six accounts, spec-literal keying, one less indirection. This
contract won the cross-epic reconciliation at design review: **E2 implements
`accounts` to this keying** (E2 has been instructed to amend). E3 owns the
`sessions` and `audit_events` migrations; E2 owns `accounts` and all domain
tables. See `data-model.md` for the merge-order rule.

## 6. Dev-mode auth

Spec Assumption: a dev-only mechanism establishes sessions as any of the six
seats without a live IdP, and **its absence in prod is part of its definition**.
The gate is compile-time (decided by Josh at design review, superseding the
spec's "env-flagged" phrasing with the stronger mechanism): the entire module
lives behind `#[cfg(debug_assertions)]`. `cargo build --release` produces a
binary that physically lacks the route and the handler — no environment variable
can conjure it. In debug builds the route is unconditionally available so CI and
agent sessions get fake sessions with zero setup.

Mechanism: `POST /api/dev/session {"seat":"bear"}` maps the seat name to a stable
test `sub` (`dev-sub-<seat>` by default; overridable per seat in dev config),
upserts the account row, inserts a **real** `sessions` row, and sets the real
cookie. Only the Authentik round-trip is faked — session lifecycle, extractors,
the GM layer, expiry, logout, and audit all run the production path, which is
what makes the CI ownership-matrix suite a faithful test of production behavior.
The debug-build login page additionally offers a six-seat picker so manual dev
needs no curl. The `gm` seat maps to Bruce's test sub and must set role `gm` via
`HIRELING_GM_SUB` in the dev environment.

## 7. Audit logging

Append-only `audit_events` table (schema in `data-model.md`), insert-only, one row
per event — the record SC-8 verifies by query. A parallel
`tracing::info!(target: "audit", …)` line with identical fields supports log-side
diagnosis and carries the request id; the table is authoritative.

Events (FR-15): `login_success`, `login_allowlist_denied` (records the presented
`sub` and username), `logout`, `forbidden_character_write`,
`forbidden_effect_write`, `forbidden_custom_write`, `forbidden_gm_write`. Each row
carries actor sub (nullable — an allowlist-denied login has no account yet),
event, target, outcome, timestamp, and the request id for correlation with trace
lines. The `authorize` rejection path writes the `forbidden_*` rows from the
extractor layer, so a rejected write is recorded exactly once regardless of
endpoint.

## 8. Configuration

Following the existing `src/config.rs` pattern (`HIRELING_*` vars, pure
`_from_env` functions taking an `EnvLookup` closure, sibling test file). New
settings, grouped as an `AuthSettings` struct on `Settings`:

| Variable | Required | Purpose |
|---|---|---|
| `HIRELING_OIDC_ISSUER` | prod | Issuer base, e.g. `https://auth.flinntech.com/application/o/hireling/` — authorize/token/JWKS URLs derive from it per `contracts/oidc.md` |
| `HIRELING_OIDC_CLIENT_ID` | prod | Authentik client id |
| `HIRELING_OIDC_CLIENT_SECRET` | prod | Authentik client secret (1Password → deploy env) |
| `HIRELING_BASE_URL` | prod | Public origin, e.g. `https://hireling.flinntech.com`; redirect URI derives as `{base}/api/auth/callback` |
| `HIRELING_COOKIE_KEY` | prod | 64 hex chars; signs the OIDC transaction cookie |
| `HIRELING_ALLOWLIST` | yes | Comma-separated provider `sub`s (the six accounts) |
| `HIRELING_GM_SUB` | yes | The allowlisted `sub` holding the GM seat (Bruce) |
| `HIRELING_SESSION_IDLE_SECS` | no (default 86400) | Sliding idle window |
| `HIRELING_SESSION_ABSOLUTE_SECS` | no (default 604800) | Hard session cap (7 days) |

Startup validation: in release builds the OIDC/cookie settings must be present or
the process refuses to boot (fail closed, clear message). In debug builds without
them, the app boots with OIDC legs disabled and a logged pointer to the
compile-gated `/api/dev/session` route (§6).

## 9. Dependencies

Constitution Article V — each new dependency carries its written justification:

- **`reqwest`** (rustls, no openssl): token-endpoint exchange and JWKS fetch.
  OIDC requires an HTTP client; none exists in the tree.
- **`jsonwebtoken`**: RS256 ID-token signature validation against JWKS.
  Hand-rolled JWT signature verification is the classic crypto footgun; this is
  the one place we deliberately do not hand-roll.
- **`rand`**: CSPRNG for session ids, `state`, `nonce`, PKCE verifiers.
- **`cookie`** (feature `signed`): correct `Set-Cookie` attribute handling and
  the HMAC-signed transaction cookie.
- **`sha2` + `base64`**: PKCE S256 challenge and base64url encodings.
- **`sqlx`** (postgres + migrations): session and audit persistence. E2 already
  mandates sqlx; E3 consumes the same choice rather than adding a second data
  layer. Exact feature set and time type (`chrono` vs `time`) follow E2's lead —
  one coordination note, not a second decision.

Deliberately **not** taken: `openidconnect` (large tree for one confidential
client; the flow above is ~200 lines over `reqwest`+`jsonwebtoken`),
`tower-sessions` (+ SQLx store: more machinery than a six-row session table, and
its generic API obscures the lifecycle rules §3 makes explicit).

## 10. Testing strategy

Per toolkit conventions: pure modules get sibling-file unit tests
(`src/auth/tests/*.rs`); the HTTP shell is exercised by integration tests.

1. **`authz` pure module — the matrix as data.** Exhaustive table test: every
   role (owner, non-owner, effect creator, target's owner, third party, GM,
   unauthenticated) × every resource kind × every action. SC-1–SC-4 are rows in
   this table, not prose.
2. **Session store** (sqlx against the compose Postgres): create, sliding
   renewal, idle expiry, absolute expiry, logout invalidation, allowlist
   withdrawal on next request.
3. **OIDC seam without Authentik.** Transaction-cookie build/verify and claim
   mapping are pure and unit-tested. ID-token validation is tested against a
   test-generated RSA keypair served as a local JWKS fixture. A stub token
   endpoint (tiny in-test axum server) drives the full login → callback →
   session round-trip. The probed discovery document is checked in as a contract
   test fixture (`contracts/oidc.md` is its citation).
4. **HTTP integration** (tower `ServiceExt::oneshot`, dev-deps already present):
   the real router plus dev-auth sessions for all six seats. Every write endpoint
   exercised as every role; the 401/403 payload shapes asserted byte-identical
   across endpoints (SC-2, SC-3, SC-5). **Zero untested write endpoints**: the
   matrix test iterates the same declarative route list that builds the router
   (§4) and asserts every mutating entry appears in the matrix.
5. **Audit**: one row per event with actor/action/target/outcome — SC-8 verified
   by query in the integration suite.
6. **Prod-safety of dev auth**: a release-profile check asserts
   `/api/dev/session` is absent (404) when compiled out — run at release-gate
   time, since CI's debug build cannot observe it.
