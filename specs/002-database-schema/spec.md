# Feature Specification: Database Schema (E2)

**Epic**: E2 — Phase 0, P0, depends on E1, parallel with E3/E4 · GitHub issue #4
**Created**: 2026-09-20
**Status**: Draft
**Input**: `docs/EPICS.md` Epic E2 specify prompt + Constraints + AI Guardrails (decomposed from PRD v3.6)

## User Scenarios & Testing *(mandatory)*

E2 ships no user-facing features. Like E1, its "users" are the builders of
the downstream epics — E5 (import), E7 (sync), E8 (engine), E4 (corpus
importer), and eventually E12 (stash/bank) — plus the operator who must
stand the database up in production. Every story below is about what those
builders can rely on the persisted state to guarantee.

### User Story 1 — Migrate up, migrate down, repeatably (Priority: P1) 🎯 MVP

A developer on a clean checkout points the checked-in migrations at a fresh,
throwaway local database and brings it from empty to current schema — and
back to empty — with one documented command each way, as many times as they
like, with zero manual steps.

**Why this priority**: Migrations are the delivery mechanism for everything
else in this epic. If they are not reversible and repeatable against a fresh
database, every downstream epic inherits a broken foundation and local dev
becomes tribal knowledge.

**Independent Test**: On a clean checkout with only the documented local
prerequisites, run the migrate-up command against a fresh throwaway
database; verify all tables exist. Run migrate-down; verify the database is
empty of application tables. Repeat the cycle; it succeeds every time.

**Acceptance Scenarios**:

1. **Given** a fresh, empty local database, **When** the developer runs the
   documented migrate-up command, **Then** every schema object is created
   and the command exits successfully.
2. **Given** a fully migrated database, **When** the developer runs the
   documented migrate-down command, **Then** every migration reverses
   cleanly and no application tables remain.
3. **Given** a database at any intermediate migration, **When** the
   developer migrates up to current and back down, **Then** each step
   reports success or fails loudly — no migration silently half-applies.
4. **Given** a developer with no access to the production database host,
   **When** they follow only the repo documentation, **Then** their entire
   dev loop runs against the local throwaway database and never requires or
   attempts a production connection.

---

### User Story 2 — Second campaign is configuration, not migration (Priority: P1)

An operator (or downstream epic) creates a second party with its own
characters, effects, inventories, and rules rows by inserting data only —
no schema changes, no new columns, no migration — and the two parties' data
never bleeds into each other.

**Why this priority**: This is the epic's headline business guarantee (PRD:
"Second campaign = config, not migration"; the PRD's business metric for the
schema verdict). Multi-party from day one is the cheapest at schema time and
the most expensive to retrofit.

**Acceptance Scenarios**:

1. **Given** a database holding one active party, **When** a second party
   and its characters are created via data inserts alone, **Then** both
   parties coexist with complete, queryable state and no schema change was
   required.
2. **Given** two parties with characters and effects, **When** any
   party-scoped query runs for one party, **Then** it can be constrained to
   exactly that party's rows by a party identifier present on the queried
   tables.
3. **Given** a character row referencing a party that does not exist,
   **When** the insert is attempted, **Then** the database rejects it
   (referential integrity is enforced, not conventional).

---

### User Story 3 — A character survives import and re-import (Priority: P1)

The import pipeline (E5) stores a character's raw import payload and its
normalized base-sheet state, then later replaces the base sheet on re-import
while the live state — HP, spell-slot usage, preparation choices, inventory
deltas — is preserved by stable anchors rather than by position in the
payload.

**Why this priority**: Characters are the product. The PRD's re-import
anchoring rule ("replaces the base sheet while preserving live state…
entities that fail to match are kept, not dropped") is a schema guarantee
first and an importer behavior second: if live state is stored entangled
with payload-derived rows, no importer can honor it.

**Acceptance Scenarios**:

1. **Given** a stored character, **When** its raw payload and normalized
   state are read back, **Then** the raw payload is retrievable verbatim (as
   received) and independently of any derived state.
2. **Given** a character with live state (reduced HP, expended spell slots,
   modified inventory), **When** its payload and base-sheet state are
   replaced wholesale by a newer import, **Then** the live-state rows
   survive untouched because they are keyed to stable anchors (character
   identity; slot rank + index; item name), not to payload structure.
3. **Given** a character whose owner account is known, **When** the
   character is queried by account identity, **Then** exactly one character
   answers — the account-to-character anchor is unique and enforced.
4. **Given** a character belonging to a party, **When** its owner's live
   state changes, **Then** the change is recorded against the character
   without ambiguity about which party scope it belongs to.

---

### User Story 4 — Concurrent writes reconcile by receipt order (Priority: P2)

The sync layer (E7) records every mutable live-state field with a
monotonically increasing, server-assigned version at the settled field
granularity — HP, temp-HP, each spell slot individually, each inventory
item's quantity, each effect as a whole — so that two devices writing the
same field resolve deterministically to whichever write the server received
last, with no locking that a UI flow could stall behind.

**Why this priority**: Offline reconciliation is P0 product behavior
(PRD FG2), but E7 builds the mechanism; E2's job is to guarantee the
bookkeeping exists at the right granularity and never requires
client-visible locking. P2 here only because E7 consumes it after E5/E6
land basic persistence.

**Acceptance Scenarios**:

1. **Given** a live-state field at version N, **When** two writes arrive
   with stale and current expectations, **Then** the server can record both
   in receipt order with strictly increasing versions, and a reader can tell
   which won without any lock having been held across a client interaction.
2. **Given** the settled field granularity, **When** the schema is
   inspected, **Then** every field in that granularity list (HP, temp-HP,
   each individual spell slot, each inventory quantity, each whole effect)
   has an addressable monotonic version — versions are per-field, not merely
   per-row.
3. **Given** a versioned field, **When** a new value is recorded, **Then**
   its version is strictly greater than any version previously recorded for
   that field (versions never repeat or move backward).

---

### User Story 5 — The rules corpus has lanes and provenance (Priority: P2)

The corpus importer (E4) lands rules rows — conditions, items, and later
spells/feats — each carrying a source lane (`core`, `imported`, or
`custom`), and, for imported rows, which upstream pack release produced
them and when; custom rows record which account created them. Rows that
carry engine-computable condition→modifier mappings are distinguishable
from display-only rows.

**Why this priority**: E4 runs in parallel with E2 and needs these tables
to exist to land its import. The lane split is the PRD's governing rule for
content ownership (importer owns structured rows; Dave owns display prose;
users own `custom`) — if lanes or provenance are missing, importer re-runs
cannot be idempotent and homebrew cannot survive them.

**Acceptance Scenarios**:

1. **Given** the corpus tables, **When** a row is inserted, **Then** its
   source lane is one of exactly `core`, `imported`, or `custom` — no other
   value is storable.
2. **Given** an `imported`-lane row, **When** it is inspected, **Then** it
   records the upstream pack version and import date, so a re-run can
   reconcile "same pack, same row" idempotently.
3. **Given** a `custom`-lane row, **When** it is inspected, **Then** its
   creator account is recorded, and nothing about the row's storage
   distinguishes it as second-class relative to imported rows.
4. **Given** a condition row, **When** the engine (E8) asks whether it
   carries structured modifier mappings, **Then** the schema answers
   unambiguously (structured-mapping rows vs. display-only rows are
   distinguishable without parsing prose).

---

### User Story 6 — P1 tables are born ready (Priority: P3)

The shared-inventory epic (E12, P1) finds its tables already present: a
party stash, an append-only claim history, and a party bank ledger — unused
until E12 ships, but fully consistent with this epic's delete, versioning,
and party-scoping rules, so E12 is a feature build, not a schema retrofit.

**Why this priority**: The epic prompt mandates the P1 stash/bank tables at
schema time. P3 because nothing at P0 reads or writes them; their value is
purely that E12 never needs a migration for its core structures.

**Acceptance Scenarios**:

1. **Given** a fully migrated database, **When** the stash, claim-history,
   and bank tables are inspected, **Then** they exist, are party-scoped, and
   carry the same audit timestamps and integrity guarantees as every P0
   table.
2. **Given** the claim-history table, **When** any attempt is made to update
   or delete an existing entry, **Then** the schema provides no such path —
   claim history is append-only by construction.
3. **Given** claim-history entries, **When** one is inspected, **Then** it
   records item, quantity, origin, destination, acting account, and
   timestamp — enough to settle "who took what" arguments without
   reconstruction.

---

### Edge Cases

- **Migration against a non-empty legacy database**: out of scope (no legacy
  exists), but a migrate-up against a partially migrated database must
  resume cleanly or fail loudly, never silently skip.
- **Re-import that renames entities**: live state anchored by item name
  finds no match; the orphaned live-state row is retained (never cascade-
  deleted by payload replacement) so the importer can surface it in the
  post-import diff.
- **Effect targeting a character removed from the party**: referential
  integrity governs what happens to the target link; the effect row itself
  is not silently destroyed by another entity's removal.
- **Two characters claiming one account, or one character with no account**:
  both are rejected by the schema — the account↔character anchor is
  one-to-one and mandatory.
- **Ended effects**: an effect's creator ending it is a state change
  (inactive), not a deletion — provenance for historical breakdowns remains
  queryable.
- **Corpus importer re-run over a hand-touched `custom` row**: `custom` rows
  are never in any import batch's scope by construction of the lane, so
  re-runs structurally cannot overwrite them.
- **Quartermaster designation**: a party setting stored as data (per-party,
  referencing a roster character), requiring no schema change when E12
  activates it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1**: The repo MUST contain versioned, ordered migrations that bring a
  fresh database from empty to the complete current schema, and every
  migration MUST have a tested reverse path back to empty.
- **FR-2**: The repo MUST contain a repeatable, documented provisioning
  path for the production database hall: a dedicated database and role,
  both named `hireling`, the role capped at 20 connections, reachable only
  over the house database network. Provisioning MUST be expressible as
  checked-in artifacts or documented commands, not console click-ops.
- **FR-3**: The documented local development flow MUST run all migrations
  exclusively against the compose-provided throwaway database; no
  documented step or default configuration points at the production
  instance.
- **FR-4**: The schema MUST model parties as first-class entities, and
  every party-scoped entity (characters, effects, stash, bank, claim
  history, and any other party-owned state) MUST carry a party reference.
  No query path may rely on "the only party" existing.
- **FR-5**: Referential integrity MUST be enforced by the database on every
  party-to-entity and character-to-entity relationship, and every such join
  path MUST be indexed.
- **FR-6**: Every entity table MUST carry creation and last-modification
  timestamps.
- **FR-7**: The schema MUST model accounts (keyed by the external identity
  provider's stable subject) and characters such that each character is
  owned by exactly one account, each account owns at most one character,
  and each character belongs to exactly one party. These constraints MUST
  be database-enforced.
- **FR-8**: Character storage MUST keep three concerns separable: the raw
  import payload verbatim as received; the normalized base-sheet state
  derived from it; and the mutable live state (vitals, slot usage,
  preparation, inventory changes, effective level adjustment). Replacing
  the first two MUST NOT require touching the third.
- **FR-9**: Live state that must survive re-import MUST be keyed to the
  PRD's stable anchors — character identity for HP/temp-HP, slot rank +
  index for slot usage and preparation, item name for inventory deltas — so
  payload replacement cannot cascade-delete or orphan-live-state silently.
- **FR-10**: The schema MUST model active effects with: a name, a creating
  (source) character, a set of target roster characters, an ordered set of
  modifiers each carrying type (`circumstance | status | item | untyped`),
  stat (the closed engine vocabulary, including blanket targets), and
  signed value; a free-text duration note; and an active/ended state that
  distinguishes ended effects from deleted ones.
- **FR-11**: The schema MUST model character inventory with per-item
  quantity, container membership (including an extradimensional flag per
  container, whose contents are excludable from Bulk queries), and
  character money.
- **FR-12**: The schema MUST include the P1 shared-inventory tables at
  delivery: a party stash (item, quantity, Bulk, notes), an append-only
  claim history recording item, quantity, origin, destination, acting
  account, and timestamp per transfer or sale, and a party bank ledger
  (gp/sp/cp). These tables are present but unused at P0.
- **FR-13**: The schema MUST model rules-corpus rows — conditions, items,
  and future kinds without structural change — each carrying a source lane
  constrained to exactly `core | imported | custom`; imported rows MUST
  record upstream pack version and import date; custom rows MUST record
  their creator account; rows carrying structured condition→modifier
  mappings MUST be distinguishable from display-only rows without parsing
  prose.
- **FR-14**: The schema MUST provide a monotonically increasing,
  server-assignable version for every mutable live-state field at the
  settled granularity (HP, temp-HP, each spell slot individually, each
  inventory item's quantity, each effect as a whole), such that concurrent
  writes can be recorded in server-receipt order with strictly increasing
  versions and no lock is ever held across a client interaction.
- **FR-15**: The schema MUST apply one delete policy consistently: hard
  delete. Lifecycle end states that must remain queryable (ended effects)
  MUST be modeled as state, not deletion. Claim history MUST be append-only
  regardless — no update or delete path.
- **FR-16**: The schema MUST NOT persist engine-derived stat totals
  (computed AC, stacked attack bonuses, and similar) as editable state;
  persisted numeric state is base-sheet data or live state only. Derived
  numbers are recomputed downstream (E8) from base + active effects.

### Key Entities

- **Party** — one campaign. The scoping root for nearly everything;
  carries party settings (including the quartermaster designation as
  data).
- **Account** — a person known to the app, keyed by the external identity
  provider's stable subject. Owns at most one character; may own none (the
  GM seat). Creates effects and custom corpus rows.
- **Character** — a roster member of exactly one party, owned by exactly
  one account. Aggregates the three storage concerns below.
- **Character import payload** — the raw Pathbuilder export verbatim, as
  received, per character (latest import). The contract shape is the
  reference export: biography/identity fields, ability scores, attributes
  (HP components, speed), proficiencies (saves, Perception, skills, armor/
  weapon/casting classes), feats/specials/lores/languages, equipment with
  containers, weapons/armor/money, one or more spellcasters (tradition,
  casting type, per-day slots, known/prepared lists), focus pools, AC
  breakdown, and companions (pets/familiars).
- **Character base sheet** — the normalized, queryable state derived from
  the payload (stats, proficiencies, slot layouts, item lists). Replaced
  wholesale on re-import.
- **Character live state** — the mutable, anchored, versioned state:
  HP/temp-HP, per-slot usage and preparation, per-item inventory deltas,
  money, and effective-level adjustment. Survives re-import; carries the
  per-field versions.
- **Effect** — a named, creator-owned buff/condition/penalty with target
  roster characters, typed modifiers, a duration note, and an active/ended
  state. Versioned as a whole.
- **Stash / Claim history / Party bank** — the P1 shared-inventory
  structures: party loot list, append-only transfer/sale log, and currency
  ledger. Present at delivery, unused until E12.
- **Corpus row** — a rules-corpus entry (condition, item, or future kind)
  in exactly one source lane, optionally carrying structured
  condition→modifier mappings, with pack provenance (imported) or creator
  (custom).
- **Field version** — the conceptual per-field monotonic version record
  backing offline reconciliation (see Assumptions for modeling latitude).

### Constraints (settled by the epic — non-negotiable)

- Postgres 16 on Asgard (house instance): database `hireling`, role
  `hireling` with `CONNECTION LIMIT 20`, reachable only over the
  `asgard-net` bridge, no published ports.
- Migrations via `sqlx migrate`, checked into the repo.
- The reference Pathbuilder export (the `#pbExport` JSON block in
  `docs/reference/lorum_ipsum_dashboard.html`) is the contract shape of
  imported character data. It carries no version field; robustness to drift
  is the importer's problem (E5), not the schema's.
- Character identity anchor: Authentik account → character.
- Local development uses the compose throwaway Postgres, never Asgard.
- Constitution Article V: boring stack — no additional datastore, search
  engine, or queue alongside Postgres.

### Assumptions

- **Delete policy: hard delete, decided once.** Rows that must remain
  queryable after their lifecycle ends are modeled with state (ended
  effects stay rows with `active = false`; their provenance survives).
  Actual row deletion is rare and administrative (e.g., removing a test
  character). Claim history is append-only regardless of this policy —
  it accepts inserts and nothing else.
- **Per-field versions are modeled conceptually, not prescriptively.**
  The guarantee is per-field monotonic, server-assigned versions at the
  settled granularity; whether that lands as version columns beside each
  field or a sidecar version table keyed by (entity, field) is a
  brainstorm/plan-stage choice. Both satisfy FR-14; the spec does not pick.
- **"P1 stash/bank tables" means present-but-unused.** The stash,
  claim-history, and bank tables ship fully formed in these migrations;
  no P0 code path writes them. E12 activates them with zero migrations
  for the core structures (its own spec owns reconciliation granularity
  for stash/bank fields joining the versioning scheme).
- **Latest payload only.** Each character retains its most recent raw
  import payload, not an import history; the post-import diff is computed
  at import time (E5) and not persisted. If the table ever wants import
  history, that is a later additive migration.
- **Accounts table is minimal and identity-only.** It stores the external
  IdP subject plus display naming; authentication flows, sessions, and
  authorization enforcement are E3. The GM's read-only nature is
  server-enforced (E3), not schema-enforced — the schema does not encode
  role-based write bans beyond ownership anchors.
- **Effective-level adjustment is live state.** The manual level adjust
  (PRD FG1: math rescale only, bounds 1–20) persists as an anchored
  live-state overlay, so a re-import at a new Pathbuilder level does not
  silently erase a mid-session adjustment without a human decision.
- **Corpus kinds are open-ended.** Conditions and items are the POC
  import targets (E4); the corpus structure does not restrict which kinds
  of rules rows can exist, so spells/feats/bestiary land later as data,
  not schema change.
- **One character per account is schema-enforced** per the settled POC
  constraint; reassignment remains a manual DB operation (E3 constraint),
  which hard delete accommodates.
- **Money is live state.** Character currency (gp/sp/cp/pp from the
  export) behaves like other anchored live state so at-table spending
  syncs like HP.
- **Connection-limit headroom.** The role's 20-connection cap is a
  deployment invariant documented in provisioning; the schema itself has
  no per-user connection needs beyond it.

## Success Criteria *(mandatory)*

*(E2 is a data-schema epic; the settled stack is confined to Constraints
and Assumptions. These criteria are written to be verifiable against any
relational database, with the same honest caveat E1 carried — see the
checklist notes.)*

- **SC-1**: On a fresh, empty database, the full schema applies via one
  documented command and fully reverts via one documented command, with
  the cycle repeatable at least three consecutive times with zero errors
  and zero manual intervention.
- **SC-2**: A second campaign — party, five characters, effects, and
  inventory — is creatable using data inserts alone, with zero schema
  changes, and both campaigns' state is independently queryable
  afterward. (This is the PRD's business metric for the schema verdict.)
- **SC-3**: 100% of relationships between parties/characters and their
  dependent entities reject orphaned references at the database level,
  verified by attempting invalid inserts against every such join path.
- **SC-4**: Every entity table records row creation and last-modification
  timestamps; verified by inspecting the migrated schema and by
  round-tripping a write on a sample of tables.
- **SC-5**: A simulated re-import — wholesale replacement of one
  character's payload and base-sheet state — preserves 100% of that
  character's live-state rows (vitals, slot usage, preparation, inventory
  deltas) with no manual repair.
- **SC-6**: Two writes to the same versioned field, recorded in a chosen
  receipt order, yield a final state equal to the later write and two
  strictly increasing versions — with the demonstration holding no lock
  across a simulated client round-trip.
- **SC-7**: The claim-history structure accepts appends and offers no
  update or delete path; verified by attempting all three operations.
- **SC-8**: A new developer reaches a fully migrated local database from a
  clean checkout in under 5 minutes following only repo documentation —
  no production credentials, no production network access.
