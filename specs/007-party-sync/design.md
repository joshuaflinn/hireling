# Design: Party Sync & Realtime (E7)

**Epic**: E7 — Phase 1, Lane B, P0 · depends on E2 (schema) + E3 (auth) · GitHub issue #9
**Created**: 2026-09-26 · **Status**: Draft for human review (design gate)
**Consumes**: `spec.md` (approved + clarified 2026-09-26), E2's `specs/002-database-schema/` (version columns, `field_version_seq`, design §4–5), E3's `src/auth/` (sessions, `authorize()`), E1's drain (`DRAIN_TIMEOUT`, `router`)
**Produces**: `data-model.md` (one new table — E7's first owned migration), `contracts/wire-protocol.md`, `contracts/degraded-mode.md` (E10's build target)

> Standing rules honored throughout: E2's design §5 ruling — **no LISTEN/NOTIFY,
> no outbox, no queue table alongside Postgres; broadcast is in-process on
> write success.** Constitution Article V — no new crates without written
> justification (one dev-dependency requested below, justified).

## Decisions at a glance

| Fork | Decision | Why |
|---|---|---|
| Transport | axum's `ws` **feature flag** on the existing `axum = "0.8"` dep; one route `GET /api/ws/party/{party_id}` inside E3's session-protected nest | Feature flag, not a new crate (Art. V). Same-origin cookies ride the upgrade; E3's middleware authenticates before upgrade — no new auth path, handshake and per-message authz both reuse `authorize()`. |
| Fan-out | In-process party registry: `RwLock<HashMap<party_id, Vec<ConnHandle>>>`; each conn = bounded mpsc(64); `try_send` only — a full/slow channel closes **that one** connection (close 1013), which recovers via snapshot catch-up | E2's settled ruling (in-process). `try_send` never awaits, so one bad client cannot block others (FR-3) — the mechanism is structural, not disciplined. A slow client's cure is reconnect+snapshot, which we need anyway. |
| Write semantics | Per-field **compare-and-set**: `UPDATE … SET value=$1, version=nextval('field_version_seq') WHERE <row-key> AND version=$base_version`; rowcount 0 → superseded. Idempotent replay via a durable op ledger (`client_ops`, see `data-model.md`) | The PRD's "server-receipt order wins / losing write silently superseded" *is* CAS-on-base-version. Durable ledger because ack-loss replay must survive server restart (FR-8); in-memory dedupe dies with the process. |
| Catch-up | **Snapshot, not delta-log**: on (re)connect the server sends the party's full live state with per-field versions; client merges strictly-newer | E2 built the total order precisely so catch-up is one cheap predicate; at 6 users a snapshot is ~10–300 KB (measured in test, logged as `snapshot_bytes`). A delta-log is a second source of truth to keep consistent — dismissed at POC (see Alternatives). |
| Field addressing | Structured JSON targets, not dotted strings: `{kind:"vitals",field:"hp"}`, `{kind:"slot",caster_key,rank,slot_index}`, `{kind:"inv",item_name}`, `{kind:"effect",effect_id}` | No escaping problem (item names contain spaces; caster keys too), no parser to get wrong, validates as data. |
| Effects | Transport carries effects read-only in E7: rows appear in snapshots and diffs and version whole-row. **No effect write command exists until E8** — E8 defines effect write semantics on this transport (coordination point) | E8 owns the effect model (`{name, targets[], modifiers[], active}`). Shipping a partial effect-write path now would pre-decide E8's semantics. Nothing in E7's FRs requires effect *writes* — only propagation. |
| Liveness | App-level `ping`/`pong` JSON frames both directions: server every 20 s, 10 s timeout; client declares dead after 50 s of silence | One mechanism, works through every proxy/browser; protocol-level WS pings are auto-ponged by browsers and invisible to page JS, so they can't drive the client side. Numbers from FR-10. |
| Reconnect | Backoff 1 s ×2 cap 30 s, **full jitter** (sleep = rand·cap_n); resets only after snapshot merge + queue drain complete | FR-10's curve. Full jitter avoids thundering-herd sync after a server restart with 6 clients. |
| Client module | Plain-JS `web/src/lib/sync/` (connection, store, queue, merge, indicator signal) + `node --test` unit tests; debug page `web/src/lib/sync/debug.html`-style harness mounted under an internal route | Matches the existing web shape (`main.js`, `lib/*.js`, `node --test`). No framework lock-in; E6/E10 consume the module, not a component. |
| Latency instrumentation | `t0` = write frame fully received (pre-DB), `t1` = fan-out loop complete; `sync_dispatch_ms=t1−t0` per event in the structured log + hand-rolled 10 000-sample ring (p50/p95/p99 on demand) at `GET /metrics/sync` (session-gated) | Zero new deps (Art. V); PRD says logging is free because we own the server. Histogram is ~40 boring lines. |
| Link-shaped e2e test | In-repo test-only TCP proxy (~120 lines tokio): seeded latency/jitter/loss between test client and server; profiles from FR-5 | toxiproxy = new external service in CI (Art. V fight). The thing under test is client+server behavior under delay, not the network itself — an in-process shaper measures the same thing with no dependency. |
| WS test client | `tokio-tungstenite` as **dev-dependency** (Article V justification: it is already in the lockfile transitively via axum's `ws` feature; promoting it to a direct dev edge adds zero new code to the build and is the standard client for axum WS integration tests) | Integration tests speak real WS against a real ephemeral listener — the alternative (mocked sockets) tests nothing about the transport. |

## Architecture

```
                        ┌─ in-process ──────────────────────────────┐
 client A ──WS──┐       │  party registry                           │
                │       │   party 1: [connA, connB, connGM]         │
 client B ──WS──┼───────│                                           │
                │       │  write path (per frame):                  │
 GM ───────WS───┘       │   authz(authorize) → tx {                 │
                        │     ledger check (op_id)                  │
                        │     CAS write + nextval(field_version_seq)│
                        │     ledger insert (outcome)               │
                        │   } → commit → fan-out try_send → t1      │
                        └───────────────────────────────────────────┘
                              │
                        Postgres (Asgard hall / compose in dev)
                        E2's tables + client_ops (E7's one migration)
```

Server module layout (flat, per house style): `src/sync/mod.rs` (registry,
route wiring), `src/sync/protocol.rs` (frame types — serde), `src/sync/write.rs`
(CAS + ledger), `src/sync/snapshot.rs`, `src/sync/metrics.rs`. Client:
`web/src/lib/sync/connection.js`, `store.js`, `queue.js`, `merge.js`,
`index.js` (public API), `debug.js` + one route registration.

## Write path, precisely

1. Frame received → `t0`. Decode; unknown frame type or malformed → log +
   drop frame (deny-by-default; repeated garbage → close 1003).
2. `authorize(actor, write_action, resource)` per message (E3's pure fn).
   Deny → ack `forbidden`. GM write → `forbidden` (server-enforced, FR-1).
3. Begin tx. `SELECT outcome FROM client_ops WHERE op_id=$1` — hit →
   ack stored outcome as `already_applied` (no write, no broadcast), commit,
   done. (Ack-loss replay path.)
4. CAS statement (one per write, per E2's write shape). Rowcount 1 →
   `applied`, `resulting_version` from `RETURNING`. Rowcount 0 → field moved:
   re-read current version → outcome `superseded` (+ `winning_version`).
5. `INSERT client_ops(op_id, account_sub, field_path, outcome, resulting_version)`.
6. Commit. On `applied`: build diff, fan-out `try_send` to every party conn
   (including the writer's — version dedupe makes the echo harmless), `t1`,
   log `sync_dispatch_ms` + histogram push. On `superseded`: structured log
   (op_id, account, field, base, winning version — FR-8's server-side record)
   + ack; **no broadcast** (nothing changed).
7. Ack to writer: `{op_id, outcome, version?}`.

Bounds validation before step 4 (server-side, complementing E6's client
checks): `hp ≥ 0`, `temp_hp ≥ 0`, money denominations `≥ 0`, `level_adjust`
in `−19..19`, slot `rank 0..=10`, `qty_delta` any int (signed by design).
Violation → ack `rejected` with reason; rejected ≠ superseded — rejected
means never applied (bad input), superseded means lost a fair race. The
client store exposes both outcomes; E6 chooses surfacing for `rejected`
(optimistic rollback), while `superseded` is always silent (FR-8/FR-9).

## Client states (summary — contract in `contracts/degraded-mode.md`)

`connecting → live → (connection lost) → backoff → connecting → live` with
`degraded` as an overlay flag (read-only view + non-empty queue), not a
separate state machine state. One path for unreachable and offline (FR-8):
the connection layer reports both identically. Queue drains only after a
snapshot merge (never before — replay-into-stale-view is how double-writes
happen). Local echo: writes apply to view state immediately, tagged
`pending`; ack replaces the tag; `superseded` reverts the field to the
snapshot/broadcast value silently. Queue storage behind a 3-function
`Storage` interface (browser: `localStorage`, keyed `hireling:queue:{sub}`;
tests: in-memory) so `node --test` runs without a DOM.

## Shutdown (joins E1's drain — no second path)

The server's existing `with_graceful_shutdown` future fires on SIGTERM; the
WS task selects on that signal alongside its socket: send app `bye` frame +
WS Close 1001, flush the outbound channel (bounded, ≤64 frames — drains in
microseconds), exit task. `DRAIN_TIMEOUT` (10 s) already bounds the wait.
Clients see 1001 → degraded mode + FR-10 backoff. No new shutdown registry,
no timeout of our own.

## Testing strategy

- **Unit (Rust)**: CAS outcomes incl. concurrent same-field writes; ledger
  dedupe incl. replay-after-restart (DB-backed, gated on `test_pool()` like
  the existing suites); frame decode deny-by-default; backoff sequence
  generator (deterministic seed) bounds; histogram percentiles.
- **Unit (JS, `node --test`)**: merge strictly-newer only; queue durability
  across `Storage` swap (reload simulation); superseded revert; indicator =
  queue non-empty exactly; account keying isolation.
- **Integration (Rust, ephemeral listener + compose DB, reusing
  `router_for`/`seed_account`/`seed_session`)**: two-plus WS clients —
  broadcast propagation; fan-out isolation (stall one client's reads, assert
  others unaffected, stalled one recovers via snapshot); GM write forbidden;
  six-client restart soak (10 000 ops, exactly-once, SC-3); drain (SIGTERM
  mid-burst → all clients get 1001, zero lost acks).
- **Latency (same harness + shaped proxy)**: FR-5 profiles — wifi
  (50 ms ±20 jitter, 0% loss) and cellular (300 ms ±100, 1%); two clients,
  p95 send→applied asserted <1 s / <3 s; `sync_dispatch_ms` p95 ≤100 ms
  asserted unshaped at POC load (6 clients, 1/s sustained, 5/s bursts).
  Seeded RNG — reproducible CI numbers.

## Alternatives considered

- **LISTEN/NOTIFY or an outbox table for fan-out** — E2 ruled it out
  (Constitution V: nothing alongside Postgres); in-process fan-out on write
  success loses nothing at one process and six users.
- **Delta-log catch-up (replay `version > last_seen` from a log table)** —
  needs a log table (new DDL, new consistency surface) to save bandwidth we
  don't need to save at POC. Snapshot + total order is E2's designed path.
- **Client-side skip of doomed replays** (check base_version against
  snapshot before sending) — duplicates the server's arbitration logic in
  the client; two CAS implementations = one bug farm. Always replay, server
  arbitrates, one code path (grug).
- **Protocol-level ping/pong as the only liveness** — browsers auto-pong
  and hide both sides from page JS; client-side dead-link detection then
  can't exist. App-level frames both directions instead.
- **CRDT/delta-merge for concurrent same-field writes** — the PRD already
  settled the semantics: server-receipt order wins, losers supersede. CAS
  is that rule, exactly, with no merge math.
- **Vite/vitest for the client module tests** — the repo's web tests are
  `node --test`; adding a runner for one module is a dependency, not a win.

## Coordination points (handed onward)

- **E8**: effect write commands ride this transport (`kind:"effect"` write
  frame + effect row semantics); E7 ships propagation + whole-row versioning
  only. E8's spec extends `wire-protocol.md`.
- **E6**: consumes `web/src/lib/sync/` — store subscription, `enqueueWrite`,
  pending/echo tags, `rejected` surfacing, optimistic rollback.
- **E10**: builds the service worker against `contracts/degraded-mode.md`
  and `contracts/wire-protocol.md`; the SW wraps the queue + read-only
  shell, it never re-implements merge or replay.
- **E12**: stash/bank join by adding version columns in its migration
  (mechanism is table-agnostic); `client_ops` needs no change.

## Risks

- **Snapshot size on cellular reconnect** (~300 KB worst case ≈ seconds on
  a bad link) — accepted at POC (one-time cost per reconnect, not per
  write); `snapshot_bytes` logged; delta-log is the documented escape hatch.
- **Ledger growth** — one row per write, no pruning at POC; six users ≈
  thousands of rows/season. Pruning policy (>90 days) is E11 go-live
  checklist material, not code now.
- **`localStorage` eviction under storage pressure** (browser may clear it)
  — known limit; the queue is best-effort durable across reloads, and E10's
  SW storage (persistent grant) is the sanctioned fix. Documented in the
  degraded-mode contract so E10 knows it's load-bearing.
