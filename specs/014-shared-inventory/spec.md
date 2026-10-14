# Feature Specification: Shared Inventory — Party Stash, Claim History, Sell & Party Bank (E12)

**Epic**: E12 — Phase 3, P1 (cuttable table-convenience) · depends on E10 (closed), E4 (closed) · GitHub issue #14
**Created**: 2026-10-14
**Status**: Specify gate run 2026-10-14 (Korrin, front half of the SDD pipeline — docs only, no code in this pass); clarify answers folded in the same day; design signed by Korrin same day. All five clarify decisions settled in place (Clarify Log below) — none moves the P0/P1/P2 line.
**Input**: `docs/EPICS.md` Epic E12 (specify prompt + AI Guardrails); PRD v3.6 FG4 (Shared Inventory), US-5, Party Screens ("Stash (P1)"), Non-Goals; E7 `specs/007-party-sync/spec.md` (FR-6 version granularity, FR-13 extensibility) + `contracts/wire-protocol.md` (§3 field table, §8 E12 extension point) + `contracts/degraded-mode.md` (§4 outcomes); E2 `specs/002-database-schema/data-model.md` §6 and merged migrations (`20260924000005_shared_inventory.sql` — `party_stash`, `claim_history`, `party_bank` shipped unused; `20260924000001` — `parties.quartermaster_character_id`); E4 `specs/004-rules-corpus-importer/contracts/foundry-packs.md` (the `system.price.value` path, its emptiness cases, and the standing note that E12 is the price consumer); merged tree: `src/sync/write.rs` (`cas_inv` — the absolute-set ledger ruling), `src/auth/authz.rs` (the ownership matrix), `web/src/lib/sheet/bulk.js` (Bulk formatting), `web/src/lib/party/` (the party surface this epic extends).

---

## Position

E2 shipped the shared-inventory tables three phases ago and left them unused
on purpose; E4 shipped the items corpus with prices riding in
`corpus_entries.data`; E7 shipped the sync mechanism and named this epic as
its first extension customer ("E12 opts the stash/bank surfaces in by adding
version columns in its own migration… with no change to the sync engine
itself" — E7 FR-13). E10 shipped the party surface the stash screen joins.
E12 activates all of it: **the party stash (a shared loot list), live
transfers between characters and the stash, an append-only claim history,
quartermaster-only sell with corpus-resolved book value, and the party bank
(gp/sp/cp ledger)** — every surface live-synced through the existing party
WebSocket, offline-queued and replayed exactly once.

The PRD's own priority note governs the epic's weight: this is
*table-convenience P1 — serves the loot workflow, not the two friction
taxes. Cuttable without touching the validation hypothesis.* Nothing here
touches the engine, the wire mechanism, or the schema's existing tables
except by this epic's own migration (version columns, one coins extension,
one audit-values constraint swap).

The two decisions the specify prompt explicitly deferred here —
**reconciliation granularity for stash/bank fields against E7's per-field
versioning**, and **the testable form of "idempotent-safe against replayed
offline writes"** — are settled in FR-2 and FR-8, not handed back.

## User Scenarios & Testing *(mandatory)*

E12's users are the six party accounts. Two roles matter: any **character
owner** (claims and transfers are open to all of them) and the **quartermaster**
—one designated character, whose owner alone sells and adjusts the bank. The
GM reads everything and writes nothing, as ever.

### User Story 1 — The night's loot dump (Priority: P1) 🎯 MVP

Session ends. Becky opens the stash from the party screen and adds what the
party found — a name, a quantity, maybe Bulk and a note — while Jake moves
the ropes he's been carrying from his sheet to the stash. Both sides update
live on every connected screen: the stash list grows, Jake's inventory
row drops, and one claim-history line records each movement with who did
it. Nobody refreshes anything.

**Why this priority**: US-5's first half — "dump the night's loot into a
shared stash… so the party loot list stops living in a group chat." The
list with live updates and the log underneath it is the feature's minimum
viable core; sell and bank are the payoff on top of it.

**Independent Test**: Two clients in one party session; client A adds an
item to the stash; client B moves an item from a character to the stash;
assert both screens show both changes without reload, the character's
inventory reflects the move, and the claim history shows two rows naming
the actors.

**Acceptance Scenarios**:

1. **Given** any character owner connected to the party session, **When**
   they add an item (name, quantity, optional Bulk and note) to the stash,
   **Then** the row appears on every connected member's stash screen within
   the E7 end-to-end budgets, and a claim-history row records the entry
   (`from: outside`, `to: stash`) with their actor identity and time.
2. **Given** any character owner, **When** they transfer an item between a
   character's inventory and the stash, **Then** both sides update in one
   committed change — the stash row and the character's inventory row move
   together or not at all — and one claim-history row names the item,
   quantity, origin, destination, actor, and time.
3. **Given** two members moving *different* items at the same moment,
   **When** both changes commit, **Then** both apply — items are
   independent rows; one member's move never blocks or erases another's.
4. **Given** a stash row whose quantity would drop to exactly zero,
   **When** the transfer commits, **Then** the row is removed from the
   list (zero-quantity rows are deleted, not displayed as zero — the
   schema's shipped rule) and the movement is in the log.

### User Story 2 — The quartermaster sells the unkept loot (Priority: P1)

The party keeps the sword, sells the rest. The quartermaster opens the
stash, taps sell on an item, and the sale prompt opens **pre-filled with
the book value** — resolved from the imported items corpus by a
case-insensitive exact-name match. An item the corpus doesn't price (or
doesn't know at all) is flagged "no book value" and the quartermaster
types a number — in-game negotiation is the point of an editable prompt.
Confirming the sell removes the item from the stash, lands the proceeds in
the party bank shown in the stash header, and appends the sale to the
claim history like any other stash event.

**Why this priority**: US-5's second half. The sell flow is where the
corpus investment (E4) pays out and where the bank gets its feed; the
quartermaster gate keeps one accountable seat on the money.

**Independent Test**: Seed the corpus with a priced item and an unpriced
one; open the sell prompt for each; assert the first pre-fills with the
corpus price and the second shows "no book value" requiring manual entry;
confirm a sell and assert the item is gone, the bank header shows the
proceeds, and one claim row records `to: bank`.

**Acceptance Scenarios**:

1. **Given** a stash item whose name matches exactly one corpus item price
   (case-insensitive), **When** the quartermaster opens the sell prompt,
   **Then** the sale value pre-fills with that book value, editable before
   confirming.
2. **Given** a stash item with no corpus match, or a corpus match whose
   price is absent or empty (the importer's captured cases: two kits with
   no `level`, one legacy item with empty `price.value`), **When** the
   prompt opens, **Then** it is flagged "no book value" and confirmation
   requires a manually entered value.
3. **Given** the quartermaster, **When** they confirm a sell, **Then** the
   quantity leaves the stash (row deleted at zero), the proceeds land in
   the party bank in the same committed change, one claim row records the
   sale (`from: stash`, `to: bank`) with the coins moved, and every
   connected screen updates without reload.
4. **Given** any account that is not the quartermaster character's owner
   (including the GM and other character owners), **When** they attempt a
   sell or a bank adjustment, **Then** the server refuses it before any
   state change, the refusal is audited with their identity, and the
   client surfaces the refused operation through its op record — never a
   silent drop.

### User Story 3 — "Who took the wand?" (Priority: P1)

A week later nobody remembers who claimed the wand. Anyone opens the claim
history from the stash screen and reads it: every add, transfer, claim and
sale, in order, with item, quantity, where it moved from and to, who did
it, and when. The log is append-only — no one, including the app's own
code paths, can edit or delete a row. It is the argument-settler, and it
settles.

**Why this priority**: The PRD's parenthetical on the guardrail —
"argument-settling is the feature." The log is not a nicety bolted under
the stash; it is the reason money and item mutations are append-logged at
all (FR-9).

**Independent Test**: Perform one of each movement type (add, transfer
out, transfer in, claim, sell, bank adjustment); assert the log shows six
rows in order with correct fields; attempt an in-place update and a delete
against the history table and assert both fail (the shipped trigger).

**Acceptance Scenarios**:

1. **Given** any stash or bank movement, **When** it commits, **Then**
   exactly one claim row exists for it, written in the same transaction as
   the state change, carrying item (or note), quantity, origin,
   destination, actor identity, and timestamp.
2. **Given** any account or code path, **When** an update or delete is
   attempted against a claim row, **Then** the database refuses it (the
   shipped append-only trigger) — the refusal needs no new code, and this
   spec adds none.
3. **Given** any party member or the GM, **When** they open the claim
   history view, **Then** the log renders newest-first from live state —
   a movement committed on another seat appears without reload.

### User Story 4 — The party bank, accounted (Priority: P1)

The stash header shows the party bank: gp, sp, cp. It feeds from sales
automatically; the quartermaster can also post a manual adjustment — "paid
25 gp for the healing potions" — and the adjustment is actor-logged in the
same claim history, visible to everyone. Nobody's gold quietly disappears:
concurrent money movements *add*, they never overwrite each other.

**Why this priority**: The bank is the sell flow's landing pad and the
party's shared wallet; its integrity rule (additive, never overwritten) is
the epic's sharpest correctness demand — FR-2's whole reason.

**Independent Test**: Queue two bank movements offline from two seats
(a sale's proceeds and a manual adjustment); reconnect both; assert the
balance equals the sum of both movements applied exactly once, and both
claim rows exist.

**Acceptance Scenarios**:

1. **Given** the quartermaster, **When** they post a manual adjustment
   (a signed amount and a note), **Then** the balance changes by exactly
   that amount, one claim row records the movement with the note and
   actor, and every seat sees it live.
2. **Given** an adjustment that would drive any denomination negative,
   **When** it is submitted, **Then** the server rejects it with a reason
   before any state change, and nothing partial applies (the shipped
   non-negative constraints on the bank row).
3. **Given** two money movements racing (two seats, offline or online),
   **When** both reach the server, **Then** both apply — the balance is
   the sum of the two; no movement is ever silently superseded away
   (FR-2's settled rule, the one thing this epic must not get wrong).

### User Story 5 — The tunnel drops mid-loot-split (Priority: P1)

The split is happening when the tunnel flakes. The stash stays fully
readable; the in-flight transfer, sell or adjustment queues locally like
any write; the "syncing…" indicator does its quiet thing. When the link
returns the queue replays and **every operation lands exactly once** — an
operation the server already applied is recognized on replay and
acknowledged, never applied twice: no item moves twice, no coin lands
twice, no log row duplicates. An operation the moving world has made
impossible (the item was already sold from another seat) comes back
refused with a reason, surfaced on the op record — not dropped silently.

**Why this priority**: This is E7's offline promise (US-2 there) carried
onto money and items, where a duplicated or silently-dropped write is not
a cosmetic bug but a wrong ledger. The AI guardrail names it: sell/bank
operations idempotent-safe against replayed offline writes.

**Independent Test**: With the server stopped, queue a transfer, a sell
and an adjustment; restart and let the queue drain; replay the same three
frames again by hand; assert the ack for each replay is `already_applied`,
and that stash quantities, bank balance, and claim-row counts are exactly
what a single application would produce.

**Acceptance Scenarios**:

1. **Given** a queued operation whose acknowledgement was lost (applied
   server-side, response never arrived), **When** the client replays it,
   **Then** the server recognizes the operation id and acknowledges it as
   already applied — no second state change, no second claim row, no
   second fan-out.
2. **Given** a queued operation, **When** the server restarts before the
   replay, **Then** exactly-once still holds — the idempotency ledger is
   durable state, not memory.
3. **Given** a queued operation whose target moved on (the stash row was
   sold to zero and deleted from another seat), **When** it replays,
   **Then** it is refused with a human-readable reason, nothing partial
   applies, and the client surfaces the refusal on the operation's record
   in the stash screen — per E7's degraded-mode contract §4, a rejected
   operation is a user-visible fact, not a sync event to be silenced.
4. **Given** any replayed operation with the same id but a different
   request body, **When** it arrives, **Then** it is refused — operation
   ids are single-use (E7's ledger rule, inherited whole).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1 — The party stash** : The system MUST provide a shared,
  party-scoped loot list. A stash row carries item name (exact text),
  quantity (integer > 0), optional per-item Bulk, and a free-text note.
  The stash screen MUST render the list plus the party bank balance in its
  header, live-synced through the party WebSocket. A row whose quantity
  reaches zero MUST be deleted, not retained at zero (the shipped schema
  rule). Row identity is the item name within the party — the same
  identity rule character inventory already uses (exact name, E5's
  semantics); adding an item whose name matches an existing row MUST be
  refused with a reason telling the actor to adjust the existing row's
  quantity instead.

- **FR-2 — Reconciliation granularity (settled here, as the prompt
  deferred)**: Stash and bank fields join E7's per-field versioning at
  these units, each carrying a version from the one global sequence:

  1. **The party bank is one version unit — the whole balance row.** All
     three denominations move atomically under the row's single version
     (the shipped schema: one `version` column on `party_bank`). A money
     movement is an **additive delta** (`+gp, +sp, +cp`, signed), applied
     server-side under the row lock and serialized by arrival order.
     Money writes MUST NOT be absolute-value CAS writes: an absolute
     write racing another movement silently supersedes it — that is
     precisely how a ledger loses gp, and it is the rejected alternative
     recorded below.
  2. **A stash row is one version unit, keyed by item name within the
     party** — the same keying as the character `inv` target. Quantity
     movements are **additive deltas**, applied server-side and serialized
     by arrival order, with two failure modes only: underflow (quantity
     would go below zero → refused with reason) and unknown row (refused).
     Bulk/notes edits are **absolute writes** under CAS on the row version
     — metadata may supersede (rare, single-field, redoable); accounting
     never does.
  3. **The quartermaster designation is a versioned party field**
     (absolute write, CAS) — see FR-5.
  4. **Claim rows are append-only and unversioned** — identity is the
     monotonic row id; clients merge by id (a row applies exactly once,
     order by id). No version arbitration exists or is needed for an
     append-only log.
  5. **Supersession never drops a stash-quantity or money delta.** The
     E7 outcome vocabulary is inherited whole (`applied | superseded |
     already_applied | rejected | forbidden`), and additive writes use
     only `applied / already_applied / rejected / forbidden` — never
     `superseded`. The `base_version` field rides the wire frame as ever
     (uniform shape), and the server ignores it for additive applies.

  **Rejected alternative (recorded per Constitution Article II)**:
  *absolute-CAS money and quantities* — read-modify-write on the balance
  or a quantity, with the loser silently superseded per E7's field rule.
  Rejected because silent supersession is the correct semantics for a
  single owner's absolute fields (HP, a slot) and the wrong semantics for
  a six-writer ledger: it loses gp and loot without a trace, and the
  argument-settling log would record the winner's move while the loser's
  vanishes. Additive-apply keeps every movement; the only losers are
  genuinely impossible operations, which are refused loudly (FR-8).

- **FR-3 — Transfers**: Any character owner MUST be able to move an item
  between a character's inventory and the stash (either direction),
  choosing any roster character as the counterparty — claims and transfers
  are open to all (PRD). A transfer is **one compound operation**: in a
  single server transaction it adjusts the stash row (additive), adjusts
  the counterparty character's inventory row (additive, minting the row if
  the character lacks the item name), appends exactly one claim row, and
  bumps both touched field versions. All-or-nothing: no partial state
  exists. The transfer's inventory write is the one sanctioned non-owner
  path to a character inventory row — accountability comes from the claim
  row naming the actor, not from ownership (design D4 records the matrix
  extension). Over-drawing the stash MUST be refused with a reason
  (never silently superseded); the client picker additionally prevents
  over-draw by disabling impossible quantities (belt and braces — the
  server is the authority).

- **FR-4 — Claim history**: Every stash and bank movement MUST append
  exactly one claim row in the same transaction as the state change:
  `{item or note, quantity, from, to, actor, timestamp}` plus, for money
  movements, the signed coin amounts (design D1's coins extension — the
  shipped table's premise is "the movements are already in
  `claim_history`; a second ledger would be complexity, not audit", and
  this spec keeps that premise true). The label vocabulary for `from`/`to`
  (the shipped schema deliberately left the format here): **`stash`**,
  **`bank`**, **`outside`** (loot entering the ledger; money deposited or
  withdrawn), and **`character:<display name at event time>`** — a frozen
  historical record; a later rename does not rewrite history. The log
  view MUST render newest-first from live-synced state and MUST be
  readable by every party member and the GM. The shipped append-only
  trigger stands unchanged.

- **FR-5 — Quartermaster designation**: One character at a time is the
  quartermaster (the shipped nullable `parties.quartermaster_character_id`,
  same-party constrained, cleared on the character's deletion). The
  toggle lives in party settings on the stash screen. **Flip rule**: when
  a quartermaster is designated, only that character's owner may change
  or clear the designation; when none is, any character owner may set it
  (bootstrap). Party settings otherwise remain writable by any character
  owner per the PRD. The designation is a versioned, live-synced party
  field: a change MUST fan out so every seat's sell/adjust affordances
  appear or disappear without reload. Only the quartermaster character's
  owner may sell or post bank adjustments; the GM never holds the
  designation and never writes it.

- **FR-6 — Sell**: Selling is a quartermaster-only compound operation on
  a stash item: prompt for sale value **defaulting to the book value**
  (editable — in-game negotiation is the feature), then in one
  transaction remove the quantity from the stash (row deleted at zero),
  credit the proceeds to the party bank (additive), and append one claim
  row (`from: stash`, `to: bank`, coins = the confirmed sale value).
  **Book value resolution**: a case-insensitive exact-name match against
  imported item rows in the corpus, resolved to the price coin object; a
  match set that is empty, missing its price, empty-priced, or disagreeing
  on price across duplicate names yields **"no book value"** — flagged in
  the prompt, manual entry required. The platinum case: corpus prices may
  carry pp; the bank holds gp/sp/cp only, so a sale value containing pp
  credits its gp equivalent at the PF2e exchange rate (1 pp = 10 gp); the
  page-level citation for the rate lands in the implementation PR's doc
  comment, written off the source page per AGENTS.md rules-exactness §1 —
  this spec cites only what it can read. Selling a quantity smaller than
  the row's MUST reduce the row, not delete it.

- **FR-7 — Party bank**: A shared gp/sp/cp balance, one row per party
  (shipped), visible to all party members and the GM in the stash header,
  fed additively by sales and by quartermaster-only manual adjustments.
  Adjustments carry a signed amount and a human note; the note rides the
  claim row. Balances MUST never go negative (the shipped constraints);
  an adjustment that would MUST be refused whole, with a reason.
  Transferring coins between a character's sheet money and the bank is
  **not** this epic's (the PRD lists no such move; see Out of scope).

- **FR-8 — Idempotence under replayed offline writes (the guardrail,
  testable)**: All stash/bank operations ride the E7 write path — the
  client operation id is the dedupe key, reserved in the durable
  `client_ops` ledger in the same transaction as the operation, before
  any state change. The replay cases, named, each with the criterion that
  proves the guardrail true — and would catch it false:

  | # | case | criterion |
  |---|---|---|
  | R1 | **Ack-lost replay** — the server applied the op; the response never arrived; the client resends the identical frame | Second ack is `already_applied`; stash quantity, bank balance, and claim-row count are each exactly the single-application values (asserted numerically) |
  | R2 | **Post-restart replay** — R1's scenario across a server restart | Same assertion; the ledger is durable, so exactly-once survives |
  | R3 | **Double-queue / double-tap** — the same op submitted twice by client defect or double-tap | Same as R1 (one op id ⇒ one application) |
  | R4 | **Id reuse** — same op id, different request body | Refused (`rejected`); nothing committed; the original application stands |
  | R5 | **Changed world** — replay against a row deleted or drawn down by another seat's already-applied op (sell to zero; over-draw) | Refused with a human-readable reason; zero partial application (no bank credit without the item leaving, no claim row); surfaced client-side on the op record |
  | R6 | **Concurrent sells from two seats** — two different op ids, both offline-queued, both target the same quantity | Exactly one applies; the other is refused per R5; the bank credits once; one claim row per applied op and none for the refused one |

  A green run of R1–R6 is the acceptance evidence for "idempotent-safe";
  the implementation PR carries it (design D8 maps each to its suite).

- **FR-9 — Append-logged before state changes**: Money and item mutations
  append their claim row **in the same database transaction** as the
  state change, with the operation-id reservation ahead of both. There is
  no ordering in which a state change is visible without its log row, and
  no log row without its state change — argument-settling is structural,
  not a follow-up write. (The AI guardrail's letter is "append-logged
  before state changes"; the same-transaction reading is the only one a
  single Postgres transaction admits, and the stronger half — no
  un-logged state change — is what testing asserts.)

- **FR-10 — Authorization**: E3's ownership matrix extends with
  party-scoped resources (design D4): stash writes (add, transfer,
  quantity adjust, metadata edit) — any character owner of the party;
  bank mutations (sell, manual adjustment) — the quartermaster
  character's owner only; quartermaster designation — FR-5's flip rule;
  reads — every party member and the GM. The GM writes nothing, ever
  (unchanged). Every refusal MUST be denied before any state change and
  audited in the existing audit trail under new stash/bank/party event
  values (design D1's constraint swap) with the actor's identity. Write
  authorization is enforced per WS message, deny-by-default, exactly as
  E7 FR-1 requires.

- **FR-11 — The stash screen (UI)**: A stash screen joins the party
  surface, reachable from the party screen's header for every seat
  including the GM. It renders: the bank balance in the header; the loot
  list (name, quantity, Bulk, note — Bulk rendered through the same
  formatting the sheet uses, no second Bulk-math path); per-item actions
  (transfer to/from a roster character — open to all; sell — rendered
  only where the server would allow it, i.e. for the quartermaster's
  owner); an add-item form; the claim-history log view; and party
  settings carrying the quartermaster toggle (flip rule per FR-5, control
  disabled for seats that may not flip it). Offline, the screen serves
  last-known state read-only with writes queueing per E7's degraded-mode
  contract — one path, "syncing…" indicator only. Refused operations
  (R4/R5 outcomes) surface through the op record in the stash screen —
  non-blocking, per degraded-mode §4's ruling that `rejected`/`forbidden`
  are user-input facts, not sync events. The GM's stash screen renders
  zero write affordances.

### Key Entities

- **Stash row** — one per (party, item name): quantity, optional Bulk,
  note, version. Deleted at zero.
- **Party bank** — one row per party: gp/sp/cp, one version. Additive.
- **Claim row** — the append-only movement record: item-or-note,
  quantity, from, to, actor, timestamp, coins (money movements only).
- **Quartermaster designation** — the versioned party field naming one
  character (or none).
- **Compound operation** — a client write (one operation id) committing
  several field changes plus one claim row atomically: transfer, sell.
- **Book value** — the corpus-resolved price for a stash item's name:
  the sell prompt's default, never the sale's constraint.

### Constraints (settled by the epic — non-negotiable)

- The E7 sync mechanism is consumed, not redesigned: new versioned field
  kinds join the existing field table, write frames, snapshot/diff/
  catch-up, and the durable op ledger — no second sync engine, no
  protocol rewrite (E7 FR-13's own words).
- E2's shipped tables are activated, not rebuilt: this epic's migration
  adds version columns, the claim-row coins columns, and swaps the audit
  event-values constraint — nothing else. All reversible.
- Client clocks are untrusted; server-receipt order is the only ordering
  authority (inherited).
- The offline queue is E7's: durable per account, unbounded, replayed
  after snapshot merge; this epic adds no queue of its own.
- PRD non-goals bind: no combat/round/initiative tracking, no GM
  tooling beyond read-only, no automation of pricing, expiry, or
  positioning. The sale prompt's default is a convenience, not a price
  authority — the confirmed value is whatever the quartermaster enters.
- Stack settled (Rust/axum single binary, Svelte static bundle, Postgres)
  — this epic changes no stack decision.

### Success Criteria

- **SC-1** A stash add or transfer committed on one seat renders on every
  other connected seat — loot list, both inventories, bank header —
  within E7's end-to-end budgets, no reload, per FR-1/FR-3.
  *(Bench-verifiable: two-client integration test asserting applied state
  on both seats.)*
- **SC-2** One claim row exists per committed movement, in the same
  transaction: after a forced failure between state change and log write
  there is no such thing as one without the other (kill the transaction,
  both vanish; let it commit, both exist). The append-only trigger
  refuses in-place UPDATE and DELETE. *(Bench-verifiable.)*
- **SC-3** The sell prompt pre-fills with the corpus book value on a
  unique case-insensitive exact-name match, flags "no book value" on
  empty/absent/disagreeing matches, and credits the confirmed value once
  under replay (R1 applied to the sell compound). *(Bench-verifiable:
  corpus fixtures for each match outcome.)*
- **SC-4** Two racing money movements both apply; the balance equals
  their sum; no additive write is ever answered `superseded`. An
  underflowing adjustment is refused whole. *(Bench-verifiable.)*
- **SC-5** Replay cases R1–R6 each pass their criterion (FR-8's table) —
  including across a server restart. *(Bench-verifiable: the named
  integration suite.)*
- **SC-6** A quartermaster change fans out live: another seat's sell
  affordance appears/disappears without reload; non-QM sell/adjust
  attempts are server-refused, audited, and surfaced on the op record.
  *(Bench-verifiable.)*
- **SC-7** Offline, the stash screen renders last-known state read-only,
  queues writes, replays exactly once on reconnect, and surfaces refused
  ops non-blockingly — no modal, no toast, no lost write.
  *(Bench-verifiable.)*
- **SC-8** The GM's stash screen renders the full list, bank, and log
  with zero write affordances; every GM write attempt is refused
  server-side and audited. *(Bench-verifiable.)*

## Edge Cases

- **Add an item whose name already exists in the stash** — refused with a
  reason pointing at the existing row (quantity adjust instead). Exact
  name means byte equality, case included — "Rope" and "rope" are two
  rows, matching the character-inventory identity rule; the corpus match
  for book value is separately case-insensitive (two different rules for
  two different jobs, each quoted from its source).
- **Two members transfer out of the same row concurrently** — additive
  apply: the first lands, the second either lands (quantity sufficed) or
  is refused (over-draw) — never silently superseded.
- **Transfer in of an item the character never had** — the character's
  inventory row is minted by the compound op.
- **Transfer out drives a character's effective quantity negative** —
  server applies additively (the `qty_delta` column has no floor, same as
  E7's own inventory writes); the UI picker prevents the over-draw
  client-side; a negative effective quantity displays honestly.
- **Sell a partial quantity** — row reduces, stays listed; claim row
  records the sold quantity.
- **Sell to zero, then a queued transfer-in replays** — the row no longer
  exists; the transfer-in mints it fresh (an inbound transfer may create
  a row — only `stash_new` is refused on duplicates).
- **Corpus name collisions with disagreeing prices** — "no book value"
  (deterministic; manual entry). Collisions with agreeing prices resolve
  to that price.
- **A pp-priced book value** — credited as its gp equivalent at the
  PF2e exchange rate (FR-6); the rate's citation lands in the
  implementation.
- **The quartermaster character is deleted** — the shipped FK clears the
  designation (SET NULL); the stash reverts to bootstrap (any character
  owner may set a new quartermaster); money history stays intact.
- **A character renames after a movement** — claim rows keep the name as
  it was (frozen history); new movements use the new name.
- **Claim log growth** — snapshot carries the full log at POC scale
  (append-only, party-scoped; a season is low thousands of rows — the
  same sizing posture as the op ledger). If a season ever outgrows it,
  the escape hatch is a recent-tail cap with paging, a schema-free
  change — recorded here so it is a decision, not a discovery.
- **Offline sell with no cached book value** — the lookup needs the link
  (a session-gated read); offline the prompt opens flagged "no book
  value" and the quartermaster types the number.
- **Solo party** — same degenerate no-op as E7; the stash works for one.

## Assumptions

- One party at POC (config, not change — inherited).
- The queue's localStorage durability limit is inherited from E7/E10
  unchanged; this epic adds no new durability surface.
- Book value is resolved at prompt time (prices ride corpus data; a
  re-import's price changes affect future prompts, not past claim rows —
  the log freezes what was actually credited).
- The party screen's header is the stash entry point; layout details are
  design- and prototype-driven. This spec fixes contents and rules, not
  pixels.

## Out of scope (guarded)

- **Coin transfer between character sheet money and the bank** — the PRD
  lists no such move (bank feeds from sales and adjustments only); adding
  it is a PRD change, not an implementation liberty. Stop and flag.
- **Per-denomination bank versioning** — wrong unit, rejected in FR-2.
- **Any curation of stash items** — E15 owns corpus prose editing; stash
  rows are live state, not corpus content.
- **Combat/round tracking, pricing automation, expiry, positioning** —
  PRD non-goals, forever.
- **Multi-party stash UI** — schema is party-scoped already; the UI stays
  one-party like the rest of the POC.

## Clarify Log

No `[NEEDS CLARIFICATION]` markers remain. Five decisions the prompt or
PRD left open are settled in place, each with its reasoning recorded
above:

1. **Stash/bank reconciliation granularity** (deferred to this spec by
   the prompt) — settled FR-2, with the lost-gp trap as the recorded
   rejected alternative.
2. **Where manual bank adjustments log their actor** — the claim history
   (one ledger, the shipped table's own premise), via the coins
   extension; audit-events-only and a second ledger table both rejected
   (design D1).
3. **Quartermaster bootstrap and delegation** — null ⇒ any character
   owner may set; set ⇒ only the designated character's owner may change
   or clear (FR-5, from the PRD's own sentences).
4. **Book-value ambiguity** — empty, absent, or disagreeing match sets
   are "no book value", deterministically (FR-6).
5. **Claim label vocabulary** (deferred to this spec by the schema's own
   comment) — `stash` / `bank` / `outside` / `character:<name at event
   time>` (FR-4).

Nothing in this spec needs Josh. The epic stays inside its P1 scope; no
P0/P1/P2 line moves.
