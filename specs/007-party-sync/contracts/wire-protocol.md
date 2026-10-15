# Contract: Party Sync Wire Protocol (E7)

**Status**: binding for E7 implementation · amended by E8 (effect write
frames + the `derived` frame, PR gh#10, 2026-10-01) · amended by E12 (the
party stash/bank/settings kinds + the additive apply class, 2026-10-14) ·
build-target for E10
(service worker) · extends `spec.md` FR-1..FR-7, FR-10, FR-11.

Transport, framing, addressing, ordering, and failure semantics of the party
WebSocket. The degraded-mode behavior built on top of it is
[`degraded-mode.md`](degraded-mode.md). If code and this document disagree,
this document wins until a PR changes it.

## 1. Transport & session

- `GET /api/ws/party/{party_id}` — WebSocket upgrade inside E3's
  session-protected nest. The session cookie rides the upgrade; no session →
  HTTP 401 before upgrade. No query-string tokens, ever.
- **Handshake authorization**: the upgrading account must (a) own a
  character in `{party_id}`, or (b) hold the GM role (read-only
  subscription; at POC one party exists, GM may join it). Otherwise 403 —
  the socket never opens.
- **Per-message authorization** (deny-by-default): every inbound frame is
  authorized through E3's `authorize()` before any other handling. The GM
  and non-owner members can send only the frames marked *any* below.
- One party per socket. Multi-party clients open one socket per party (the
  POC UI never does; the rule keeps the protocol party-scoped).

## 2. Framing

JSON **text** frames, one object per frame, tagged by `"t"`:

```jsonc
// client → server
{"t":"write","op_id":"<uuid4>","target":{"kind":"vitals","character_id":3,
 "field":"hp"},"base_version":1042,"value":14}
{"t":"write","op_id":"<uuid4>","target":{"kind":"slot","character_id":3,
 "caster_key":"Wizard","rank":3,"slot_index":0},"base_version":871,
 "value":{"used":true}}
{"t":"write","op_id":"<uuid4>","target":{"kind":"inv","character_id":3,
 "item_name":"Chalk"},"base_version":990,"value":{"qty_delta":-2}}
{"t":"ping"}                                    // any authenticated conn
{"t":"pong"}                                    // client → server: answer to the server's liveness ping

// server → client
{"t":"hello","party_id":1,"you":{"sub":"…","role":"player"},
 "server_now":"2026-10-01T19:04:05Z"}           // first frame after upgrade
{"t":"snapshot","fields":[ /* every field row, shape = diff + value */ ],
 "snapshot_bytes":184223}
{"t":"diff","field":{ …target… },"value":14,"version":1043,"actor_sub":"…",
 "op_id":"<uuid4>"}                             // broadcast on applied writes
{"t":"ack","op_id":"<uuid4>","outcome":"applied","version":1043}
{"t":"ack","op_id":"<uuid4>","outcome":"superseded","winning_version":1045}
{"t":"ack","op_id":"<uuid4>","outcome":"already_applied","version":1043}
{"t":"ack","op_id":"<uuid4>","outcome":"rejected","reason":"hp must be ≥ 0"}
{"t":"ack","op_id":"<uuid4>","outcome":"forbidden","reason":"gm is read-only"}
{"t":"ping"}                                    // server → client: liveness probe (§5)
{"t":"pong"}
{"t":"bye","reason":"shutdown"}                 // precedes Close 1001 on drain
```

Unknown `"t"`, malformed JSON, or a write to a field kind the protocol does
not define (at P0: any `kind:"effect"` write) → frame dropped with a
structured log entry; repeated abuse → Close 1003. Drops are silent on the
wire — the client's queue treats a dropped write as never-acked (it retries
per `degraded-mode.md`, and the op ledger makes that safe).

## 3. Field addressing (the sync field set — spec FR-6, settled)

| `kind` | key fields | value shape | version unit |
|---|---|---|---|
| `vitals` | `character_id`, `field`: `hp` \| `temp_hp` \| `money` \| `level_adjust` \| `focus_current` \| `hero_points` \| `daily` \| `shield_raised` | `hp`/`temp_hp`: int ≥0; `money`: `{pp,gp,sp,cp}` (absolute, all four); `level_adjust`: int −19..19; `focus_current`: int ≥0 (max is the character's focus max — client clamps, server validates ≥0 only); `hero_points`: int ≥0; `daily`: `{staff_charge_rank: 0..10, staff_spent: int ≥0, drain_used: bool}` (whole-row write, one version unit); `shield_raised`: bool (gh#82; snapshot carries `null` = never toggled live — the sheet's own `ac.shieldRaised` governs; an applied write fans out the character's recompute) | the column's `*_version` |
| `slot` | `character_id`, `caster_key` (text), `rank` (0..10), `slot_index` (≥0) | `{used?: bool, prepared?: string\|null}` — whole-slot write | the slot row's `version` |
| `inv` | `character_id`, `item_name` (text, exact match — E5 owns matching semantics) | `{qty_delta: int}` (absolute delta value, signed) | the row's `version` |
| `effect` | `effect_id` (globally unique — identity PK) | appears in `snapshot`/`diff` as `{name, source_character_id, targets[], modifiers[], duration_note, active}` (+ `tracked_manually` since E8); write frames **realized by E8** (below) | `effects.version` (whole row) |
| `effect_new` | `party_id` (E8 create only) | no diff value — the create mints the row; every diff addresses the new `effect` id | n/a (identity PK mint) |
| `stash_new` | `party_id` (E12 mint only) | `{item_name, quantity>0, bulk?, notes?}` — no diff value: the mint creates the row, every later diff addresses the `stash` name; a duplicate exact name is `rejected` with FR-1's reason (the unique constraint is the structural identity), and the apply creates the row plus its `outside → stash` claim row | mint (identity — no CAS) |
| `stash` | `party_id`, `item_name` (byte-exact — the `inv` keying rule) | `{op:"qty", qty_delta}` (additive — §4), `{op:"meta", bulk?, notes?}` (absolute, CAS on the row version), `{op:"transfer", direction:"in"\|"out", qty>0, character_id}` (additive, one claim row, both sides move), or `{op:"sell", qty>0, price:{pp?,gp?,sp?,cp?} with at least one positive denomination}` (additive draw + bank credit + one `stash → bank` claim row; pp converts to gp at 1 pp = 10 gp); appears in `snapshot`/`diff` as `{item_name, quantity, bulk, notes}` — a draw to exactly zero deletes the row and fans a removal diff `value:null` (§4) | the row's `version` |
| `bank` | `party_id` | `{gp_delta, sp_delta, cp_delta}` — signed, additive (§4); per-denomination non-negative enforced, refusal is `rejected` whole; every movement appends one `bank adjustment` claim row (coins = the signed amounts, direction by sign); appears in `snapshot`/`diff` as `{gp, sp, cp}` | the balance row's `version` |
| `party` | `party_id`, `field`: `quartermaster` | `{character_id: integer\|null}` — absolute write under CAS; the character must be roster; appears in `snapshot`/`diff` as `{quartermaster_character_id}` | `quartermaster_version` |
| `claim` | `claim_id` | read-only — appears in `snapshot`/`diff` as the whole row `{item_name, quantity, origin, destination, actor_sub, created_at, coins_gp?, coins_sp?, coins_cp?}` (coins NULL on item rows); no write frames: the log is append-only (E12 FR-2.4), appends ride the diff mechanism keyed by the monotonic row id | the row id (merge-by-id — the id in the version slot makes the strictly-newer rule and the id rule the same predicate) |

`character_id` names the row's owner explicitly: a party snapshot spans every
member's fields, so a target must identify its row unambiguously (the E2
schema keys vitals/slots/inventory per character). Effects are the exception —
`effect_id` is already globally unique.

`caster_key` disambiguates the export's overlapping caster blocks (E2's
anchoring rule). Item identity is exact-name; anything fuzzier is E5's.

## 4. Semantics

- **Ordering**: the global `field_version_seq` is the only order. Diffs and
  snapshot rows carry per-field versions; a client applies a value **only if
  its version is strictly newer** than the last applied for that field. Own
  write echo (`diff` for your own op) dedupes by the same rule.
- **Write = CAS on `base_version`**: applied if the field is still at
  `base_version`; `superseded` if it moved. The writer's own ack is
  authoritative for its op; a matching `diff` may arrive before or after it.
- **Additive applies (E12)**: `stash` `qty`/`transfer`/`sell`, the sell's
  bank credit, and `bank` writes are additive deltas. The server ignores
  `base_version` for them (the frame still carries it — uniform shape) and
  applies the delta under the row lock, serialized by arrival order.
  `superseded` is unreachable for additive writes — their outcomes are
  `applied` / `already_applied` / `rejected` / `forbidden` only. CAS
  remains for the absolute fields (`stash` `meta`, `party`). An impossible
  delta (underflow, overdraft) is `rejected` whole with a human-readable
  reason; nothing partial applies. The compound ops (`transfer`, `sell`)
  and the `bank` adjust each append exactly one claim row in the same
  transaction — a refusal rolls the claim row back with the field changes,
  so a refused movement leaves no claim row. A `stash` draw that lands
  exactly on zero deletes the row (FR-1) and fans a **removal diff** —
  `value:null` on the `stash` target with a fresh sequence version — so
  every seat drops the row under the strictly-newer rule.
- **Version-slot number spaces (E12)**: claim row ids and
  `field_version_seq` values are distinct number spaces that share the wire
  version slot (a claim diff carries the row id in that slot — merge-by-id).
  Nothing compares versions across targets today, and nothing may: a global
  version cursor that compared them would silently break claim merges.
- **Idempotency**: `op_id` is the dedupe key. Same `op_id` twice → second
  gets `already_applied` with the original's version, no re-write, no
  re-broadcast. Durable across server restart (the `client_ops` ledger).
- **Exactly-once fan-out**: a diff exists iff a write committed. No diff for
  `superseded`/`rejected`/`forbidden`/`already_applied`.
- **Bounds** (validated pre-CAS, ack `rejected`): §3's value constraints;
  unknown target (no such character/slot/item under the writer's ownership)
  → `rejected` with reason `unknown target`.

## 5. Liveness & reconnect (numbers are binding)

- Server sends `ping` every **20 s** (the app-level `{"t":"ping"}` frame,
  so the client's any-frame watchdog sees it — design.md: protocol-level
  WS pings are invisible to page JS); a conn without `pong` (`{"t":"pong"}`)
  within **10 s** is closed (1011). Client declares the link dead after
  **50 s** of silence (any frame counts) and enters reconnect.
- Reconnect: **silent**, exponential backoff **1 s ×2, cap 30 s, full
  jitter** (`delay = random() × min(30s, 2^n s)`), reset to n=0 only after
  `snapshot` merge **and** queue drain complete.
- Sequence after upgrade: `hello` → `snapshot` → client merges (strictly
  newer) → client replays queued writes (always — the server arbitrates) →
  live. Acks during replay are processed identically to live acks.

## 6. Close codes

| code | meaning | client behavior |
|---|---|---|
| 1000 | server closed deliberately post-drain | reconnect per backoff |
| 1001 | server draining (E1 shutdown path, after `bye`) | degraded mode + backoff |
| 1011 | liveness timeout / internal error | backoff (no user signal) |
| 1013 | outbound buffer overflow (this client too slow) | reconnect immediately; snapshot restores state |
| 4001..4999 | reserved for E8/E10 extensions | not used by E7 |

## 7. Metrics

- Per applied write: structured log `sync_dispatch_ms`, `party_id`,
  `op_id`, `field`. Per snapshot: `snapshot_bytes`.
- `GET /metrics/sync` (session-gated): `{count, p50_ms, p95_ms, p99_ms}`
  from a 10 000-sample in-process window. Budgets: dispatch p95 ≤ 100 ms at
  POC load; e2e budgets are verified by the shaped-link integration test
  (spec FR-5), not by production client instrumentation.

## 8. Extension points (owned by later epics, changes via PR to this file)

- **E8 — REALIZED (2026-10-01, gh#10)**: effect write frames + effect
  lifecycle semantics are in the protocol:
  - **Write frames** ride the ordinary `write` frame (§2) with the §3
    targets above and per-op values, authorized per message (creator-only;
    the GM is denied), deduped by `op_id` through the ledger, CAS on
    `effects.version`:
    - create — `target: {kind:"effect_new", party_id}` (CAS-free; the
      identity PK mints the row), `value: {op:"create", name (1..=120),
      source_character_id, targets[], modifiers[] (≤16, closed vocabulary,
      values −50..=50), duration_note, corpus_entry_id, condition_value}`.
      A corpus-sourced create (`corpus_entry_id` set) carries NO inline
      modifiers: the stored corpus mappings resolve at apply
      (`condition_value × polarity`, signed) and freeze on the row;
      display-only conditions freeze `tracked_manually=true` with zero
      modifiers.
    - update (retarget) — `target: {kind:"effect", effect_id}`,
      `base_version` = the row version, `value: {op:"update", targets[]}`.
    - end — `value: {op:"end"}`; `active=false`, row retained.
    - Outcomes are E7's five (`applied|superseded|already_applied|rejected|
      forbidden`), reasons human-readable.
  - **Diff echo**: an applied effect op fans out the RESOLVED whole row as
    its `diff` (a corpus create emits the applied signed modifiers).
  - **New server frame `derived`** (E8): `{"t":"derived","character_id":N,
    "output": {…EngineOutput…}}` — fans out after the `diff` that caused
    it, one frame per affected character; the `snapshot` frame gains a
    `derived: [EngineOutput per roster character]` array, so a reconnecting
    client loses nothing by skipping missed frames. Shape per
    `specs/008-buff-effect-engine/contracts/engine-output.md`.
  - Close-code reservations below are unchanged.
- **E10**: service-worker behaviors (offline shell, cross-session cache)
  wrap this protocol; the SW never invents frames.
- **E12 — REALIZED (2026-10-14, lane L2)**: the party stash, bank, and
  quartermaster designation joined the protocol — §3's `stash_new` /
  `stash` / `bank` / `party` write rows and the read-only `claim` row
  (appends ride the ordinary diff mechanism, keyed by row id). Additive
  applies (`stash` `qty`, `bank`) ignore `base_version` and are never
  `superseded` (§4's additive class); `stash` `meta` and `party` are CAS;
  `stash_new` mints under the party-scoped unique name identity. Write
  authorization is per message through E3's matrix with the party-scoped
  resources; denials audit under `forbidden_stash_write` /
  `forbidden_bank_write` / `forbidden_party_write`, the GM's under the
  existing `forbidden_gm_write`. Outcomes are E7's five; the mechanism is
  unchanged. Close-code reservations below are unchanged.
- **E12 — REALIZED (2026-10-15, lane L3)**: the compound ops joined the
  stash write path — §3's `transfer` and `sell` value shapes (additive, one
  claim row each, pp→gp conversion at 1 pp = 10 gp cited in `write.rs` off
  Player Core pg. 267) and the `bank` adjust's `bank adjustment` claim row
  (FR-4: every movement appends exactly one row; FR-9: one transaction).
  The sell routes through the Bank resource (QM-only, D4) — a non-QM
  member's sell denies under `forbidden_bank_write`, not the stash's member
  gate. Outcomes are E7's five; the mechanism is unchanged.
