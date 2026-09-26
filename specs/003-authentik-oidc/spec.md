# Feature Specification: Authentik OIDC Auth & Ownership Enforcement (E3)

**Epic**: E3 — Phase 0, P0, parallel with E2/E4, blocks E5/E7 · GitHub issue #5
**Created**: 2026-09-20
**Status**: Draft
**Input**: `docs/EPICS.md` Epic E3 specify prompt + Constraints + AI Guardrails (decomposed from PRD v3.6)

## User Scenarios & Testing *(mandatory)*

E3 ships no visible product surface of its own — its "feature" is that the six
people at the table are who the app thinks they are, and that Hireling's
ownership law (Constitution Article VI: players own their sheets, casters own
their effects, the GM watches) holds on every request, not just the ones the
UI offers. Every story below is about identity being right and write
authority being enforced server-side.

### User Story 1 — A player logs in (Priority: P1) 🎯 MVP

A player (Josh, Bear, Dave, Becky, or Jake) opens the app, is sent to the
house identity provider, signs in there, and lands back in the app recognized
as their account — no account creation, no password field anywhere in
Hireling itself.

**Why this priority**: Every later epic keys off "the session identifies the
account." Import ownership claim (E5), party membership, the GM seat (E10),
and offline write reconciliation (E7) all trust this story's output.

**Independent Test**: From a fresh browser with no app state, open the app,
complete the provider login round trip, and confirm the app identifies the
session as the expected account. Repeat for each of the six accounts.

**Acceptance Scenarios**:

1. **Given** a visitor with no session, **When** they open the app,
   **Then** they are directed to the identity provider's login — the app
   itself never renders a password field or a signup form.
2. **Given** a successful provider login for one of the six provisioned
   accounts, **When** the provider returns them to the app, **Then** the app
   establishes a session bound to that account and lands them in the app.
3. **Given** an established session, **When** the account makes any request,
   **Then** the server resolves the account from the session — identity is
   never taken from client-supplied request data.
4. **Given** a successful login, **When** the account logs out, **Then** the
   session is invalidated server-side and subsequent requests with it are no
   longer authenticated.

---

### User Story 2 — An owner writes their own sheet (Priority: P1)

A player who owns a character changes live state on that character (HP, spell
slots, inventory — whatever later epics expose) and the server accepts the
write.

**Why this priority**: This is the happy path the whole product exists for —
and the baseline against which every rejection story is measured.

**Independent Test**: As the owning account, issue a write to the owned
character's state and verify it is accepted and persists. (The concrete write
paths arrive with E5+; this story is tested against the first write path that
exists, and the authorization rule is tested in isolation before that.)

**Acceptance Scenarios**:

1. **Given** an authenticated account that owns a character, **When** it
   writes to that character's state, **Then** the server accepts the write.
2. **Given** an authenticated account that owns no character yet (has not
   imported), **When** it reads party state, **Then** reads succeed normally —
   ownership gates writes, never reads.

---

### User Story 3 — A non-owner's write is rejected (Priority: P1)

A player attempts — by hand-crafted request, stale UI, or malice — to write
to a character they do not own. The server rejects the write, every time,
with the standard forbidden-response payload, and records the rejection.

**Why this priority**: The ownership matrix is the heart of this epic. The UI
will hide non-owners' edit affordances, but the server's enforcement is the
product promise ("players own their sheets") made load-bearing.

**Independent Test**: For every write path in the app, issue the write as an
authenticated account that is not the target character's owner and verify:
rejection, the standard forbidden payload shape, and an audit record.

**Acceptance Scenarios**:

1. **Given** an authenticated account that does not own character X, **When**
   it attempts any write to X's state, **Then** the server rejects the write
   with the standard forbidden response and X's state is unchanged.
2. **Given** a rejected ownership violation, **When** the rejection is
   recorded, **Then** the audit record identifies the acting account, the
   target, and the outcome.
3. **Given** any write endpoint added by a later epic, **When** it is
   exercised by a non-owner, **Then** the same rejection applies — no write
   path is exempt (deny by default).

---

### User Story 4 — A caster manages their own effect, even on other sheets (Priority: P1)

Bear casts *Bless* on Josh and Becky. Bear — the effect's creator — can edit
it, add or remove targets, and end it, even though it lives on other people's
sheets. Josh (a target, not the creator) cannot modify or end it, and neither
can Dave (neither target nor creator).

**Why this priority**: This is the second axis of the ownership matrix and
the one most likely to be implemented wrong ("the sheet's owner must approve
effect changes" is the tempting mistake — the PRD says the caster is the sole
writer of effects they created).

**Independent Test**: As the creator account, modify and end an effect
targeting another account's character — accepted. As the target character's
owner, attempt the same modifications — rejected. As a third account, attempt
the same — rejected.

**Acceptance Scenarios**:

1. **Given** an effect created by account A that targets account B's
   character, **When** A edits the effect or changes its targets or ends it,
   **Then** the server accepts.
2. **Given** the same effect, **When** B (owner of a targeted character)
   attempts to edit, retarget, or end it, **Then** the server rejects with
   the standard forbidden response.
3. **Given** the same effect, **When** any other non-creator account attempts
   the same, **Then** the server rejects identically.
4. **Given** any authenticated account, **When** it reads the effect and its
   provenance, **Then** the read succeeds — effect visibility is unrestricted
   within the party.

---

### User Story 5 — The GM reads everything and writes nothing (Priority: P1)

Bruce's GM account opens the party view and reads every character's state and
every active effect. Any write attempted by the GM account — to any
character, any effect, anything — is rejected by the server, even though the
UI never renders him an edit control.

**Why this priority**: Constitution Article I is explicit: no GM workload is
a load-bearing non-goal, and the GM seat is read-only by design. Server-side
rejection is the guarantee that survives UI bugs.

**Independent Test**: As the GM account, read all party state (accepted) and
attempt every write path in the app (all rejected with the standard forbidden
response, all audit-recorded).

**Acceptance Scenarios**:

1. **Given** the GM account, **When** it reads any party state, **Then** the
   reads succeed like any other party member's.
2. **Given** the GM account, **When** it attempts any write on any endpoint,
   **Then** the server rejects it with the standard forbidden response,
   regardless of target.
3. **Given** the GM account owns no character, **When** it attempts to create
   an effect or import a character, **Then** the server rejects — the GM
   account holds no write authority of any kind.

---

### User Story 6 — A session expires and the player re-authenticates (Priority: P2)

A player's session reaches its lifetime limit (or is invalidated) mid-use.
The app stops accepting the session, tells the player authentication is
needed, and after re-login the player is back where they were — as the same
account.

**Why this priority**: Session timeout + invalidation is a stated guardrail,
but at a six-person friends-and-family table the failure mode that matters is
"the sheet stopped working mid-combat," so the experience of expiry is P2 to
the enforcement stories above.

**Independent Test**: Establish a session, force it to expire, verify
subsequent requests are rejected as unauthenticated (not forbidden), then
re-authenticate and verify the account and app context are restored.

**Acceptance Scenarios**:

1. **Given** an expired or invalidated session, **When** any request is made
   with it, **Then** the server responds unauthenticated — distinctly from
   the forbidden response used for authorization failures.
2. **Given** an expired session in the UI, **When** the player next
   interacts, **Then** they are directed to re-authenticate and, after
   login, return as the same account.
3. **Given** an explicit logout, **When** the old session is replayed,
   **Then** it is rejected as unauthenticated.

---

### Edge Cases

- **Authenticated but unknown account**: the provider authenticates someone
  outside the six designated accounts. The app denies access (no session
  established as a party member) and records the attempt. Default: an
  account allowlist, configurable, per Assumptions.
- **Identity provider unreachable**: existing sessions remain valid until
  their own expiry; new logins fail with a human-readable message. The app
  never falls back to any local credential path — there isn't one.
- **Account deprovisioned at the provider while a session is live**: the
  session is not silently immortal — sessions carry bounded lifetimes, and
  the allowlist can withdraw access immediately (denied on next request).
- **Client-supplied identity disagreement**: a request carries account or
  character identifiers that contradict the session's account. The server
  resolves identity from the session alone; a conflicting request is treated
  as what it is — typically a non-owner write — and rejected accordingly.
- **Ownership reassigned mid-session**: reassignment is a DB operation (per
  Constraints); authorization is evaluated per request against current
  ownership, so a reassignment takes effect on the next request with no
  session restart.
- **Two devices, one account**: both sessions carry the same account identity
  and the same write authority; conflicts between them are E7's versioning
  concern, not an authorization question.
- **Realtime connections**: the party sync channel (E7) is an endpoint like
  any other — unauthenticated connections are refused, and the GM's
  connection carries no write authority either.

## Requirements *(mandatory)*

### Functional Requirements

Authentication and session:

- **FR-1**: The system MUST authenticate users exclusively via delegated
  login at the house identity provider (protocol per Constraints). The app
  MUST NOT render a signup form, a password field, or any local credential
  path.
- **FR-2**: The system MUST restrict access to a configured allowlist of
  accounts (the six designated: Josh, Bear, Dave, Becky, Jake, Bruce). A
  successful provider login by an account not on the allowlist MUST be
  denied and audit-recorded.
- **FR-3**: Every authenticated request MUST resolve the acting account from
  the server-held session. The server MUST NOT trust client-supplied identity
  claims (headers, parameters, or body fields) to determine who is acting.
- **FR-4**: Sessions MUST have a bounded lifetime and MUST be invalidatable
  server-side (logout, administrative action, allowlist withdrawal). An
  expired or invalidated session MUST be rejected as unauthenticated on
  every endpoint.
- **FR-5**: The unauthenticated response MUST be distinguishable from the
  authorization-failure response, so clients can route the user to
  re-login rather than reporting a permission error.

Authorization — deny by default:

- **FR-6**: EVERY endpoint (including realtime/sync connections and any
  endpoint added by later epics) MUST require authentication unless it is
  explicitly designated public. The only public endpoint at this stage is
  the health check.
- **FR-7**: EVERY endpoint that mutates state MUST perform a server-side
  authorization check against the ownership matrix below. Endpoints MUST NOT
  default to allowing a write when no explicit check exists — absence of a
  rule is a denial.

The ownership matrix (enumerated — this is the heart of the epic):

- **FR-8**: A character's owning account is its sole writer. A write to a
  character's state by any other account MUST be rejected. Ownership is
  bound at import (the importing account owns the character) and changes
  only by the out-of-band reassignment path (Constraints).
- **FR-9**: An effect's creating account is its sole writer — including
  editing it, adding or removing targets, and ending it — regardless of
  whose characters the effect targets. A write to an effect by any
  non-creator account (including the owner of a targeted character) MUST be
  rejected.
- **FR-10**: Only an account that owns a character may create an effect (the
  caster is a character at the table). Any character owner may create
  effects; creation is not restricted to effects on one's own sheet.
- **FR-11**: A creator-owned custom content row (the `custom` lane E9 builds)
  is writable only by its creating account, by the same sole-writer rule as
  effects. E3 establishes the rule; E9's endpoints consume it.
- **FR-12**: The GM account (Bruce) MUST be rejected on every write of every
  kind — character state, effects, custom content, party settings, and
  anything later epics add. GM reads succeed like any party member's.
- **FR-13**: Reads are unrestricted within the party: any authenticated
  allowlisted account (players and GM alike) MAY read all party state —
  every character, every effect, every visible projection of it. Ownership
  gates writes; nothing gates reads.

Enforcement quality:

- **FR-14**: Every authorization rejection MUST return the same response
  shape across all endpoints: a stable machine-readable error code and a
  human-readable message, carrying no internal detail (no stack traces, no
  leaked ownership internals beyond what the party can already read).
- **FR-15**: The system MUST audit-record every login (success and allowlist
  denial) and every ownership-relevant rejection (non-owner character write,
  non-creator effect write, any GM write), identifying the acting account,
  the attempted action and target, and the outcome.
- **FR-16**: Authorization MUST be evaluated per request against current
  ownership state, so an out-of-band ownership reassignment takes effect
  without any session action.

### Key Entities

- **Account**: one of the six pre-provisioned identities at the house
  identity provider, mapped into the app from the provider's identity claims
  (claim mapping per Assumptions). Carries a stable provider identifier, a
  display name, and a role: player or GM.
- **Session**: a server-held binding between a browser and an account, with a
  bounded lifetime, invalidatable. The sole source of acting identity.
- **Account allowlist**: the configured set of provider identifiers the app
  will establish sessions for.
- **Character ownership**: the binding of a character to the account that
  imported it. One character per account at POC; reassignable only by a DB
  operation.
- **Effect authorship**: the binding of an effect (and likewise a custom
  content row) to its creating account.
- **Audit record**: an append-only entry for a login or an
  ownership-relevant rejection: actor, action, target, outcome, timestamp.

### Constraints (settled by the epic — non-negotiable)

- Existing house Authentik instance, OIDC only. No other identity provider,
  no local accounts, no password handling in the app.
- Ownership is claimed at import: the importing account owns the character.
- One character per account at POC; ownership reassignment is a DB
  operation, not a UI feature.
- Six static accounts; no self-serve signup, ever (PRD non-goal).
- Constitution Article I: the GM seat is read-only — server-enforced, not
  merely UI-hidden.
- Constitution Article VI: players own their sheets; casters own their
  effects. The matrix above is that promise, enumerated.

### Assumptions

- **Account mapping**: the OIDC `sub` claim is the stable account identifier
  the allowlist and all ownership bindings key on; display name comes from
  the provider's name/username claims. Account provisioning in Authentik
  (creating the six users) is Josh's operational task, outside the app.
- **GM designation**: which allowlisted account is the GM is application
  configuration (Bruce), not derived from provider group membership — one
  party at POC makes a config flag the simplest correct mechanism.
- **Session lifetime default**: sessions are sliding with activity and hard-capped
  at an absolute lifetime on the order of days (a week), so a player who
  logs in before game night stays logged in through it, while a deprovisioned
  account cannot linger indefinitely. Exact values are a plan-stage tuning
  knob; the requirements are only "bounded" and "invalidatable."
- **Local dev strategy**: a dev-only mechanism (env-flagged) lets
  developers and agents establish sessions as any of the six seats without a
  live identity provider, so the ownership matrix is testable locally and in
  CI. This mechanism MUST be inert in the production configuration; its
  absence in prod is part of its definition.
- **First-run flow**: an authenticated player with no character yet sees the
  empty party screen with the import CTA (per the PRD's adopted design); the
  import itself, and the ownership claim it performs, are E5 scope — E3
  defines the claim's semantics (FR-8) that E5 executes.
- **Sync channel**: E7's realtime connection authenticates via the same
  session as the rest of the app; no separate credential path.
- **Health check stays public**: `/healthz` (E1) requires no session —
  Heimdall's probe depends on it — and exposes no party state.

## Success Criteria *(mandatory)*

- **SC-1**: The full ownership matrix is verified by an automated test that
  exercises every endpoint against every role combination (owner, non-owner,
  effect creator, effect target, third party, GM, unauthenticated): 100% of
  verdicts match the matrix, with zero untested write endpoints.
- **SC-2**: A non-owner's write attempt to another account's character is
  rejected 100% of the time with the standard forbidden payload — identical
  in shape on every endpoint it is tested against.
- **SC-3**: A GM write attempt is rejected 100% of the time with the same
  standard forbidden payload, on every write path that exists.
- **SC-4**: An effect's creator can modify and end it even when it targets
  another account's character; the target's owner and third parties are
  rejected 100% of the time.
- **SC-5**: Any unauthenticated request to a non-public endpoint is rejected
  as unauthenticated 100% of the time, and the response is distinguishable
  from an authorization failure (verified by test).
- **SC-6**: A player completes login — from opening the app to an identified
  session — in under one minute with no password ever entered into the app
  itself (observed once per seat during first-use onboarding).
- **SC-7**: An expired or logged-out session is rejected on every subsequent
  request, and after re-authentication the user resumes as the same account.
- **SC-8**: 100% of logins (including allowlist denials) and
  ownership-relevant rejections appear in the audit record with actor,
  action, target, and outcome — verified by test, one record per event.
