# Party Sync & Realtime (E7) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One authenticated WebSocket per party with server-broadcast diffs, per-field CAS versioning on E2's columns, offline-tolerant queued writes with idempotent replay, and the measured latency budgets — the transport E6/E8/E10 build on.

**Architecture:** In-process fan-out registry (E2's settled ruling — no LISTEN/NOTIFY, no outbox); writes are compare-and-set per field against `base_version` with a durable `client_ops` ledger for exactly-once replay; catch-up is a full party snapshot merged by strictly-newer global-sequence versions; client is a plain-JS module (connection/store/queue) with one degraded path for offline and unreachable.

**Tech Stack:** Rust/axum (existing; adds the `ws` **feature** to the existing axum dep), sqlx (existing), tokio-tungstenite as **dev-dependency** (design-gate approved; already transitive via axum `ws`), plain JS + `node --test` (existing web shape).

**Spec:** `specs/007-party-sync/spec.md` (approved + clarified), `design.md` (approved), `data-model.md`, `contracts/wire-protocol.md`, `contracts/degraded-mode.md` — all in this directory; this plan argues from them, executors read both.

## Global Constraints

- Branch: `feat/9-party-sync` (exists; Task 1 merges `origin/main` — E5 landed in PR #28 after this branch was cut). **Claim gh#9 (self-assign) before the first code commit.** Conventional commits, `(#9)` suffix, imperative.
- Never commit to `main`; never force-push. `just ci-local` green before PR. PR body: `Closes #9` + link to the Paperclip task + decisions-with-alternatives (from `design.md`).
- Lint law: `unwrap_used = "deny"`, pedantic warns, `unsafe_code = "deny"`. No `#[allow]` — propose lint tuning instead. Name intermediate variables (grug).
- Binding numbers (verbatim from spec/contracts): ping **20 s**, pong timeout **10 s**, client dead-link **50 s**; backoff **1 s ×2 cap 30 s full jitter**, reset only after snapshot+drain; dispatch budget **p95 ≤ 100 ms** at POC load (1 party, 6 clients, 1/s sustained, 5/s bursts); e2e **p95 < 1 s wifi (50 ms ±20 jitter, 0% loss)**, **< 3 s cellular (300 ms ±100, 1%)**; outbound buffer **64** frames; soak **10 000 ops** exactly-once.
- Field set (FR-6): `vitals.hp`, `vitals.temp_hp`, `vitals.money {pp,gp,sp,cp}`, `vitals.level_adjust`, `slot {caster_key,rank,slot_index} → {used?,prepared?}`, `inv {item_name} → {qty_delta}`, `effect` **read-only in E7**.
- Bounds (reject, never silently fix): `hp ≥ 0`, `temp_hp ≥ 0`, money denominations `≥ 0`, `level_adjust ∈ −19..=19`, `rank 0..=10`, `slot_index ≥ 0`; `qty_delta` any int.
- Outcomes: `applied | superseded | already_applied | rejected | forbidden`. Superseded = silent UI revert + server-side log + ledger row. No error theatre; indicator = queue non-empty, nothing else.
- Auth: handshake + **every frame** through E3's `authorize()`; deny-by-default on unknown frame types; GM writes forbidden.
- House rule (PR #30, on main now): tests for observability requirements must drive the production path (real router / real socket), never call the recorder directly.

## File Structure

| File | Responsibility |
|---|---|
| `migrations/20261001000001_client_ops.sql` + `.down.sql` | the one new table |
| `src/sync/protocol.rs` | frame serde types, field targets, outcomes; decode is deny-by-default |
| `src/sync/write.rs` | `apply_write`: bounds → authz → ledger dedupe → CAS → ledger insert, one tx |
| `src/sync/snapshot.rs` | party live-state snapshot with per-field versions (incl. effect rows) |
| `src/sync/registry.rs` | party→connections map, bounded channels, `try_send` fan-out, overflow close |
| `src/sync/session.rs` | WS route, handshake authz, hello/snapshot/ack loop, liveness, drain select |
| `src/sync/metrics.rs` | dispatch timing, ring histogram, `GET /metrics/sync` |
| `src/sync/mod.rs` | module wiring + registry state construction |
| `src/http.rs` (modify) | route registration + `API_ROUTES` declarative entry (ownership matrix) |
| `web/src/lib/sync/queue.js` | durable FIFO op queue behind a Storage interface |
| `web/src/lib/sync/store.js` | last-known state, per-field versions, local echo, pending tags, reverts |
| `web/src/lib/sync/connection.js` | WS wrapper: backoff, liveness watchdog, reconnect sequence |
| `web/src/lib/sync/index.js` | public API (`createSync`), indicator signal, debug-page wiring |
| `tests/sync/*.rs` (crate: `src/tests/sync/`) | integration: broadcast, supersession, soak, drain, latency harness |
| `tests/shaper.rs` (dev) | seeded latency/jitter/loss TCP proxy for the latency harness |
| `web/tests/sync/*.test.js` | node --test units for queue/store/connection |

Rust test layout follows the house shape (`src/tests/` in-crate, DB-gated like `src/tests/db.rs`); JS tests follow `web/tests/import.test.js` (E5's, now on main).

---

### Task 1: Base merge + `client_ops` migration

**Files:**
- Modify: branch `feat/9-party-sync` (merge `origin/main`)
- Create: `migrations/20261001000001_client_ops.sql`, `migrations/20261001000001_client_ops.down.sql`
- Test: `src/tests/migrations.rs` (extend E5's file)

**Interfaces:**
- Produces: table `client_ops(op_id text PK, account_sub text NOT NULL FK accounts(sub) RESTRICT, field_path text NOT NULL, outcome text NOT NULL CHECK IN ('applied','superseded','rejected','forbidden'), resulting_version bigint NULL, created_at timestamptz NOT NULL DEFAULT now())` + index `(account_sub, created_at)`. No `updated_at` (append-only).

- [x] **Step 1: Claim + merge base.** Self-assign gh#9. `git merge origin/main` into `feat/9-party-sync`; resolve (specs/007 touches nothing E5 touched); `just ci-local` green on the merge commit before proceeding.
- [x] **Step 2: Write the failing test** — extend `src/tests/migrations.rs`: apply all migrations to a fresh DB (existing pattern), then assert table shape: `op_id` PK, outcome CHECK rejects `'bogus'`, FK RESTRICT on account delete, and the down migration drops the table cleanly (up→down→up round-trip).
- [x] **Step 3: Run — expect FAIL** (table absent). `cargo test --test none migrations::client_ops` or the file's filter per existing pattern.
- [x] **Step 4: Write the migration** exactly as the Interfaces block; `.down.sql` = `DROP TABLE client_ops;` with the house header comment.
- [x] **Step 5: Run — PASS.** Then `just db-reset && just db-migrate && just db-revert && just db-migrate` (compose round-trip).
- [x] **Step 6: Commit** — `feat: E7 client_ops idempotency ledger migration (#9)`.

### Task 2: Protocol frames, deny-by-default

**Files:** Create `src/sync/protocol.rs`, `src/sync/mod.rs` (empty wiring). Test: in-file `#[cfg(test)]`.

**Interfaces:**
- Produces: `pub enum ClientFrame { Write { op_id: String, target: FieldTarget, base_version: i64, value: JsonValue }, Ping }`; `pub enum ServerFrame { Hello { party_id: i64, you: ActorInfo, server_now: DateTime<Utc> }, Snapshot { fields: Vec<SnapshotField>, snapshot_bytes: u64 }, Diff { field: FieldTarget, value: JsonValue, version: i64, actor_sub: String, op_id: Option<String> }, Ack { op_id: String, outcome: Outcome, version: Option<i64>, winning_version: Option<i64>, reason: Option<String> }, Pong, Bye { reason: String } }`; `pub enum FieldTarget { Vitals { character_id: i64, field: VitalsField }, Slot { character_id: i64, caster_key: String, rank: i32, slot_index: i32 }, Inv { character_id: i64, item_name: String }, Effect { effect_id: i64 } }`; `pub enum Outcome { Applied, Superseded, AlreadyApplied, Rejected, Forbidden }`; `impl ClientFrame { pub fn decode(raw: &str) -> Result<Self, ProtocolError> }`.
- Wire shapes are **exactly** `contracts/wire-protocol.md` §2 (tag `"t"`, snake_case fields).

- [x] **Step 1: Failing tests**: round-trip every frame kind through `serde_json` to the contract's literal JSON (copy the examples from the contract as fixtures — derivation rule); `decode("{\"t\":\"wat\"}")` is `Err`; `decode` of `kind:"effect"` write is `Err(ProtocolError::EffectWritesDeferred)` (deny-by-default, distinct reason); `base_version` missing → `Err`.
- [x] **Step 2: Run — FAIL** (module absent). `cargo test sync::protocol`.
- [x] **Step 3: Implement** serde types with `#[serde(tag = "t", rename_all = "snake_case")]`; `decode` = `serde_json::from_str` + effect-write guard.
- [x] **Step 4: Run — PASS.** `cargo test sync::protocol`.
- [x] **Step 5: Commit** — `feat: E7 wire protocol frames, deny-by-default decode (#9)`.

### Task 3: `apply_write` — bounds, CAS, ledger (the engine's core)

**Files:** Create `src/sync/write.rs`. Test: `src/tests/sync/write.rs` (DB-gated on `testing::test_pool()`).

**Interfaces:**
- Consumes: E2 tables (`character_vitals`, `character_spell_slots`, `character_inventory_live`), `field_version_seq`, Task 1's `client_ops`.
- Produces: `pub async fn apply_write(pool: &PgPool, actor: &Actor, op: ClientOp) -> WriteResult` where `ClientOp { op_id, target, base_version, value }` and `WriteResult { outcome: Outcome, version: Option<i64>, winning_version: Option<i64> }`.

- [x] **Step 1: Failing tests** (seed a party+character+owner via `testing::seed_account` + E5's party seed helpers on main):
  1. owner writes `hp 14` at current version → `Applied`, returned version = old+sequence advance, DB row updated, `client_ops` row `applied` with `resulting_version`.
  2. same op replayed → `AlreadyApplied` with the same version, no second version bump.
  3. stale `base_version` → `Superseded` + `winning_version` = current, DB unchanged, ledger row `superseded`.
  4. `hp = -1` → `Rejected`, reason names the bound; `level_adjust = 25` → `Rejected`.
  5. GM actor → `Forbidden`; non-owner member → `Forbidden` (E3 `authorize()` wired per call).
  6. two concurrent writers, same field, same base (spawn `tokio::join!`) → exactly one `Applied`, one `Superseded`.
  7. slot write `{used:true}` and inv write `{qty_delta:-2}` CAS correctly on their row versions; money write sets all four denominations under one `money_version`.
- [x] **Step 2: Run — FAIL.** `cargo test sync::write`.
- [x] **Step 3: Implement**: validate bounds first (pure fn, table-driven — unit-test it inline); `authorize(actor, Write, resource)`; one transaction: `SELECT outcome… FROM client_ops WHERE op_id=$1 FOR UPDATE` → hit = `AlreadyApplied`; else the CAS statement per target kind — `UPDATE character_vitals SET hp=$1, hp_version=nextval('field_version_seq') WHERE character_id=$2 AND hp_version=$3 RETURNING hp_version` (rowcount 0 → re-read current version → `Superseded`); `INSERT client_ops …`; commit. Every error path is `Result`, no `unwrap`.
- [x] **Step 4: Run — PASS** (all seven).
- [x] **Step 5: Commit** — `feat: E7 per-field CAS write engine + durable ledger (#9)`.

### Task 4: Snapshot builder

**Files:** Create `src/sync/snapshot.rs`. Test: `src/tests/sync/snapshot.rs`.

**Interfaces:**
- Produces: `pub async fn party_snapshot(pool: &PgPool, party_id: i64) -> Result<Vec<SnapshotField>>` — every versioned field of every character in the party, plus effect rows `{effect_id, name, source_character_id, targets, modifiers, duration_note, active, version}`.

- [x] **Step 1: Failing tests**: seeded party with vitals+slots+inventory+two effects → snapshot contains each field with its exact version (read versions straight from the DB as oracle); second party's data absent (party scoping); `snapshot_bytes` = serialized length.
- [x] **Step 2: FAIL** → **Step 3: Implement** (one SELECT per table, all party-scoped, effects joined with targets/modifiers per E2 shapes) → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 party live-state snapshot with per-field versions (#9)`.

### Task 5: Registry + fan-out isolation

**Files:** Create `src/sync/registry.rs`; wire into `src/sync/mod.rs`. Test: `src/tests/sync/registry.rs` (pure tokio, no DB).

**Interfaces:**
- Produces: `pub struct PartyRegistry { … }` with `pub fn subscribe(&self, party_id: i64) -> ConnHandle`, `pub fn unsubscribe(&self, party_id: i64, handle_id: u64)`, `pub fn broadcast(&self, party_id: i64, frame: &ServerFrame) -> FanOutReport` where `ConnHandle { pub rx: mpsc::Receiver<ServerFrame>, id: u64 }` and `FanOutReport { delivered: usize, overflowed: usize }`. Channel capacity **64**.

- [x] **Step 1: Failing tests**: two subscribers receive a broadcast; a subscriber whose channel is full (fill 64, don't drain) is dropped from the registry by the next broadcast (overflow close is the session's job — registry reports it) and the other subscriber still received everything (isolation); no `await` on any send path (compile-level: `broadcast` is sync).
- [x] **Step 2: FAIL** → **Step 3: Implement** `RwLock<HashMap<i64, Vec<ConnHandle>>>`, `try_send`, overflow → remove handle → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 party registry, try_send fan-out, overflow eviction (#9)`.

### Task 6: WS session — route, handshake, write loop

**Files:** Create `src/sync/session.rs`; modify `src/http.rs` (route + `API_ROUTES` entry) and `src/sync/mod.rs`. Dev-dep: `tokio-tungstenite` in `Cargo.toml [dev-dependencies]` with the design-gate justification as comment. Test: `src/tests/sync/session.rs` (real ephemeral listener via `router_for` + `tokio::spawn(axum::serve)`; sessions via `testing::seed_session`).

**Interfaces:**
- Consumes: Tasks 2–5; E3 middleware Actor extension.
- Produces: route `GET /api/ws/party/{party_id}`; on upgrade: `hello` then `snapshot`; write frames → `apply_write` → ack (+ broadcast of `diff` on `applied`); `ping`→`pong`.

- [x] **Step 1: Failing tests** (drive the real router over a real socket — PR #30 rule):
  1. member connects to own party → `hello` + `snapshot`; outsider → HTTP 403 pre-upgrade; GM connects → `hello`/`snapshot` arrive.
  2. member writes hp → ack `applied` + every connected client (2nd socket) receives `diff` with version and `actor_sub`; writer receives own diff too.
  3. GM write → ack `forbidden`, no diff.
  4. garbage frame → connection stays open, frame ignored (assert via subsequent successful ping/pong); `kind:"effect"` write → ignored likewise.
- [x] **Step 2: FAIL** → **Step 3: Implement** handshake authz (owner-in-party or GM), the select loop (inbound socket | outbound rx | shutdown), `bye`+Close on drain paths, Close 1013 on rx-closed (overflow), liveness timer armed here. **Step 4: PASS.**
- [x] **Step 5: Commit** — `feat: E7 party WS session, handshake authz, write/ack/diff loop (#9)`.

### Task 7: Liveness + drain numbers

**Files:** Modify `src/sync/session.rs` + `src/sync/mod.rs` (config: `SyncSettings { ping_interval, pong_timeout }` overridable in tests). Test: extend `src/tests/sync/session.rs`.

- [x] **Step 1: Failing tests** (short intervals, e.g. 100 ms/50 ms): server pings at interval; a client that never pongs is closed (1011) within timeout; well-behaved client stays connected across several cycles; drain signal mid-connection → client receives `bye` then Close 1001 (assert frame + close code) — drive the drain via the same shutdown channel `http.rs` uses, not a second path.
- [x] **Step 2: FAIL** → **Step 3: Implement** (`tokio::time::interval`, `select!` on shutdown broadcast receiver from the server's `with_graceful_shutdown` wiring — reuse, do not create a new signal) → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 liveness ping/pong + drain joins E1 shutdown (#9)`.

### Task 8: Dispatch metrics

**Files:** Create `src/sync/metrics.rs`; modify `src/http.rs` (`GET /api/me`-adjacent `/metrics/sync` + `API_ROUTES`) and session/write wiring (t0 at frame receipt, t1 post-fan-out). Test: `src/tests/sync/metrics.rs`.

**Interfaces:**
- Produces: `pub struct SyncMetrics { … }` (`push(ms)`, `snapshot() -> MetricsSummary { count, p50_ms, p95_ms, p99_ms }`, 10 000-sample ring); endpoint returns the JSON summary behind session auth.

- [x] **Step 1: Failing tests**: histogram percentiles on a known sample set (e.g. 1..=100 → p50=50, p95=95, p99=99); endpoint: unauthenticated → 401; authenticated → shape + auth route listed in the ownership matrix (extend the existing router test). Log assertion: a write over the real socket produces a `sync_dispatch_ms` tracing event (capture via `tracing` test subscriber — production path per PR #30).
- [x] **Step 2: FAIL** → **Step 3: Implement** ring + percentile; wire t0/t1; route. → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 sync_dispatch_ms instrumentation + /metrics/sync (#9)`.

### Task 9: Client queue

**Files:** Create `web/src/lib/sync/queue.js`. Test: `web/tests/sync/queue.test.js`.

**Interfaces:**
- Produces: `export function createQueue({ storage, accountSub })` → `{ enqueue(op), peekAll(), dequeue(opId), length, onChange(cb) }`; `op = { op_id, target, base_version, value, created_at }`; storage key `hireling:queue:{accountSub}`; FIFO order; unbounded; no drop paths.

- [x] **Step 1: Failing tests** (in-memory storage fake): enqueue preserves FIFO; persists through fake "reload" (new queue, same storage); per-account isolation (different sub → different key); `onChange` fires on enqueue/dequeue.
- [x] **Step 2: FAIL** → **Step 3: Implement** (JSON array in storage; storage interface = `{getItem,setItem}` so `node --test` needs no DOM) → **Step 4: PASS** (`node --test web/tests/sync/queue.test.js`).
- [x] **Step 5: Commit** — `feat: E7 durable per-account write queue (#9)`.

### Task 10: Client store + merge + echo/revert

**Files:** Create `web/src/lib/sync/store.js`. Test: `web/tests/sync/store.test.js`.

**Interfaces:**
- Produces: `export function createStore()` → `{ state(), applyServerField(target, value, version), enqueueView(op), ack(opId, outcome, serverVersion, serverValue), isSyncing(), subscribe(cb) }`; merge rule strictly-newer per field; pending tags keyed by `op_id`; superseded/rejected/forbidden revert the field from the pending op to the server value silently; `isSyncing()` = queue length > 0.

- [x] **Step 1: Failing tests**: older version ignored (state unchanged); newer applied; own-write echo (diff with own op's version) is a no-op after ack applied; superseded ack reverts field to server value and clears pending — and emits **no error event** (assert the event bus stays silent); rejected ack reverts and *does* expose the op record for E6; two devices' interleavings converge regardless of arrival order.
- [x] **Step 2: FAIL** → **Step 3: Implement** → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 client store — strictly-newer merge, echo, silent revert (#9)`.

### Task 11: Connection + backoff + reconnect sequence

**Files:** Create `web/src/lib/sync/connection.js`. Test: `web/tests/sync/connection.test.js`.

**Interfaces:**
- Produces: `export function createConnection({ url, socketFactory, rng, now, timers })` → `{ connect(), send(frame), state(), onChange(cb) }`; backoff = `delay = rng() * min(30000, 1000 * 2**attempt)`; watchdog: no inbound frame for **50 s** → treat dead; reconnect sequence emits `snapshot→merge→drain-queue→live` phases; state `connecting|live|offline` with `offline` covering unreachable and browser-offline identically (one path).

- [x] **Step 1: Failing tests** (mock socket + fake timers): backoff sequence respects cap and jitter bounds across 10 attempts; reset-to-zero only after drain completion (assert next delay after full cycle = base); 50 s silence triggers reconnect; offline event and socket error produce the **same** state transition; queue drains only after snapshot phase marker.
- [x] **Step 2: FAIL** → **Step 3: Implement** → **Step 4: PASS**.
- [x] **Step 5: Commit** — `feat: E7 client connection — jittered backoff, watchdog, one-path offline (#9)`.

### Task 12: Public API + indicator + debug page

**Files:** Create `web/src/lib/sync/index.js`, `web/src/lib/sync/SyncDebug.svelte`; modify `web/src/App.svelte` (internal route, e.g. `#/sync-debug`). Test: `web/tests/sync/index.test.js` + `just web-check`.

**Interfaces:**
- Produces: `export function createSync({ url, storage, accountSub })` composing Tasks 9–11: `{ state(), write(target, value), subscribe(cb), snapshotForBoot() }`; `isSyncing` derives from queue length only. Debug page: connect/disconnect buttons, queue view, live field table, indicator mock — internal tool, not product UI.

- [x] **Step 1: Failing tests**: `write()` enqueues + echoes optimistically and survives fake reload; indicator true iff queue non-empty (empty queue + dead socket = false); `snapshotForBoot()` serializes merged state (E10's cold-boot input). **Step 2: FAIL → Step 3: Implement → Step 4: PASS** (`node --test web/tests/sync/` then `just web-check`).
- [x] **Step 5: Commit** — `feat: E7 sync public API, indicator signal, debug page (#9)`.

### Task 13: Integration suite — broadcast, supersession, soak, restart

**Files:** Create `src/tests/sync/integration.rs` (reuses the Task 6 harness). No new production code expected — this task is the spec's proof.

- [x] **Step 1: Failing-first where behavior is new, passing-now where it verifies**: (a) offline queue story over the wire — client A writes while socket severed, server state moves via client B, A reconnects → snapshot merge, queue replay, superseded ack, UI-silent (store assertion), server log/ledger row present; (b) ack-loss replay — kill the connection between commit and ack delivery, reconnect, resend same `op_id` → `already_applied`, exactly one version bump total; (c) **soak**: 6 clients, 10 000 ops (1/s sustained, 5/s bursts interleaved), then server **restart** (drop + recreate listener/pool in-process) → zero lost acknowledged writes, every op exactly-once in `client_ops`, all clients converged (SC-3); (d) stalled-reader isolation from Task 5 now over real sockets.
- [x] **Step 2–4**: run, fix what the suite catches (failures here are findings, not test changes — the tests encode the spec), converge to green.
- [x] **Step 5: Commit** — `test: E7 integration — offline replay, ack-loss, 10k soak + restart (#9)`.

### Task 14: Latency harness — the measured numbers

**Files:** Create `tests/shaper.rs` (dev-only TCP proxy: seeded latency ± jitter, loss%; ~120 lines tokio) and `src/tests/sync/latency.rs`.

- [x] **Step 1: Failing tests**: (a) unshaped, POC load (6 clients, 60 s at 1/s + 10 s bursts at 5/s): dispatch p95 ≤ **100 ms** from `/metrics/sync`; (b) two clients through shaper profiles — wifi `{50ms, ±20, 0%}` → send→applied p95 < **1 s**; cellular `{300ms, ±100, 1%}` → p95 < **3 s** (1 000 samples each, seeded RNG, p95 from the test's own measurements; budgets are the spec's FR-5); (c) `snapshot_bytes` logged.
- [x] **Step 2–4**: implement shaper + harness; run; tune nothing in the budgets — if a budget fails, that's a stop-and-report, not a threshold edit.
- [x] **Step 5: Commit** — `test: E7 latency harness — dispatch p95 + shaped e2e budgets (#9)`.

### Task 15: Final gate + PR

- [ ] `just ci-local` green end-to-end; paste output in the PR body. Update `specs/007-party-sync/plan.md` checkboxes as executed (this file lives in-repo — check off as you go).
- [ ] PR from `feat/9-party-sync` → `main`: body carries `Closes #9`, link to the Paperclip task (MOR-27), decisions-with-alternatives table copied from `design.md`, verification output (soak, latency, ci-local), and the two gate-blessed deviations (`tokio-tungstenite` dev-dep; E8-deferred effect writes).
- [ ] Orsik's independent review is the acceptance path (separate task); a human merges.

---

## Self-review (run by the plan author)

- **Spec coverage**: FR-1→T6, FR-2→T2/T3/T6, FR-3→T5/T6, FR-4→T8/T14, FR-5→T14, FR-6→T2/T3, FR-7→T4/T6/T10, FR-8→T1/T3/T9/T13, FR-9→T10/T12, FR-10→T7/T11, FR-11→T7, FR-12→contracts (shipped at design step)+T12 (boot snapshot), FR-13→T4 (table-agnostic queries)/E12 note. SC-1→T8/T14, SC-2→T14, SC-3→T13, SC-4→T3/T10/T13, SC-5→contracts docs exist + T12 exposure. Edge cases: echo race→T6/T10, ack-loss→T13, logout-with-queue→T9/T11, backgrounded tab→T11 (event-driven flush), solo party→T13 degenerate case added there.
- **Placeholder scan**: none — every code step names files, signatures, and assertions.
- **Type consistency**: `ClientOp`/`WriteResult` (T3) vs session loop (T6); `ServerFrame` variants (T2) used by registry (T5) and session (T6); JS `op` shape (T9) matches `ClientFrame::Write` payload (T2) field-for-field.
