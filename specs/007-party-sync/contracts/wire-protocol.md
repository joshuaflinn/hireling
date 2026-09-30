# Contract: Party Sync Wire Protocol (E7)

**Status**: binding for E7 implementation · build-target for E8 (effect
writes) and E10 (service worker) · extends `spec.md` FR-1..FR-7, FR-10, FR-11.

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
| `vitals` | `character_id`, `field`: `hp` \| `temp_hp` \| `money` \| `level_adjust` | `hp`/`temp_hp`: int ≥0; `money`: `{pp,gp,sp,cp}` (absolute, all four); `level_adjust`: int −19..19 | the column's `*_version` |
| `slot` | `character_id`, `caster_key` (text), `rank` (0..10), `slot_index` (≥0) | `{used?: bool, prepared?: string\|null}` — whole-slot write | the slot row's `version` |
| `inv` | `character_id`, `item_name` (text, exact match — E5 owns matching semantics) | `{qty_delta: int}` (absolute delta value, signed) | the row's `version` |
| `effect` | `effect_id` (globally unique — identity PK) | **read-only in E7** — appears in `snapshot`/`diff` as `{name, source_character_id, targets[], modifiers[], duration_note, active, version}`; write frames are E8's extension point | `effects.version` (whole row) |

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
- **Idempotency**: `op_id` is the dedupe key. Same `op_id` twice → second
  gets `already_applied` with the original's version, no re-write, no
  re-broadcast. Durable across server restart (the `client_ops` ledger).
- **Exactly-once fan-out**: a diff exists iff a write committed. No diff for
  `superseded`/`rejected`/`forbidden`/`already_applied`.
- **Bounds** (validated pre-CAS, ack `rejected`): §3's value constraints;
  unknown target (no such character/slot/item under the writer's ownership)
  → `rejected` with reason `unknown target`.

## 5. Liveness & reconnect (numbers are binding)

- Server sends `ping` every **20 s**; a conn without `pong` within **10 s**
  is closed (1011). Client declares the link dead after **50 s** of silence
  (any frame counts) and enters reconnect.
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

- **E8**: `kind:"effect"` write frames + effect lifecycle semantics.
- **E10**: service-worker behaviors (offline shell, cross-session cache)
  wrap this protocol; the SW never invents frames.
- **E12**: new versioned field kinds (stash, bank) join §3's table plus
  new rows here; the mechanism is unchanged.
