# Contract: Degraded Mode & Offline Queue (E7)

**Status**: binding for E7 implementation · **build-target for E10** (the
service worker implements offline durability on top of this contract) ·
consumed by E6 (sheet UI reads the state signals) · extends `spec.md`
FR-8..FR-10.

How the client behaves when the backend is unreachable, the browser is
offline, or the connection is mid-reconnect — and the exact surface E10's
service worker may build against. The wire-level details live in
[`wire-protocol.md`](wire-protocol.md).

## 1. One path (the load-bearing rule)

**Unreachable and offline are the same state.** The connection layer
reports "no working socket" — it does not distinguish DNS failure, refused
connection, radio off, or mid-backoff. Every consumer below keys off that
single signal. Nothing may branch on *why* the link is down.

## 2. Client states

`degraded` is an overlay property of the store, not a separate machine:

| state | socket | view | queue |
|---|---|---|---|
| `live` | up, merged, drained | read-write | empty |
| `live+degraded` | up | read-write (writes enqueue anyway — same path) | non-empty (replay in progress) |
| `offline` (no socket, backing off) | down | **read-only, last-known state** | accepting writes |

- Last-known state is always fully readable — a cold UI renders the last
  merged store, never a spinner over nothing (FR-9: reads never blocked).
- In `offline`, edit affordances disable (E6/E10's UI choice) but the write
  API does not throw — it enqueues. The sync module never refuses a write.
- Transitions are silent. No modal, toast, or banner exists in this module.
  The **only** permitted visible signal: the `syncing` indicator (§5).

## 3. The queue

- **Unit**: the client operation — `{op_id, target, base_version, value,
  created_at}`, exactly the wire `write` frame's payload.
- **Durability**: persisted through page reload via the storage interface
  (`localStorage`, key `hireling:queue:{account_sub}` — per-account keying
  is the isolation boundary; a different account logging in never sees or
  replays another's queue). Known limit, accepted at P0: browsers may evict
  `localStorage` under storage pressure; E10's service-worker storage
  (persistent grant) is the sanctioned durable upgrade and must treat
  migrating this queue's contents as a first-run concern.
- **Ordering**: FIFO per account. No client-side conflict resolution, no
  client-side skip of "doomed" ops — the queue is dumb; the server
  arbitrates every replay (single CAS implementation, `wire-protocol.md` §4).
- **Bounds**: unbounded length, no silent drops (spec FR-8). No TTL.
- **Replay trigger**: only after a `snapshot` merge on a fresh connection
  (replaying into a stale view is how double-writes are born), then in FIFO
  order; acks processed as in live mode. The queue is the indicator's truth.

## 4. Outcomes, and what the UI does

| ack | meaning | module behavior | UI rule |
|---|---|---|---|
| `applied` | won the CAS | confirm the echoed value, drop from queue | nothing |
| `superseded` | lost the race (field moved past `base_version`) | **silently** revert the field to the server value (snapshot/diff), drop from queue | **nothing — ever** (spec FR-8; server logs it, not us) |
| `already_applied` | replay of a prior win | same as `applied` | nothing |
| `rejected` | invalid input, never applied | revert the echo, drop from queue, expose on the op record | E6's choice — this is a user-input error, not a sync event; **not** covered by the silence rule |
| `forbidden` | authz denied (incl. any GM write) | revert the echo, drop from queue, expose on the op record | E6's choice, same as `rejected` |

Local echo: a write applies to view state immediately, tagged `pending`
(carrying `op_id`). The tag clears on the ack; a `superseded`/`rejected`/
`forbidden` ack reverts that field to the server value. E6 renders pending
state as it sees fit (typically: value shown, control disabled until ack).

## 5. The `syncing` indicator

- **Visible iff the queue is non-empty.** Not "connection down", not
  "reconnecting" — the queue. Empty queue on a dead link = nothing to do =
  no indicator (read-only state carries its own affordance disabling, not a
  spinner).
- Subtle by contract: a small persistent affordance in the chrome, matching
  the prototype's design language. No animation cycles faster than 1 Hz, no
  color semantics beyond "neutral attention".

## 6. What E10's service worker may assume (and must not do)

> **E10 ruling (2026-10-08, supersedes the letter of the second "may
> assume" bullet below):** the store snapshot is NOT cached by the service
> worker. It lives app-side in per-account localStorage (the boot cache,
> `hireling:boot:{sub}`), seeded through the version merge on cold boot —
> a SW-held copy would be a second invalidation surface without adding
> safety; version-merge, not release invalidation, is what makes a stale
> snapshot harmless. The full ruling, with the rejected alternative, is
> [`specs/009-party-view-gm-seat-pwa/contracts/sw-shell-cache.md`](../../009-party-view-gm-seat-pwa/contracts/sw-shell-cache.md)
> §5. The bullets below stand as written for everything else; read "the SW
> caches it versioned" as "the APP persists it, keyed per account".

May assume:
- This module exists in-page and owns merge/replay/queue logic; the SW
  serves the shell and the initial store snapshot so a cold offline boot
  still renders §2's read-only view.
- The store's last-merged state is available (E7 exposes a serializable
  snapshot accessor); the SW caches it versioned and invalidates by app
  release, not by wall clock.
- `localStorage` queue contents are migrate-able input on first run.

Must not:
- Invent or translate wire frames (protocol changes go through
  `wire-protocol.md` by PR).
- Run its own reconciliation or version math — one CAS implementation,
  server-side, forever.
- Surface sync errors the page hasn't surfaced (no error theatre by proxy).

## 7. Session edge cases

- **Session expiry with a non-empty queue**: reconnect attempts get 401 →
  stay `offline`; after re-login (same account) the next connection
  snapshot-merges and drains the queue. The queue never crosses accounts.
- **Logout**: E3's logout flow clears the session; the queue stays keyed to
  its account on the device (it is the same user's pending work, and it
  replays only under that account's next session).
- **Multi-tab**: each tab runs its own connection; server-side CAS + this
  contract's revert rules make tab races a non-event. (Shared-worker
  single-socket optimization is explicitly out of P0 scope.)
