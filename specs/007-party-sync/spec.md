# Feature Specification: Party Sync & Realtime (E7)

**Epic**: E7 — Phase 1, Lane B, P0 · depends on E2 (schema) + E3 (auth) · parallel with E5 (Lane A) · blocks E8, E10 · GitHub issue #9
**Created**: 2026-09-26
**Status**: Draft — specify step complete; two clarification questions open (Q1, Q2 below)
**Input**: `docs/EPICS.md` Epic E7 specify prompt + Constraints + AI Guardrails; PRD v3.6 Feature Group 2 + Technical Metrics; E2's `specs/002-database-schema/data-model.md` (per-field version columns — consumed, not re-designed)

---

## User Scenarios & Testing *(mandatory)*

E7's users are the six party accounts at the table, plus two downstream epics
that build directly on what this epic ships: E8 (the buff engine recomputes on
every synced effect change) and E10 (the PWA service worker builds against the
degraded-mode contract published here). Nothing in E7 is a screen of its own —
it is the nervous system the sheet UI (E6) and party view (E10) will feel
through.

### User Story 1 — The table sees the hit land (Priority: P1) 🎯 MVP

Becky takes damage and marks it on her sheet. Every connected party member's
view — Josh's sheet, Bear's, the GM's later party screen — reflects the new HP
within the latency budget, with no refresh and no action on anyone's part. A
slow or broken connection on one member's device never delays the broadcast to
anyone else.

**Why this priority**: This is the product's core promise — one shared table
state. Everything else in the epic (queueing, catch-up, degraded mode) exists
so this story keeps being true when the network misbehaves.

**Independent Test**: Two clients join one party session; client A writes HP;
assert client B's applied state changes without reload, the event is logged
with its dispatch latency, and killing client B's socket mid-broadcast leaves
client C's update unaffected.

**Acceptance Scenarios**:

1. **Given** two or more connected party members, **When** one commits a
   live-state write, **Then** every other connected member's client applies
   the resulting diff without a page reload or user action.
2. **Given** a broadcast to a party, **When** delivery to one connected client
   fails or stalls, **Then** delivery to every other client is unaffected —
   no client's send failure blocks another's.
3. **Given** any committed live-state write, **When** the broadcast is
   dispatched, **Then** the event is logged with its measured dispatch
   latency (Success Criterion SC-1's input).
4. **Given** a party member with the GM role, **When** any state change
   broadcasts, **Then** the GM receives it like any member, and any write the
   GM attempts is rejected server-side per E3's rules.

### User Story 2 — The tunnel drops mid-session (Priority: P1)

The home tunnel flakes at 8pm with the party mid-fight. Backend unreachable
and browser offline are the same path: the sheet stays fully readable from
last-known state, writes are refused as edits and accepted as queue entries,
and a subtle "syncing…" indicator shows while the queue is non-empty. No
error modal, no dead sheet, no lost writes. When the tunnel comes back, the
connection re-establishes itself silently, the queue replays, and every
acknowledged write landed exactly once. A queued write that raced against a
newer write of the same field loses silently — the view re-renders to synced
state, and the loss is logged server-side.

**Why this priority**: Session-night resilience is the reason E7 exists as its
own epic. The PRD's offline rules (FG2) are the contract the whole PWA story
depends on.

**Independent Test**: With the server stopped, make writes from a client,
reload the page, and verify the queue survived and the sheet rendered
read-only. Restart the server and verify the queue drains with each operation
applied exactly once; craft a stale-base write and verify it is superseded
silently client-side and recorded server-side.

**Acceptance Scenarios**:

1. **Given** the backend is unreachable (network down or server up but
   erroring), **When** the client attempts any live-state read or write,
   **Then** the client serves last-known state read-only and enqueues the
   write locally — one code path for both causes.
2. **Given** a non-empty local write queue, **When** the page is reloaded,
   **Then** the queue and its entries survive intact.
3. **Given** the backend becomes reachable again, **When** the client
   reconnects, **Then** queued writes replay with each acknowledged exactly
   once — a replayed operation the server already applied is detected as such
   and acknowledged, never applied twice, including across a server restart.
4. **Given** a queued write whose target field advanced past the version the
   write was based on, **When** it replays, **Then** it is dropped without
   any user-facing error, the client re-renders to server state, and the
   supersession is logged server-side with the operation id, account, field,
   base version, and winning version.
5. **Given** any sync-failure condition in this story, **When** it occurs,
   **Then** no modal, dialog, or blocking error UI appears — the only visible
   signal is the "syncing…" indicator while the queue is non-empty.

### User Story 3 — Two devices, one owner, zero ceremony (Priority: P1)

Josh has the sheet open on his laptop and his phone. He edits on both. There
is no lock, no "another device is editing" banner, nothing to accept or
dismiss. Both devices converge on server-receipt order; a device that wrote
against stale state simply re-renders to the synced result.

**Why this priority**: The PRD settled this (no locking UI — versioning
handles it). This story is the proof the settlement works end to end.

**Independent Test**: Two clients authenticated as the same owner, both
offline, write different values to the same field; reconnect them in a chosen
order; assert both converge to the value determined by server-receipt order
and neither surfaces a conflict UI.

**Acceptance Scenarios**:

1. **Given** one owner on two connected devices, **When** either device
   writes, **Then** both devices apply the change and neither ever presents a
   lock, conflict prompt, or takeover notice.
2. **Given** two writes to the same field from the same owner on different
   devices, **When** both reach the server, **Then** server-receipt order
   decides the winner; the loser is superseded per US2.4 and the losing
   device re-renders silently.
3. **Given** writes to different fields from the two devices, **When** both
   replay, **Then** both apply — fields are independent; there is no
   whole-character conflict concept.

### User Story 4 — The latency number has a measurement behind it (Priority: P2)

The operator can, on demand, produce the p95 dispatch latency the epic is
held to — from per-event structured logs and a live histogram — and the
end-to-end wifi/cellular budgets are proven by an automated two-client test
under shaped network profiles, not by assertion in a document.

**Why this priority**: P2 because the mechanism that makes it true (per-event
instrumentation) ships with US1 anyway; this story is the operator-facing
readout and the CI evidence that the budgets hold. An unmeasured target is a
wish.

**Independent Test**: Run the integration test under both network profiles
and observe the reported p95 within budget; hit the metrics readout on a live
server with traffic and observe a histogram consistent with the logs.

**Acceptance Scenarios**:

1. **Given** a server with party traffic, **When** the operator reads the
   sync metrics, **Then** a dispatch-latency histogram (count, p50, p95, p99)
   is available behind session auth, consistent with per-event log lines.
2. **Given** the two-client integration test under the home-wifi profile,
   **When** it runs, **Then** it reports p95 send-to-applied under 1s.
3. **Given** the same test under the cellular profile, **When** it runs,
   **Then** it reports p95 send-to-applied under 3s.

---

## Edge Cases

- **Own-write echo race**: the writing client may receive its own change as a
  broadcast before or after its write acknowledgement. Merging is by per-field
  version (apply only strictly newer), so both orders are correct and
  idempotent.
- **Reconnect during a write burst**: catch-up snapshot merges first, then the
  queue replays; per-field versions arbitrate everything. No burst ordering
  logic exists outside version comparison.
- **Ack-lost replay**: a write applied server-side whose response never
  arrived is replayed by the client; the server must recognize the operation
  id (durably — see FR-8) and acknowledge as already-applied, not re-apply.
- **Server graceful shutdown mid-session**: E1's drain owns the WS close (no
  second shutdown path); clients treat the going-away close as degraded mode
  and reconnect per FR-10. No acknowledged write is lost.
- **Logout or session expiry with a non-empty queue**: the queue persists,
  keyed to its account; it replays only after re-authentication as that
  account; one account's queue is never visible to or replayable by another.
- **Backgrounded tab**: browser timer throttling must not lose queued-write
  flushes or indicator state; flush is driven by connectivity/visibility
  events, not only timers.
- **Solo party session**: a party with one connected member still versions
  and logs everything; fan-out to zero peers is a degenerate no-op, not an
  error.
- **Hostile or skewed client clock**: no ordering, supersession, or
  measurement decision ever uses client time. Cross-clock measurements exist
  only in the same-machine test harness; production metrics span
  server-observable points only.
- **Message before handshake completes / malformed frames**: denied by
  default and dropped with a logged reason; a party socket carries exactly
  the message types this spec defines.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1**: The system MUST provide one WebSocket session per party,
  authenticated by the existing E3 session at handshake. Every connected
  party member and the GM account receives broadcasts. Authorization MUST be
  enforced per message server-side (E3's rules: owner is sole writer of their
  character's fields; the GM writes nothing), deny-by-default on every
  message type — not only at handshake.
- **FR-2**: Live-state writes MUST enter through the party WebSocket; REST
  remains auth/import/bootstrap per the PRD. Every write MUST carry a
  client-generated unique operation id and, per target field, the field
  version it was based on (`base_version`). The server MUST assign new field
  versions from E2's `field_version_seq` in the same transaction as the
  write.
- **FR-3**: Every committed live-state change MUST fan out to all connected
  party clients as a diff identifying field, new value, new version, and
  acting account. A send failure to one client MUST NOT block or delay
  delivery to any other client. Disconnected clients are served by catch-up
  (FR-7), never by per-client retry queues.
- **FR-4 — the measured interval**: The dispatch interval is defined as:
  *server receipt of an accepted write message* → *the resulting diff
  enqueued to every connected party client's outbound send path*. It MUST be
  measured per event (`sync_dispatch_ms`), written to the structured log, and
  maintained in an in-process histogram with a session-gated readout. Budget:
  **p95 ≤ 100 ms at POC load** (POC load = 1 party, 6 connected clients,
  sustained 1 write/s with 10-second bursts at 5 writes/s).
- **FR-5 — the end-to-end budgets**: The end-to-end interval is: *writing
  client sends* → *another connected client has applied the change*. Budgets:
  **p95 < 1 s on the home-wifi profile, p95 < 3 s on the cellular profile**,
  where the profiles are defined for the test harness as — wifi: 50 ms RTT,
  ±20 ms jitter, 0% loss; cellular: 300 ms RTT, ±100 ms jitter, 1% loss.
  Verification MUST come from an automated same-machine two-client
  integration test under these shaped profiles (per the PRD's Technical
  Metrics ruling: server-side instrumentation is the production measurement;
  client-receipt instrumentation in production is productization, out of POC
  scope).
- **FR-6 — version granularity (adopted from E2, not re-designed)**: The
  unit of versioning and conflict is exactly the field set E2 versioned:
  `hp`; `temp_hp`; money as one four-denomination unit; `level_adjust`
  [NEEDS CLARIFICATION: Q1 — confirm money and level_adjust sync at POC, or
  HP/temp-HP/slots/inventory/effects only]; each spell slot individually
  (`caster_key` + `rank` + `slot_index`); each inventory item's quantity
  (`item_name`-keyed); each effect as a whole (any target, modifier, or
  active change is one version bump). Effects are whole-row versioned —
  there is no per-modifier version.
- **FR-7 — catch-up**: On connect and on reconnect, the client MUST receive a
  party live-state snapshot carrying every field's current version. The merge
  rule is uniform: apply only fields whose version is strictly newer than the
  local one. Server-receipt order (the global sequence) is the only ordering
  authority; client clocks are never consulted.
- **FR-8 — offline and degraded mode (one path)**: Backend unreachable and
  browser offline MUST be the same code path: last-known state served
  read-only, writes enqueued to a client-durable queue that survives page
  reload. The queue MUST NOT be bounded and entries MUST NOT be silently
  dropped client-side. Replay MUST be idempotent: the server MUST recognize
  an already-applied operation id and acknowledge it as applied — durably,
  surviving server restart (a persisted operation ledger, not an in-memory
  cache). A write whose `base_version` is older than the field's current
  version MUST be superseded: applied to nothing, silently dropped from the
  client's view, and logged server-side with operation id, account, field,
  base version, and winning version.
- **FR-9 — the indicator**: A subtle "syncing…" indicator MUST be visible if
  and only if the client's write queue is non-empty. Sync events (dropped
  connection, supersession, replay) MUST NOT produce modals, toasts, or
  blocking error UI. Reads MUST remain available in every degraded state.
- **FR-10 — reconnect and liveness**: Reconnection MUST be silent and
  automatic: exponential backoff starting at 1 s, doubling, capped at 30 s,
  with full jitter; the backoff resets once a connection completes catch-up.
  Liveness: the server pings every 20 s and expects a pong within 10 s; a
  missed pong is treated as a dead connection on both ends. The reconnect
  sequence is fixed: handshake → snapshot merge (FR-7) → queue replay
  (FR-8) → live operation.
- **FR-11 — shutdown**: WebSocket shutdown MUST join E1's existing graceful
  drain as one shutdown path: stop accepting new party connections, finish
  dispatching in-flight broadcasts, then close sockets with a going-away
  code that clients translate into degraded mode plus FR-10 backoff. No
  separate WS shutdown mechanism exists.
- **FR-12 — contract publication**: The degraded-mode semantics, queue and
  replay rules, catch-up merge rule, and indicator contract MUST be
  published as contract documents under `specs/007-party-sync/contracts/` at
  the design step, written so E10's service worker can be built against them
  without re-deriving any decision [NEEDS CLARIFICATION: Q2 — confirm the
  E7/E10 boundary: E7 owns in-session degraded mode and the durable
  client-side queue; E10 owns the service-worker offline shell and
  cross-session caching].
- **FR-13 — extensibility (E12)**: The sync mechanism MUST be table-agnostic:
  any live-state row carrying a `field_version_seq`-fed version column joins
  broadcast, catch-up, and conflict handling by construction. E12 opts the
  stash/bank surfaces in by adding version columns in its own migration
  (`party_bank` is already versioned; `party_stash` is not yet) — with no
  change to the sync engine itself. This epic MUST NOT build stash/bank
  behavior.

### Key Entities

- **Party session** — one authenticated WebSocket bound to a party; the
  broadcast domain.
- **Broadcast diff** — {field path, value, version, actor account} emitted to
  all party sessions on commit.
- **Field version** — E2's per-field monotonic version from the one global
  sequence; the only ordering authority.
- **Client operation** — {operation id, field, base_version, payload}; the
  queued/replayed unit; idempotency key.
- **Sync queue** — the client-durable, account-scoped, unbounded operation
  list; the indicator's data source.
- **Catch-up snapshot** — party live state with per-field versions, sent on
  (re)connect.
- **Supersede record** — the server-side log entry for a losing write.
- **Dispatch measurement** — `sync_dispatch_ms` per event; histogram + log.

### Constraints (settled by the epic — non-negotiable)

- Client clocks are untrusted; server-receipt order always wins (PRD FG2,
  EPICS E7).
- Two devices, one owner: no locking UI — versioning handles it.
- One WebSocket per party; REST stays auth/import/bootstrap (PRD).
- E2's per-field version columns fed by `field_version_seq` **are** the
  mechanism — this epic consumes them and re-designs nothing about them.
- Ownership and write-gating come from E3 and are enforced per WS message,
  deny-by-default.
- E1's graceful shutdown owns connection draining; E7 adds no second path.
- Stack (confined to constraints per house convention): Rust/axum single
  binary, Svelte static bundle served by it.
- PRD Technical Metrics ruling: server-side instrumentation is the POC
  measurement; production client-receipt instrumentation is productization.

### Success Criteria

- **SC-1**: p95 `sync_dispatch_ms` ≤ 100 ms at POC load, producible on demand
  from the metrics readout and the structured log.
- **SC-2**: The two-client integration test reports p95 send-to-applied
  < 1 s (wifi profile) and < 3 s (cellular profile), and runs in CI.
- **SC-3**: A six-client party session survives a backend restart with zero
  manual reloads, zero user-visible errors, and zero lost acknowledged
  writes (soak: 10,000 operations, every one exactly-once).
- **SC-4**: 100% of superseded writes are logged server-side; 0 surface as
  client errors; no lock or conflict UI exists anywhere.
- **SC-5**: E10's implementer can build the service worker from
  `specs/007-party-sync/contracts/` alone (reviewer-verified at E10).

### Assumptions

- E6 does not exist yet (parallel lane): E7 ships the sync client as a
  headless module plus a minimal internal debug page — not the product sheet
  UI. E6 and E10 consume it.
- One party is live at POC; the schema and the WS are per-party from day one
  (multi-party is config, not change).
- Queue durability means browser storage surviving reload; anything
  surviving browser-data wipe or offline app boot is E10's service worker.
- Money-as-one-unit and `level_adjust` syncing are E2-uniformity defaults,
  pending Q1.

### Out of scope (guarded)

Party view UI, service worker, PWA shell, offline app boot — E10. Buff/effect
engine semantics — E8. Stash/bank sync behavior — E12. Any combat/round
tracking, auto-expiry, or aura automation — non-goals, Constitution Article I.
