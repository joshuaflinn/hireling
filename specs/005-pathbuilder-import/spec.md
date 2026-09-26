# Feature Specification: Pathbuilder Import Pipeline (E5)

**Epic**: E5 — Phase 1, Lane A, P0, depends on E2+E3, blocks E6 · GitHub issue #7
**Created**: 2026-09-26
**Status**: Draft (pipeline step 1 of 4: specify)
**Input**: `docs/EPICS.md` Epic E5 specify prompt + Constraints + AI Guardrails (decomposed from PRD v3.6)

## User Scenarios & Testing *(mandatory)*

E5's "user" is a player with a Pathbuilder 2e character on their phone and a
JSON export in their clipboard. The product promise being tested: the sheet
arrives complete, the live table state it replaces never evaporates, and every
way an import can go wrong gets a sentence a human can act on.

### User Story 1 — A player imports their character (Priority: P1) 🎯 MVP

A signed-in player pastes (or uploads) their Pathbuilder JSON export. The full
character derives from it, joins the party roster, and is owned by the
importing account from that moment. Everything the sheet needs — identity,
stats, proficiencies, spell slots, inventory with containers, weapons, money —
is stored server-side.

**Why this priority**: The roster metric ("5/5 players have imported sheets")
starts here. E6 renders, E7 syncs, E8 computes — none of it exists until an
import put a character row in the hall.

**Independent Test**: As a player account, paste the reference export
(`docs/reference/lorum_ipsum_dashboard.html`, `#pbExport` block), then read
the character back: identity, ability scores, AC breakdown, every proficiency,
both caster blocks with their slot layouts, all 16 equipment items with
container membership, weapons, armor, and money match the export.

**Acceptance Scenarios**:

1. **Given** a signed-in player with no character, **When** they submit a
   valid Pathbuilder export, **Then** a character is created owned by their
   account, joined to the POC party, and the response confirms what imported.
2. **Given** the importing account, **When** anyone in the party (or the GM)
   later reads the roster, **Then** the imported character is present with its
   derived sheet.
3. **Given** a valid export, **When** the import completes, **Then** current
   HP starts at the character's maximum HP, temp HP at 0, and money at the
   export's coin counts — the character arrives ready for the table.
4. **Given** a martial character's export (no `spellCasters` content),
   **When** it is imported, **Then** the import succeeds with an empty
   spellcasting section.

### User Story 2 — Re-import preserves the session (Priority: P1)

The player leveled up in Pathbuilder and re-exports mid-campaign. The base
sheet is replaced wholesale; the live table state — HP and temp HP, money,
spent/unspent slots, prepared spells, inventory quantity changes — survives
untouched, anchored exactly. Anything that no longer matches the new export
(a renamed item, a vanished slot, a swapped prep) is kept and named in a
post-import diff. Nothing is dropped.

**Why this priority**: "Worst case is a manual re-export, never data loss" is
the epic's settled constraint. Anchoring is the mechanism that makes a
re-export boring.

**Independent Test**: Import the reference export, mutate live state (drop
HP, spend slots, prep a spell, mark two chalk consumed), then re-import a
modified export (rename an item, shrink a slot rank, change a prepared
spell). Verify: HP/temp-HP/money/used/prep/deltas all preserved verbatim; the
diff names the kept-but-unmatched item and slot and the prep divergence.

**Acceptance Scenarios**:

1. **Given** an existing character with live state, **When** the owner
   re-imports any export, **Then** active effects, HP, temp HP, money,
   level-adjust, slot usage, slot prep, and inventory deltas are all
   unchanged by the import itself.
2. **Given** a re-import where a live entity no longer matches the new export
   (item name absent, caster block gone, slot beyond the new layout),
   **When** the import completes, **Then** that live entity is kept and
   surfaced in the returned diff — never dropped, never reset.
3. **Given** a re-import where the new export disagrees with live prep on a
   still-matching slot, **When** the import completes, **Then** live prep
   wins and the divergence appears in the diff.
4. **Given** the identical export imported twice in a row, **When** the
   second import completes, **Then** zero live state changes and the diff is
   empty.

### User Story 3 — Bad input gets a human answer (Priority: P1)

A player pastes the wrong clipboard, a truncated file, or a future export
shape. The import fails with one of three failure classes, each carrying an
exact, human-readable message — never a stack trace, never silence. Unknown
fields never fail an import: they are logged and skipped.

**Why this priority**: Six non-technical users at a table. The difference
between "500 Internal Server Error" and "that isn't a Pathbuilder export" is
the difference between adoption and abandonment.

**Independent Test**: Submit (a) truncated garbage, (b) valid JSON that is
not a Pathbuilder export (e.g. `{"hello":"world"}`), (c) the reference export
with an injected unknown top-level field. Verify the exact messages for (a)
and (b), and success-plus-logged-skip for (c).

**Acceptance Scenarios**:

1. **Given** a body that is not valid JSON, **When** submitted, **Then** the
   import is rejected with message **"That isn't valid JSON. Copy the whole
   export from Pathbuilder (Share → Export JSON) and paste it again."**
2. **Given** valid JSON without the required Pathbuilder shape, **When**
   submitted, **Then** the import is rejected with message **"That's valid
   JSON, but it doesn't look like a Pathbuilder export — the character sheet
   fields (name, level, abilities, proficiencies) are missing."**
3. **Given** a valid export carrying unknown fields, **When** submitted,
   **Then** the import succeeds, every unknown field is logged server-side,
   and the response reports how many fields were skipped.
4. **Given** any input larger than the size cap or nested deeper than the
   depth cap, **When** submitted, **Then** the import is rejected before
   parsing with a human-readable message naming the limit.

### User Story 4 — Ownership is claimed and enforced at import (Priority: P2)

The importing account owns the character, full stop. A GM account cannot
import. An anonymous visitor cannot import. A player who already owns a
character re-imports *their own* character — there is no second character, no
character picker. Every attempt, allowed or denied, is recorded with the
account and outcome.

**Why this priority**: The ownership machinery (E3) exists; this story is its
first real exercise. It is P2 only because it rides the same middleware and
matrix already proven in E3 — the story's test is the route-level guard.

**Independent Test**: As the GM account, POST an import (expect the standard
forbidden payload plus an audit row). Unauthenticated, POST an import
(expect rejection). As a player with an existing character, POST a different
character's export (expect your own character's base sheet replaced — the
re-import path — with the diff explaining what happened).

**Acceptance Scenarios**:

1. **Given** the GM account, **When** it submits any import, **Then** the
   server rejects the write (standard forbidden payload) and records the
   rejection — the GM never owns a character.
2. **Given** an unauthenticated visitor, **When** they submit an import,
   **Then** they are rejected before any validation work.
3. **Given** a player who already owns character X, **When** they import any
   export, **Then** the target is character X (base sheet replaced, live
   state anchored) — the account cannot acquire a second character, and the
   import never touches another account's character.
4. **Given** any import attempt, **When** it completes or is rejected,
   **Then** an audit record names the acting account and the outcome.

### Edge Cases

- **`success: false` envelope** — an export object whose `success` is false,
  or whose `build` is missing/null, is failure class (b): it is not a usable
  export. Same message.
- **Martial characters** — `spellCasters` absent or `[]` imports cleanly;
  required-key detection (class b) must not demand spellcasting.
- **Duplicate caster block names** — two blocks with the same `name` (e.g. a
  dual-classed or archetype build): anchors get deterministic ordinal
  suffixes (see FR-10 tie-break).
- **Innate caster blocks** — a block with `innate: true` and non-zero
  `perDay` still materializes slot anchors; the fixture's Wellspring Gnome
  (one innate cantrip) is the regression case.
- **Homebrew content** — "500 Toads" prepared at rank 1 imports as a name
  like any other; Hireling never validates spell or item names against a
  rules source.
- **Same item name, different casing** — item anchors match
  case-sensitively; "500 Toads" and "500 toads" are two distinct items.
- **Container UUID with no matching container entry** — the item imports
  with no container membership and the dangling reference is logged as an
  unknown-field-class notice.
- **`dualClass: null`** — imports as absent dual class; a string dual class
  imports as-is (Hireling displays it, does not compute from it).
- **Unknown fields nested inside known sections** — class (c) logging covers
  nested paths (e.g. `build.attributes.newFutureField`), not just top-level
  keys.
- **Slot ranks beyond prepared lists** — `perDay` may grant more slots than
  the prepared list names; unlisted slots exist, unprepared. Never an error.
- **Re-import after character deletion by an admin** — the next import is a
  first import (fresh character, fresh seeds); no stale live state survives a
  hard-deleted character (E2 cascade).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1**: The system MUST accept a Pathbuilder 2e JSON export — pasted as
  the request body or uploaded as a file — from an authenticated player
  account, and derive the full character sheet from it, stored server-side.
- **FR-2**: The import contract MUST be the reference export embedded in
  `docs/reference/lorum_ipsum_dashboard.html` (the `#pbExport` JSON block),
  frozen with the prototype. That shape is the spec; no version pin exists
  and none may be invented.
- **FR-3**: Every import MUST be validated and sanitized server-side before
  any storage: body size capped at **1 MiB** (1,048,576 bytes) and JSON
  nesting depth capped at **64** levels. Inputs over either cap MUST be
  rejected before parsing with a human-readable message naming the limit.
  Caps are enforced server-side regardless of any client-side checks.
- **FR-4**: Failure class (a) — a body that is not valid JSON — MUST be
  rejected with exactly: "That isn't valid JSON. Copy the whole export from
  Pathbuilder (Share → Export JSON) and paste it again."
- **FR-5**: Failure class (b) — valid JSON that is not a Pathbuilder export —
  MUST be detected by the required-shape rule: the top level is an object
  with a truthy `success` and a `build` object carrying, at minimum, string
  `name`, integer `level` (1–20), object `abilities` with the six ability
  scores, and object `proficiencies`. Anything else MUST be rejected with
  exactly: "That's valid JSON, but it doesn't look like a Pathbuilder export
  — the character sheet fields (name, level, abilities, proficiencies) are
  missing." No other key may be required (class (c) carries drift).
- **FR-6**: Failure class (c) — unknown fields — MUST NOT fail the import.
  Every unknown field (top-level, or nested inside a known section) MUST be
  logged server-side with the acting account and a count, and the response
  MUST report the number of skipped fields.
- **FR-7**: A successful first import MUST create the character owned by the
  importing account (`owner_sub` = the session's account), in the single POC
  party, with the export stored verbatim (`payload_raw`, byte-for-byte as
  received) and the normalized sheet (`base_sheet`) derived from it.
- **FR-8**: One character per account. A player importing while already
  owning a character MUST land on the re-import path for that character —
  the system MUST NOT create a second character for the account, and MUST
  NOT touch any other account's character. Character reassignment remains a
  manual admin/DB operation (PRD FG2).
- **FR-9**: First import MUST seed live state to a session-ready baseline:
  current HP = the character's maximum HP (derived from the sheet's HP
  inputs), temp HP = 0, money = the export's coin counts, level-adjust = 0.
- **FR-10 — Anchoring (re-import)**: A re-import MUST replace `payload_raw`
  and `base_sheet` wholesale and preserve live state by these exact anchors:
  - **HP / temp-HP / money / level-adjust**: anchored by character identity
    (the `character_vitals` row). Never read from, compared against, or
    overwritten by the new export.
  - **Slot usage and prep**: anchored by `(caster_key, rank, slot_index)`.
    `caster_key` = the caster block's `name` when unique within the export;
    when a name repeats, blocks after the first get `name#2`, `name#3`, … in
    array order (the tie-break — deterministic across re-imports). A live
    slot row is *matched* iff the new export still contains that caster_key
    (same rule applied to the new block list) and `perDay[rank]` still
    exceeds `slot_index`. Matched rows keep `used` and `prepared_spell`
    untouched.
  - **Inventory deltas**: anchored by exact item name (case-sensitive).
    A live delta row is *matched* iff the new export's equipment list
    contains that name. Matched rows keep `qty_delta` untouched; the
    displayed quantity is new-base + delta, and a delta that drives the
    displayed quantity below zero is surfaced in the diff, not corrected.
- **FR-11 — Unmatched entities**: Live entities (vital state aside) with no
  match in the new export — a vanished item, a removed caster block, a slot
  beyond the new layout — MUST be kept unchanged and surfaced in the
  post-import diff. The diff MUST name each kept-unmatched entity and each
  prep divergence (live prep ≠ export prep on a matched slot). The diff MUST
  be empty when nothing diverges.
- **FR-12 — New anchors seed from the export**: Slot positions that exist in
  the new export but have no live row (first import; layout growth; slots
  the player never touched) MUST be seeded from the export: `used` = false,
  `prepared_spell` = the export's prepared list value at that position, or
  NULL when the prepared list names nothing there. Inventory items get no
  live rows at import time (absent row = zero delta).
- **FR-13**: Re-import MUST be idempotent per FR-10–FR-12: importing the
  identical export a second time changes no live state and returns an empty
  diff.
- **FR-14**: Active effects MUST be untouched by import and re-import — no
  read, no write, no cascade. They live server-side (E2 `effects` tables)
  and this epic never modifies them.
- **FR-15**: The GM account MUST be unable to import (rejected by the
  existing server-side GM write enforcement, audited). Unauthenticated
  requests MUST be rejected before validation. No route added by this epic
  may bypass the ownership matrix; the write route MUST carry an explicit
  authorization rule in the route registry.
- **FR-16**: Every import attempt MUST be audit-logged with the acting
  account and outcome (allowed or denied, and for rejections, the failure
  class), in the append-only audit trail alongside E3's existing events.
- **FR-17**: The import response MUST return, on success: a character
  summary, the post-import diff (FR-11), and the skipped-field count
  (FR-6); on failure: the exact class message. The diff and summary shape
  is contract data (see `contracts/`).
- **FR-18**: A minimal import page MUST exist for players: paste area, file
  upload, submit, and the result with diff — functional, no design pass.
  The live sheet itself is E6's surface; this page is the import interaction
  only.

### Key Entities

- **Export (input)**: the verbatim Pathbuilder JSON as received. Stored
  byte-verbatim as `characters.payload_raw`.
- **Base sheet (derived)**: the normalized `characters.base_sheet` document
  E5's parser produces and E6/E8 consume. Shape owned by this epic
  (`data-model.md`).
- **Live state (preserved)**: `character_vitals`, `character_spell_slots`,
  `character_inventory_live` — E2's anchor-keyed tables. Import seeds
  (first) or preserves (re-import); never recomputes.
- **Post-import diff**: the returned list of kept-unmatched entities, prep
  divergences, negative-quantity flags, and new-anchor seeds. Review
  surface, not a mutation.
- **Import attempt (audit)**: `{actor, outcome, failure class?, target}`
  appended to the audit trail per attempt.

### Constraints (settled by the epic — non-negotiable)

- The export schema is unofficial and can drift; robustness comes from
  failure class (c), never from a version pin. Worst case is a manual
  re-export, never data loss.
- Active effects live server-side and are untouched by re-import (FR-14).
- Storage is E2's landed schema: `payload_raw` text + `base_sheet` jsonb +
  the three live-state tables. E5 adds no tables and no columns. The two
  migration findings this epic raises (party seed, audit event kind — see
  Assumptions) are data/constraint changes flagged for gate review, not
  schema redesign.
- AuthN/AuthZ is E3's landed contract: session-resolved `SessionAccount`,
  the pure ownership matrix, the route registry, and the standard forbidden
  payload. E5 binds, does not re-derive.
- Class features that reshape slot layouts (Staff Nexus, school spells,
  flexible spellcasting) are handled per-feature as real exports surface
  them — every quirk lands as an importer test case. The reference export
  covers exactly one build, not the feature space.

### Assumptions

- **Party provisioning (raised finding)**: `characters.party_id` is NOT
  NULL and E2 seeds no parties, so the first import needs a party row. E5
  proposes one reversible data migration creating the single POC party
  (guarded: inserts only when `parties` is empty). Rejected alternative:
  auto-creating the party inside the import handler — hides the multi-party
  config story behind a write path. Approved at the gate or E5 does not
  ship the migration.
- **Audit event kind (raised finding)**: E3's `audit_events.event` CHECK
  admits seven login/forbidden kinds — none names an import. E5 proposes a
  reversible migration adding `character_import` to the CHECK. Rejected
  alternative: tracing-only logging — the epic guardrail says audit-log
  attempts, and ownership is claimed at import; a tracing line is not an
  audit trail.
- **Size/depth justification**: the frozen reference export is 5,494 bytes
  at depth 8. Real high-level exports with full repertoires run tens of
  kilobytes — two-plus orders of magnitude under 1 MiB; the cap is DoS
  hygiene, not correctness. Depth 64 is 8× the fixture's deepest path and
  far under any legitimate export.
- **HP/temp-HP on re-import**: live values are preserved verbatim even when
  the new export implies a different max (level-up). Clamping to the new max
  and its UI are E6/E8 concerns; the diff carries the old and new max so the
  player sees the change.
- **Max HP derivation** (first import, FR-9) follows the PRD/prototype
  model from the export's own HP inputs (`ancestryhp + classhp +
  bonushp + bonushpPerLevel×(level−1)` keyed by level); the formula is
  contract data in `contracts/pb-export.md`.
- **Import UI boundary**: the minimal page (FR-18) is functional only; Dave's
  prototype styling lands with E6, which owns every sheet surface.

## Success Criteria *(mandatory)*

*(E5 is a pipeline epic; its users are six people at a table. Criteria are
verifiable without implementation details.)*

- **SC-1**: The frozen reference export imports with zero failures and zero
  skipped fields, and every sheet section reads back matching the export
  (US-1 independent test, executed as an automated test).
- **SC-2**: Each failure class (a), (b), size-cap, and depth-cap rejection
  returns its exact message — asserted verbatim in automated tests — and
  never a raw error or stack trace.
- **SC-3**: The US-2 re-import drill (mutate live state → re-import a
  modified export → verify preservation + diff) passes as an automated
  test, including the empty-diff idempotency case (FR-13).
- **SC-4**: Every route this epic adds is covered by the ownership-matrix
  route tests: GM rejected with audit, unauthenticated rejected, owner
  accepted, non-owner paths nonexistent-by-construction (self-scoped
  routes).
- **SC-5**: Importing with the same account twice never yields two
  characters (verified by constraint-level test against the unique
  owner binding).
- **SC-6**: All six provisioned accounts can hold imported characters
  simultaneously in one party (roster test with six accounts).
