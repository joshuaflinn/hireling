# Design: Shared Inventory — Party Stash, Claim History, Sell & Party Bank (E12)

**Status**: signed by Korrin (PM seat) 2026-10-14 — decisions internal to
E12's P1 scope; nothing here moves the P0/P1/P2 line. Input chain:
[`spec.md`](spec.md) header. Implements the spec; the plan
([`plan.md`](plan.md)) executes it.

The shape in one paragraph: one migration adds version columns where the
shipped schema lacks them (stash rows, the quartermaster field), a coins
extension on `claim_history`, and swaps the audit event-values constraint;
four new field kinds join the existing sync wire (stash row, stash mint,
bank, party-settings) with additive-apply semantics for quantities and
money and CAS for metadata; transfers and sells are compound operations on
the E8 op-discriminated-value pattern, one transaction, one claim row,
multi-diff fan-out; the authz matrix grows party-scoped resources; one
session-gated REST read resolves book value from the corpus (E9's
party-scoped-REST precedent); the stash screen and its store join the
party surface E10 shipped.

---

## D1 — The schema delta: one migration, five changes, all reversible

`migrations/<date>e12_shared_inventory_activation.sql` (+ `.down.sql`,
reversible per house convention; date-stamped at implementation).

1. **`party_stash.version`** — `bigint NOT NULL DEFAULT nextval('field_version_seq')`.
   The row's version unit (spec FR-2.2). The shipped table had none; E7
   FR-13 named this epic as the one to add it.
2. **`parties.quartermaster_version`** — same shape. Makes the designation
   a live-synced party field (spec FR-2.3/FR-5). The column already holding
   the value (`quartermaster_character_id`, same-party FK-cycled, SET NULL
   on delete) is untouched.
3. **`claim_history` coins columns** — `coins_gp int`, `coins_sp int`,
   `coins_cp int`, all nullable. NULL = item movement; a value = money
   movement carrying the signed amounts (spec FR-4). `quantity` stays
   `CHECK (> 0)` — for coin rows it carries a meaningless 1, documented in
   the migration comment; the signed truth is in the coins. Item rows keep
   `quantity` as the movement size. A money row's `item_name` carries a
   deterministic label — the sold item's name for a sale, the fixed text
   `bank adjustment` for a manual adjustment — documented in the same
   comment; **no note column exists or is added** (spec Clarify Log #5:
   the PRD asks for actor logging, not notes).
4. **`audit_events` event values** — drop/re-add the CHECK with
   `forbidden_stash_write`, `forbidden_bank_write`,
   `forbidden_party_write` added to the list. Nothing else about the table
   changes.
5. **`party_stash` row identity — `UNIQUE (party_id, item_name)`**. The
   shipped table's only key is the surrogate `id` plus a non-unique party
   index, so FR-1's declared identity ("the item name within the party")
   had no structural backing: a check-then-insert race on `stash_new`
   could mint two rows for one name — after which every
   `(party_id, item_name)` op is ambiguous against a wire contract that
   addresses one row with one version. The constraint mirrors
   `character_inventory_live`'s composite PK
   (`migrations/20260924000002_live_state.sql`), makes the race
   impossible rather than unlikely, and its backing index serves the old
   non-unique index's prefix queries, so `party_stash_party_id_idx` is
   dropped in the same migration. `stash_new`'s friendly duplicate refusal
   maps the unique violation to `rejected` carrying FR-1's reason (D2).

**Rejected — a second money-ledger table**: the shipped `party_bank`
comment is the ruling ("the movements are already in `claim_history`; a
second ledger would be complexity, not audit"). A movements table with its
own idempotency story would duplicate the op ledger *and* the claim log.

**Rejected — logging adjustments to `audit_events` only**: audit events
are the ops trail, not a party-visible record; US-3's argument-settler
must render in the stash screen for every member. FR-4's claim-row ruling
stands on the shipped table's own premise.

**Rejected — a `party_stash_meta` split table**: metadata and quantity in
one row with one version is the `inv`/`effect` precedent (whole-row
version units); splitting the row to give notes its own version column
multiplies kinds for no acceptance criterion.

## D2 — The wire extension: four field kinds, two apply semantics

`src/sync/protocol.rs` grows `FieldTarget` variants; the contract rows
below land in `specs/007-party-sync/contracts/wire-protocol.md` **§3's
field table, §4's semantics, and §8's E12 section**, and
`field-targets.json` gains the kinds — the two suites that pin those
files stay green over the edited files (the pinned-contract rule). §4 is
an edit site because its "Write = CAS on `base_version`" bullet is
written as a universal; E12 introduces a third class — a field kind that
*has* a version unit and still does not CAS (additive applies ignore
`base_version`; `superseded` is unreachable for them) — and §4 gains that
additive class beside the CAS rule so the contract of record does not
state a rule E12's shipped code breaks.

| `kind` | key fields | value shape | version unit | apply |
|---|---|---|---|---|
| `stash_new` | `party_id` | `{item_name, quantity>0, bulk?, notes?}` | mint (identity) | refuses duplicate name — the D1.5 unique constraint is the backstop: a racing second insert raises, the violation maps to `rejected` with FR-1's reason; creates row + claim row (`outside → stash`) |
| `stash` | `party_id`, `item_name` | `{op:"qty", qty_delta}` \| `{op:"meta", bulk?, notes?, base_version}` \| `{op:"transfer", direction:"in"\|"out", qty, character_id}` \| `{op:"sell", qty, price:{gp?,sp?,cp?}}` | the row's `version` | `qty`: additive, never `superseded`; `meta`: CAS; `transfer`/`sell`: compound (D3) |
| `bank` | `party_id` | `{gp_delta, sp_delta, cp_delta}` | the balance row's `version` | additive, never `superseded`; QM-only |
| `party` | `party_id`, `field:"quartermaster"` | `{character_id \| null}` | `quartermaster_version` | absolute, CAS |

Snapshot/diff shapes: stash rows as `{item_name, quantity, bulk, notes,
version}`; bank as `{gp, sp, cp, version}`; party as
`{quartermaster_character_id, quartermaster_version}`; claim rows by id
(D7). Additive applies ignore `base_version` (the frame still carries it —
uniform shape); acks return the resulting version. `superseded` is
unreachable for additive ops — spec FR-2.5's rule, asserted in D8's
suite.

**Rejected — additive ops as new frame types**: E8's realized pattern
(op-discriminated values on the target they identify) keeps one write
frame, one ack path, one ledger; new frame types would fork replay and
ack handling for zero semantic gain.

**Rejected — CAS on quantities/money**: spec FR-2's recorded alternative;
the lost-gp/lost-loot trap. Not revisited here.

## D3 — Compound operations: one transaction, one claim row, multi-diff

`transfer` and `sell` ride `stash` targets with op-discriminated values
(the `effect` create/update/end pattern). Server-side, in one transaction,
in this order:

1. reserve `op_id` in `client_ops` (the shipped PK-reservation discipline);
2. authorize (D4) — refuse early, audited, nothing touched;
3. append the claim row;
4. apply the field changes (stash delta ± character-side delta for
   transfer; stash delta + bank credit for sell), each taking
   `nextval('field_version_seq')`;
5. commit → fan out one diff per touched field plus one claim append (the
   E8 create precedent: mint/compound ops emit the resolved rows).

Underflow/unknown-row checks run inside the transaction; a failure rolls
back claim row and field changes together (spec FR-9's structural reading).
The character-side inventory adjustment is `qty_delta = qty_delta ± qty`
on `character_inventory_live` (minting the row when absent) — additive on
the stored delta, no baseline read, no read-modify-write: E7's `cas_inv`
ruling governs the column; the transfer op is the sanctioned non-owner
writer (D4).

**Rejected — two client writes per transfer** (stash write + inventory
write): two ops, two ids, no atomicity — a crash between them loses one
side of the movement and logs half of it. The compound op is the whole
point.

**Rejected — claim row after the state change** ("best-effort log"): the
guardrail's letter is append-logged *before* state changes; a follow-up
write can be lost independently. Same-transaction is the only honest
reading (spec FR-9).

## D4 — Authorization: party-scoped resources join the matrix

`src/auth/authz.rs` grows resources that key on party membership rather
than sole ownership. The deciding facts ride **in the variant payload**;
the matrix stays a pure function — facts are inputs, loaded fresh per
message at the call site (E3's existing "ownership loaded fresh" rule;
the matrix performs no lookups) — and `authorize()`'s write branch
becomes a per-variant match:

- **`Stash { actor_is_party_member: bool }`** (add/qty/transfer/meta):
  allow iff `true` — any player who owns a character in the party. GM
  denied first by the role check (`forbidden_gm_write`, existing value);
  non-members deny.
- **`Bank { quartermaster_owner_sub: Option<String> }`** (sell/bank
  adjust): allow only when `Some(sub) && actor.sub == sub` — the owner of
  the character named by `parties.quartermaster_character_id`;
  `None` (undesignated) denies (`forbidden_bank_write`).
- **`PartySettings { actor_is_party_member: bool }`** (the quartermaster
  field): allow iff `true` — any character owner, per FR-5's settled flip
  rule (party settings are writable by any character owner; the
  designated owner holds no exclusive lock on the toggle)
  (`forbidden_party_write`).

`sole_writer()` is **deleted** and its three sole-owner arms
(`Character`, `Effect`, `CustomRow`) move inline into the per-variant
write match — the per-variant write branch is where the new rules live,
and once it absorbs those arms the function has no callers (a dead `pub
fn` in a single-binary crate is a `dead_code` warning, and the gate's
warning census is zero). It cannot simply stay unchanged: an exhaustive
`&str`-returning match has nothing to return for a party-membership
variant, and the `_ =>` catch-all that would paper over that is
unavailable — `wildcard_enum_match_arm` is denied project-wide
(`docs/toolkit-conventions.md`, Lint Configuration). The other exit,
retyping it `Option<&str>` with `None` for the party variants, was
rejected: it keeps a table with no caller and forces an
unwrap-and-compare ceremony into every sole-owner arm. Reads need
no matrix row: E3's matrix allows every authenticated read, and read
scoping is the shell's party gate — `party_readable()` at the REST layer
(`src/engine_host/rest.rs`), the D5 precedent, which E12's surfaces
join. Deny-by-default per message stands. Audit values per D1.4.

**Rejected — variants keyed on `party_id` alone**: a lookup key is not a
deciding fact. With only `party_id` in the value the matrix must either
look up membership/designation itself (breaking "facts are inputs, not
lookups") or decide nothing — and Task 1's allow/deny table cannot name
its non-member cell. The payloads carry the facts the rules read.

**Rejected — a `quartermaster` role on accounts**: the designation is a
character fact (the PRD's own model — "one character is designated"), not
an account role; role-izing it would fight the shipped FK and the
party-settings flip rule.

## D5 — Book value: one session-gated party-scoped REST read

`GET /api/parties/{party_id}/items/book-value?name=<item>` —
session-gated, party-member-or-GM readable (E9's `GET
/api/parties/{id}/conditions` precedent for party-scoped corpus reads).

Resolution (spec FR-6): `SELECT name, data->'system'->'price'->'value'
FROM corpus_entries WHERE kind='item' AND lower(name)=lower($1)`; reduce
the match set to distinct non-empty coin objects; exactly one distinct
value → `{found: true, price: {pp?, gp, sp, cp}}`; zero matches, missing
or empty prices, or disagreeing values → `{found: false}`. No lane filter
— price presence decides (custom rows lack the price path and read as
unpriced). The pp→gp exchange happens at credit time (bank holds no pp),
in the sell apply — one conversion site, cited there per rules-exactness.

Offline: no lookup (the read needs the link) — the prompt opens flagged
"no book value", manual entry (spec's edge case). The lookup is a
prompt-time convenience only; **the confirmed value is what the QM
entered** — the claim row records it, never the looked-up one.

**Rejected — flagging pp-priced items "no book value" instead of
converting**: would push gp arithmetic onto the quartermaster for every
pp price to avoid one cited constant. The conversion is kept as the
convenience default, and the cost is recorded: a rules-exactness
obligation — the rate cited off the source page at the conversion site
plus a two-fixture boundary test — with no rate value quoted anywhere
upstream of that page (spec FR-6).

**Rejected — resolving book value into the stash row at add time**: a
corpus re-import's price changes would strand stale copies on rows; FR-4's
frozen-history rule and the "resolved at prompt time" assumption both
point the other way.

**Rejected — shipping the whole item price map to the client** (a corpus
sync surface): thousands of rows to save one round-trip on a rare prompt;
E9 already ruled the party-scoped REST read is the pattern for corpus
reads the client needs on demand.

## D6 — Client: the stash store joins the roster state; one screen

- `web/src/lib/party/stash.js` — a stash store over the existing sync
  module (`createSyncSession`'s connection): merges snapshot sections
  (stash rows, bank, party settings, claims), applies diffs by version/id,
  exposes `write()` for the FR-11 actions (the sync module's queue,
  indicator, and replay semantics are inherited whole — no new queue).
- `web/src/lib/party/StashScreen.svelte` — the screen: bank header, loot
  list (Bulk via the sheet's own formatting helpers — no second Bulk-math
  path), add-item form, per-item transfer dialog (roster picker),
  QM-gated sell dialog (book-value prefill via D5, "no book value" flag),
  claim-history log, party-settings quartermaster control (disabled for
  seats that may not flip). Refused ops render from the op records the
  sync store already exposes — non-blocking, per degraded-mode §4.
- `web/src/App.svelte` — the view machine gains the stash route off the
  party header (the drill-in pattern the sheet already uses).
- `web/src/lib/sheet/components/InventoryPanel.svelte` — unchanged
  character-side surface; transfers *out* launch from the stash screen's
  roster picker, not from the sheet (one entry point, one claim flow).

**Rejected — a stash tab inside the sheet**: the stash is party state, not
character state; the PRD places the screen in the party surface ("Stash
(P1)" under Party Screens). The sheet keeps its inventory panel as-is.

**Rejected — optimistic local claim rows**: the claim log renders
server-committed rows only; a pending op shows as the pending state of its
own controls (degraded-mode's local-echo rule covers the write targets —
the log is the settlement record and stays server-authoritative).

## D7 — Claim merge: append-only by id, newest-first render

Claim appends fan out as `claim` diff rows keyed by the monotonic row id;
the store keeps an id-keyed map, renders newest-first. Snapshot carries
the party's full log at POC scale (spec's edge-case sizing note; the cap
is an escape hatch, not a build item). Labels per spec FR-4's vocabulary —
`outside`, `stash`, `bank`, `character:<display name at event time>`
(frozen history; renames never rewrite rows). The log view needs no
paging at POC.

## D8 — Test layout: every SC and every replay case to a named suite

| criterion | suite (bench) |
|---|---|
| SC-1 live two-seat render | `src/tests/sync/` two-client integration (the E7 harness): stash add + transfer fan-out to both seats |
| SC-2 same-transaction log + append-only | `src/tests/sync/stash.rs`: movement commits carry exactly one claim row; forced mid-transaction failure leaves neither; UPDATE/DELETE on `claim_history` still raises (trigger regression) |
| SC-3 book value outcomes | `src/tests/book_value.rs`: corpus fixtures for unique-match/absent/empty/disagreeing/duplicate-agreeing + pp conversion at credit |
| SC-4 additive money | `src/tests/sync/stash.rs`: two racing credits sum; underflow refused whole; no additive ack is ever `superseded` |
| SC-5 R1–R6 replay | `src/tests/sync/stash.rs` idempotence block: ack-lost, post-restart (the E7 restart harness), double-submit, id-reuse, changed-world, concurrent-sells — each asserting the FR-8 table's numeric criterion |
| SC-6 QM designation | `src/tests/sync/stash.rs` + `src/tests/book_value.rs`: flip-rule matrix (all owner/role combinations), live fan-out, non-QM refusal + audit row |
| SC-7 offline queue | `web/tests/` + the E7 offline harness: read-only render, queue, replay, refused-op surfacing |
| SC-8 GM read-only | `web/tests/` component: GM stash screen's zero write affordances; `src/tests/`: GM write refusal + audit |
| authz table | `src/auth/tests/authz.rs`: the new resources' full allow/deny table |
| wire contract | `src/tests/sync/contract.rs` + `web/tests/helpers/wire-contract.js`: pinned field-targets/wire rows |
| UI production-path | `web/tests/party/stash*.test.js`: render + fireEvent drives per the house style rules (roster picker, sell dialog default/flag, settings toggle disabled states, refused-op record) |

Component tests mount through `@testing-library/svelte` on jsdom, drive
real events, and assert visible text — the house production-path rules;
every spec'd affordance gets its non-default state exercised (the sell
dialog's two match outcomes, the toggle's disabled state, the refused-op
record).

## D9 — File map (the whole future diff surface)

Backend: `migrations/<date>e12_shared_inventory_activation{,.down}.sql`;
`src/sync/protocol.rs` (targets + wire shapes); `src/sync/write.rs`
(apply arms, compound ops); `src/sync/snapshot.rs` (sections);
`src/sync/registry.rs` (fan-out kinds); `src/auth/authz.rs`;
`src/book_value.rs` (new top-level module — the book-value route
handler; declared `pub mod book_value;` in `src/lib.rs`, one line;
test-loaded per the tree's standing pattern — `#[cfg(test)]
#[path = "tests/book_value.rs"] mod tests;` at the module's bottom, the
`src/http.rs` ↔ `src/tests/http.rs` precedent); `src/lib.rs` (that
declaration); `src/http.rs` (the route registration in the `protected`
router — the `conditions` precedent at `src/http.rs:149-186`);
`src/tests/sync/stash.rs`, `src/tests/book_value.rs`,
`src/auth/tests/authz.rs`, `src/tests/sync/contract.rs`. Contract docs:
`specs/007-party-sync/contracts/wire-protocol.md` (§3 + §4 + §8's E12
rows, per that file's own change rule) + `field-targets.json`. Frontend:
`web/src/lib/party/stash.js`, `web/src/lib/party/StashScreen.svelte`,
`web/src/App.svelte` (route), `web/tests/party/` suites,
`web/tests/helpers/wire-contract.js` (new kinds).

Nothing outside this map is touched by the implementation. Engine,
E6 sheet internals, E8 effect paths, E10 PWA shell: untouched.
