# E3 Data Model: Auth & Ownership Entities

**Spec**: `specs/003-authentik-oidc/spec.md` · **Design**: `design.md`

E3 owns two tables (`sessions`, `audit_events`) and defines the keying contract
for a third (`accounts`, implemented by E2 — settled at cross-epic design review,
2026-09-23). Types below are logical; physical DDL lands in sqlx migrations at
implementation time.

## Entities

### Account (E2-owned; E3-defined keying contract)

One row per allowlisted human, upserted at first successful login. Minimal and
identity-only per E2's spec — everything domain (characters, effects, …) hangs
off this key.

| Field | Type | Rules |
|---|---|---|
| `sub` | TEXT **PRIMARY KEY** | Provider `sub` claim; non-empty. The stable identifier the allowlist and every ownership binding key on (spec Assumption). |
| `username` | TEXT NOT NULL | From `preferred_username`. |
| `display_name` | TEXT NOT NULL | From `name`, falling back to `preferred_username`. |
| `role` | TEXT NOT NULL | `CHECK (role IN ('player','gm'))`. Set at login: `gm` iff `sub = HIRELING_GM_SUB`, else `player`. |
| `created_at` / `updated_at` | timestamptz NOT NULL | E2 guardrail: timestamps on all entities. `updated_at` bumps on login upsert. |

**Relationships**: referenced by `sessions.account_sub`, by every ownership
binding (`characters.owner_sub`, effect/custom `creator_sub` — E2/E5/E8/E9
tables), and by `audit_events.actor_sub`.

**Upsert semantics (E3-defined)**: on each successful login, insert-or-update
`username`/`display_name`/`role` for the authenticated `sub`. An account row can
exist only for an allowlisted `sub` — the allowlist check precedes the upsert.

### Session (E3-owned)

A server-held binding between a browser and an account; the sole source of acting
identity (FR-3).

| Field | Type | Rules |
|---|---|---|
| `id` | TEXT **PRIMARY KEY** | 256-bit CSPRNG, base64url (43 chars). Opaque lookup key — never derived, never guessable. |
| `account_sub` | TEXT NOT NULL **FK → accounts.sub** | The bound account. |
| `created_at` | timestamptz NOT NULL | Login time. |
| `last_seen_at` | timestamptz NOT NULL | Last authenticated activity; written at most once per 5 minutes per session (write amortization, design §3). |
| `expires_at` | timestamptz NOT NULL | Idle deadline: `last_seen_at + HIRELING_SESSION_IDLE_SECS` (default 86400), extended on activity. Must be `> created_at`. |
| `absolute_expires_at` | timestamptz NOT NULL | Hard cap: `created_at + HIRELING_SESSION_ABSOLUTE_SECS` (default 604800 = 7 d). Never extended. |

No `updated_at`: `last_seen_at` carries that meaning.

**State transitions** (evaluated, not stored — there is no status column):

```
[none] --login / dev-session--> ACTIVE
ACTIVE --expires_at reached (idle)--> EXPIRED        (rejected as unauthenticated)
ACTIVE --absolute_expires_at reached--> EXPIRED      (activity does not rescue)
ACTIVE --logout / admin row delete--> INVALIDATED    (terminal: row gone)
```

- EXPIRED sessions may be reaped by row delete at any time; a replayed id then
  misses the lookup — behaviorally identical to INVALIDATED.
- **Allowlist withdrawal is not a row state**: `require_auth` checks the bound
  `account_sub` against live config on every request (design §3), so withdrawal
  denies the next request without touching the table.

**Indexes**: PK on `id`; index on `account_sub` (join path for per-account
session listing and administrative invalidation).

### AuditEvent (E3-owned)

Append-only record of logins and ownership-relevant rejections (FR-15, SC-8).
Insert-only by rule: no update or delete path exists in application code — this
is the deliberate exception to the E2 `updated_at` guardrail, because an
append-only row never changes.

| Field | Type | Rules |
|---|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY **PK** | Monotonic append order. |
| `occurred_at` | timestamptz NOT NULL | Event time (server clock). |
| `actor_sub` | TEXT NULL | Acting account. Nullable: an allowlist-denied login has no account yet. |
| `event` | TEXT NOT NULL | `CHECK (event IN ('login_success','login_allowlist_denied','logout','forbidden_character_write','forbidden_effect_write','forbidden_custom_write','forbidden_gm_write'))` |
| `target` | TEXT NOT NULL | What was attempted: e.g. `character:<id>`, `effect:<id>`, `login`, `custom:<id>`. |
| `outcome` | TEXT NOT NULL | `allowed` / `denied` — stored explicitly even though each event kind implies one, so ad-hoc queries need no case table. |
| `request_id` | TEXT NULL | Correlates with the request-id on trace lines (E1 stack). |

**Relationships**: `actor_sub` logically references `accounts.sub`; **no FK** —
an allowlist-denied login presents a `sub` with no account row, and audit rows
must never be blocked by account lifecycle. Written exactly once per event by the
auth legs (`login_*`, `logout`) and by the extractor rejection path
(`forbidden_*`).

**Indexes**: on `occurred_at` (recall order) and `actor_sub`.

## Merge-order rule (E2 ∥ E3)

E3's migrations (`sessions`, `audit_events`) FK-reference `accounts`, so in final
migration order the `accounts` migration must precede them. Whichever epic merges
second rebases and re-timestamps its migrations; the rule exists so that rebase
is mechanical, not negotiated. E3's `audit_events` has no account FK (above), so
only `sessions` is order-sensitive.

## What E3 deliberately does not model

- **No password/credential columns** — there is no local credential path (FR-1).
- **No refresh tokens** — Authentik refresh tokens are not requested or stored;
  session continuity is Hireling's own sliding window. An expired session
  re-authenticates through the full code flow (US6).
- **No session status column** — expiry is evaluated from timestamps; deletion is
  invalidation. One less state machine to disagree with itself.
- **No ownership tables** — `characters.owner_sub`, effect/custom `creator_sub`
  belong to E2/E5/E8/E9. E3's contract to them: sole-writer keys are
  `accounts.sub` references, loaded fresh per request.
