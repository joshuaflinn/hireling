# Shared Inventory Implementation Plan (E12)

**Epic**: E12 — Phase 3, P1 · GitHub issue #14 · spec + design in this
directory (2026-10-14, Korrin). **No implementation hold sits on this
lane** (docs merges were never under the E14 hold; build work may start
when a seat frees — check the lane board).

## Global Constraints

- **Scope**: the whole diff surface is design D9's file map. Anything a
  task wants outside it stops and flags — engine, E6 sheet internals, E8
  effect paths, E10 PWA shell are all out.
- **The mechanism is consumed, not redesigned** (spec Constraints): new
  field kinds join the existing write frame, ledger, snapshot/diff path.
  No second sync engine, no new frame types, no new queue.
- **Additive never supersedes** (spec FR-2.5): quantity and money writes
  are additive applies; `superseded` is unreachable for them. CAS exists
  only on `stash` `meta` ops and the `party` quartermaster field.
- **One transaction per movement** (spec FR-9): op-id reservation →
  authz → claim row → field changes; all-or-nothing, always.
- **Rules facts carry citations** (AGENTS.md rules-exactness): the pp→gp
  exchange rate's page-level citation lands in the sell apply's doc
  comment, written off the source page — never from memory, never from a
  review comment. The spec quotes no rate value; any value quoted
  upstream of the source page is not a source, and a mismatch is a
  stop-and-flag. The `L = 0.1` Bulk convention is already cited where it
  ships (schema comment); the UI reuses the sheet's helpers and adds no
  Bulk math.
- Tests run the production path (AGENTS.md Style): WS integration through
  the real apply arms; components via `@testing-library/svelte` with real
  events; every data-sourced assertion gets both sides of its boundary
  (the sell dialog's found/flagged states, the flip rule's designated/
  undesignated states, underflow's refuse/allow sides).
- Verify per task: scoped suites first, `just ci-local` full before the
  PR; paste real output; a handover that calls the gate clean states the
  warning count. `Co-Authored-By: Paperclip <noreply@paperclip.ing>` on
  every commit; imperative subjects.

## Task 1 — Migration + authz table (D1, D4)

**Files**: `migrations/2026MMDD000001_e12_shared_inventory_activation.sql`
(+ `.down.sql`), `src/auth/authz.rs`, `src/auth/tests/authz.rs`,
`src/tests/db.rs` (schema assertions), `src/main.rs` or boot path only if
the migration list is registered somewhere (it isn't — sqlx migrate picks
the dir up; leave alone).

**Do**: the migration's five changes exactly as D1 (stash `version`,
`parties.quartermaster_version`, `claim_history` coins columns with the
quantity-is-1 and money-row-label comments, audit CHECK swap, the
`UNIQUE (party_id, item_name)` identity constraint with the redundant
non-unique index dropped). Then the authz extension:
`Resource::Stash/Bank/PartySettings` as pure variants carrying the
deciding facts in the payload — `Stash { actor_is_party_member: bool }`,
`Bank { quartermaster_owner_sub: Option<String> }`, `PartySettings {
actor_is_party_member: bool }` (D4) — the lookups happen at the call
site, and `authorize()`'s write branch becomes a per-variant match that
absorbs `sole_writer()`'s three sole-owner arms (`Character`, `Effect`,
`CustomRow`); **delete `sole_writer()`** (gh#219): an exhaustive
`&str`-returning match has nothing to return for a party variant and
`wildcard_enum_match_arm` is denied, so it cannot stay unchanged, and
once the write branch is per-variant it has no callers.

**Acceptance** (test-first, each before its impl):
- `authz.rs` suite: the full allow/deny table over the variant facts —
  stash×{member, non-member, GM}, bank×{QM owner, other member,
  non-member, GM, undesignated}, settings×{member, non-member, GM}.
  Every cell named.
- Migration: up/down idempotent (`sqlx migrate run` + revert + re-run);
  db test asserts the new columns, the unique constraint (a second
  insert of the same `(party_id, item_name)` raises post-migration), and
  the trigger still firing on
  `claim_history` UPDATE **after** the coins columns landed (regression:
  an ALTER must not drop the trigger).
- Audit values: an insert of each new event value passes the CHECK; an
  unknown value still fails.

## Task 2 — Wire kinds + apply semantics (D2)

**Files**: `src/sync/protocol.rs`, `src/sync/write.rs`, `src/sync/snapshot.rs`,
`src/tests/sync/stash.rs` (new), `src/tests/sync/contract.rs`,
`specs/007-party-sync/contracts/wire-protocol.md` + `field-targets.json`
(the pinned-contract rule: both suites green over the edited files).

**Do**: the four `FieldTarget` kinds and their value shapes (D2 table);
`stash_new` mint + duplicate refusal mapped from the D1.5 unique
violation to `rejected` with FR-1's reason; `stash` `qty` additive apply
with underflow/unknown-row rejection; `stash` `meta` CAS; `bank` additive
apply with per-denomination non-negative enforcement (the shipped CHECKs
surface as `rejected` with reason, pre-formatted); `party`
quartermaster absolute CAS. Snapshot sections for all four. The §3 E12
contract rows, **§4's additive-class semantics bullet**, and the
`field-targets.json` entries land in the same commit as the kinds
(contract and code move together).

**Acceptance**:
- Additive test pair: two `qty` ops at the same stale `base_version` both
  apply (sum asserted); the same pair on `bank` both credit; **no ack in
  either suite is ever `superseded`** — assert the outcome enum directly.
- CAS pair: `meta` at stale base supersedes; `party` at stale base
  supersedes (the only two places it can).
- Underflow: `qty` over-draw and a negative bank delta each refuse whole
  with human-readable reasons; nothing partial (balance and quantity
  unchanged, no claim row).
- `stash_new` on an existing exact name refuses with the FR-1 reason.
- Boundary style: `stash_new` quantity 0 refused / 1 allowed; the two
  sides of every CHECK asserted.

## Task 3 — Compound ops: transfer, sell, adjust (D3)

**Files**: `src/sync/write.rs` (the op arms), `src/tests/sync/stash.rs`.

**Do**: `transfer` (either direction; character-side additive delta,
minting `character_inventory_live` when absent; claim row with the FR-4
labels), `sell` (quantity off the row, row deleted at zero, pp→gp credit
with the cited rate in the doc comment, claim row `stash → bank` with
coins), `bank` adjust (claim row `outside → bank` / `bank → outside` by
sign; coins carry the amounts; `item_name` carries the fixed
`bank adjustment` label per D1.3 — no note field). Transaction order per
D3; fan-out = one diff per touched field + the claim append.

**Acceptance** (each a test that fails first):
- Transfer: both sides move together; forced failure between (a poisoned
  transaction — the test harness's established pattern) leaves **neither**
  side moved and no claim row (SC-2).
- Sell: partial quantity reduces the row and leaves it listed; full
  quantity deletes it; pp price credits the gp equivalent (both sides of
  the conversion asserted with two fixtures).
- Authz: non-QM sell/adjust → `forbidden`, audited, nothing touched; GM
  write → `forbidden_gm_write`, audited (SC-6/SC-8 server halves).

## Task 4 — Idempotence block: R1–R6 (spec FR-8, D8)

**Files**: `src/tests/sync/stash.rs` (the block), the E7 restart harness
reused as-is.

**Do**: the six named replay cases as D8's table reads them — R1 ack-lost
(identical frame resend → `already_applied`; stash qty, the counterparty
character's inventory qty, bank balance, and claim-row count each
asserted as the single-application values), R2 the same across a server
restart, R3 double-submit, R4 id reuse with a different body (refused,
original stands), R5 changed world (row deleted / drawn down first →
refused with reason, **no partial bank credit, no claim row**), R6
concurrent sells (exactly one applies, one credit, one claim row for the
winner only).

**Acceptance**: the FR-8 table's criteria verbatim, each asserted
numerically. This suite is the guardrail's proof — a green run here is
what "idempotent-safe" means (SC-5).

## Task 5 — Book-value read (D5)

**Files**: `src/book_value.rs` (new module + its `src/tests/book_value.rs`
via the tree's `#[path]` pattern), `src/lib.rs` (the one-line `pub mod
book_value;`), `src/http.rs` (route registration in the `protected`
router), corpus fixtures for the match outcomes.

**Do**: `GET /api/parties/{id}/items/book-value?name=` — session-gated,
member-or-GM; the D5 resolution query; `{found, price}` shape. Fixtures:
unique match, no match, empty `price.value`, missing price path, two
names disagreeing on price, two agreeing, case-insensitive equality
(`longsword` vs `Longsword`).

**Acceptance**: both sides of every match boundary; non-member 403; GM
200. Found prices carry `pp` when the corpus row does (the conversion
itself is Task 3's credit site — this route reports, never converts).

## Task 6 — Stash store + screen (D6, D7)

**Files**: `web/src/lib/party/stash.js`, `web/src/lib/party/StashScreen.svelte`,
`web/src/App.svelte`, `web/tests/party/stash.test.js`,
`web/tests/helpers/wire-contract.js` (new kinds).

**Do**: the store (snapshot merge by version/id; diff apply; `write()`
actions per FR-11; op-record exposure for refused ops), the screen (bank
header, list with the sheet's Bulk formatting, add form, transfer roster
picker, QM-gated sell dialog — book-value prefill with the found/flagged
states, claim log newest-first, settings toggle disabled where the flip
rule denies), the App route.

**Contract pin — read this before writing the store** (transcribed read-only
from the #236 head `3b1323a644b3f64e49760b03502b0922575dc95b`, per the
pinned-contract rule; re-verify against `origin/main` at pickup, because the
pin is only binding once L2 merges):

- Snapshot section values (the store's input shape, `src/sync/snapshot.rs`):
  `stash` → `{item_name, quantity, bulk, notes}` @ row `version`;
  `bank` → `{gp, sp, cp}` @ row `version`, and the field is **absent** until
  the first movement (no row, no zero-mint); `party`/`quartermaster` →
  `{quartermaster_character_id: int|null}` @ `quartermaster_version`, always
  present; `claim` → `{item_name, quantity, origin, destination, actor_sub,
  created_at, coins_gp, coins_sp, coins_cp}` (coins NULL on item rows) with
  the **row id in the version slot** — D7's merge-by-id is the ordinary
  strictly-newer rule, not a special case.
- Write frames the client may build: `stash_new` `{item_name, quantity ≥ 1,
  bulk?, notes?}`; `stash` `{op:"qty", qty_delta}` (additive — ignore
  `base_version`, never `superseded`) and `{op:"meta", bulk?, notes?}` (CAS
  on the row version); `bank` `{gp_delta, sp_delta, cp_delta}` (additive,
  signed, per-denomination non-negative); `party` `{character_id: int|null}`
  (CAS). `claim` is read-only — no client write frame exists for it.
- `web/tests/helpers/wire-contract.js` must gain the `stash_ops` set from
  `field-targets.json` alongside the existing `effect_ops`/`vitals_fields`
  pins, so a client `op` literal drift fails the suite, not the socket.
- **The lane boundary is checkable, and Task 6 must honor it:** at `3b1323a`,
  `grep -nE '"transfer"|"sell"' src/sync/write.rs` returns **1 hit, and it is
  a test** — `stash(json!({"op": "sell", "qty": 1})).is_err()` with the
  message `"sell is lane L3, not wired yet"` (write.rs:3000). So `transfer`
  and `sell` are **not on the wire yet** — L3 adds them to the accepted set.
  Task 6's transfer dialog and sell dialog must post through the ops that do
  exist (`stash_new` / `stash qty` / `bank`) or gate behind L3's merge; a
  client frame the server refuses at bounds is a red production-path test,
  not a TODO.

**Acceptance** (production-path, real events):
- The sell dialog's two prompt states (prefilled / "no book value") both
  driven — one fixture asserts each (two-fixture rule).
- The settings toggle's states: character owner enabled (any owner may
  flip per FR-5 — including while designated), GM absent; flip posts the
  `party` write and the affordance moves on the fan-out without reload
  (fixture-mutated store).
- A refused op (injected `rejected` ack) renders its record
  non-blockingly; the queue indicator behaves per degraded-mode.
- GM seat: zero write affordances — scoped query in the screen's region,
  the E14 SC-5 discipline (assert the region's control set, not a page
  count).
- Transfer dialog: pick roster character, qty bounded by availability,
  submit → `stash` `transfer` op; the two-fixture rule on quantities.

## Task 7 — Offline + end-to-end sweep, gate, PR

**Files**: no new source; `web/tests/party/stash-offline.test.js`; PR body.

**Do**: the offline suite (read-only render from last-known state; writes
  queue; reconnect drains; replayed ops land once — the client half of
  R1; refused replay surfaced). Then the reachability sweep: walk spec
  FR-1..FR-11 and SC-1..SC-8, name the test that proves each (grep the
  suites; anything unproven is a bug, not a footnote). Full `just
  ci-local` with the warning count; check-runs read at the pushed head
  via the API before the review ask; bring the branch up to date with
  `origin/main` before asking (the main-gate up-to-date rule).

**Acceptance**: the sweep table lands in the PR body next to the gate
output; every SC names its suite; PR references gh#14 (`Refs #14` —
**not** `Closes`, the epic closes when built), links the Paperclip task.
