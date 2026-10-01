# Data Model: Party Sync & Realtime (E7)

Companion to [`design.md`](design.md); implements [`spec.md`](spec.md).
E2 owns every existing table — this document adds exactly **one** table,
E7's first owned migration. No E2 column changes. No triggers, no sequences
(`field_version_seq` already exists and remains the only version source).

**Migration:** `migrations/20261001000001_client_ops.sql` (+ `.down.sql`,
reversible per E2's convention). Date-stamped at implementation; the name
here is the committed intent.

## `client_ops` — the durable idempotency ledger

One row per processed client write operation, written **in the same
transaction** as the field write it records. The replay-dedupe key for
ack-lost writes (spec FR-8): a client that never saw its ack resends the
same `op_id`; the server answers from this table without re-writing.

| column | type | constraints | notes |
|---|---|---|---|
| `op_id` | text | **PRIMARY KEY** | client-generated UUID per operation; globally unique by construction (v4) — collisions are a client bug the PK catches loudly. The id is **reserved through the PK before the CAS** in the same transaction: a concurrent twin blocks on the reservation and answers from the holder's row — reuse for a different request is rejected, never a second committed write (review hardening, PR #34) |
| `account_sub` | text | NOT NULL, FK → `accounts(sub)` ON DELETE RESTRICT | who sent it; a replayed op under a different account is not this table's problem (authz rejects before the ledger is consulted) — the FK is provenance, not enforcement |
| `field_path` | text | NOT NULL | canonical serialized field target (e.g. `vitals:hp`, `slot:Wizard:3:0`, `inv:Chalk`, `effect:42`) — for humans reading the table and for the supersede log correlation; the wire format stays structured JSON |
| `request` | jsonb | NOT NULL | the exact client request (`{target, base_version, value}`) — what tells a genuine replay apart from reuse of the id for a different request; reuse is refused (`rejected` ack, nothing committed, the holder's row standing) |
| `outcome` | text | NOT NULL, CHECK IN (`applied`,`superseded`,`rejected`,`forbidden`) | terminal disposition of the op; `already_applied` is never stored (it is a read answer, not a new outcome) |
| `resulting_version` | bigint | NULL | the field version this op produced; NULL for `superseded`/`rejected`/`forbidden` (nothing was written) — the ack for `already_applied` replays this value |
| `created_at` | timestamptz | NOT NULL DEFAULT now() | ledger age; no `updated_at` — rows are append-only |

**Indexes:** PK on `op_id` (the dedupe path); secondary index on
`(account_sub, created_at)` — the pruning/inspection query path for E11's
go-live checklist. Nothing else: no party_id (derivable from the account's
character), no FK to fields.

**Write discipline:** inserted once, never updated, never deleted by app
code at POC. `superseded` rows are the server-side supersession record the
spec requires — the structured log line is the operational view; this table
is the durable one (logs rotate, tables don't).

**Down migration:** `DROP TABLE client_ops` — losing the ledger costs
idempotency for in-flight offline queues only; no other table references it.

## What deliberately does NOT change

- **E2's live tables** — version columns exist, fed by `field_version_seq`;
  E7's CAS statements consume them (`WHERE … AND version = $base`).
- **`field_version_seq`** — remains the single global sequence. E7's writes
  call `nextval` in the same transaction as the field write, per E2 design §4.
- **`party_bank` / `party_stash`** — untouched at P0. E12 opts stash in by
  adding its version columns; the sync mechanism is table-agnostic by design
  (spec FR-13) and needs zero schema awareness of them until then.

## Sizing note

One row per write. Six users at table cadence ≈ low thousands of rows per
campaign season — a non-problem at POC. Pruning policy (if ever) belongs to
E11's operational checklist, decided with real numbers, not guessed now.
