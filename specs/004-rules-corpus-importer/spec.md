# Feature Specification: Rules Corpus Importer (E4)

**Epic**: E4 — Phase 0, P0, depends on E1, parallel with E2/E3 · GitHub issue #6
**Created**: 2026-09-20
**Status**: Draft
**Input**: `docs/EPICS.md` Epic E4 specify prompt + Constraints + AI Guardrails (decomposed from PRD v3.6)

## User Scenarios & Testing *(mandatory)*

E4 ships no player-facing features. Its "users" are the operator who seeds and
refreshes the rules corpus (Josh or an agent he drives) and the downstream
epics that consume corpus rows — the buff engine (E8), tooltips and custom
content (E9), and book-value lookups (E12). Every story below is about whether
those consumers can trust the corpus without ever thinking about where it came
from.

### User Story 1 — Initial import seeds the corpus (Priority: P1) 🎯 MVP

The operator, on a machine with the app stack running, runs one documented
command naming a pinned upstream pack release. When it finishes, every Player
Core condition and every item from that release exists as a corpus row in the
`imported` lane, each row stamped with the pack version and the import date,
and the run's per-table counts are recorded.

**Why this priority**: E8's seeded condition picker and E9's tooltips cannot
exist without these rows. This is the single write-path that creates the
structured rules data the whole product computes from — the PRD's ruling is
import-or-nothing; ad-hoc hand-seeding was explicitly rejected.

**Independent Test**: From a clean checkout with an empty database, run the
documented import command against a pinned release. Verify condition and item
rows exist, every row carries a source lane, pack version, and import date,
and the run log shows non-zero per-table counts.

**Acceptance Scenarios**:

1. **Given** an empty corpus and a pinned upstream pack release, **When** the
   operator runs the import, **Then** condition rows and item rows land in the
   corpus, every one in the `imported` source lane.
2. **Given** the import has run, **When** any imported row is inspected,
   **Then** it carries the pack release identifier it came from and the date
   it was imported.
3. **Given** the import has run, **When** the run's log is inspected,
   **Then** it records per-table row counts for that run.
4. **Given** the operator omits the pinned release identifier, **When** the
   import is invoked, **Then** it refuses to run and writes nothing.

---

### User Story 2 — Re-run after a new pack release is boring (Priority: P1)

Weeks later, a new upstream pack release ships. The operator re-runs the same
command pinned to the new release. Rows unchanged upstream are untouched;
rows changed upstream update in place; every refreshed row's provenance
advances to the new pack version and date. Homebrew rows the party created
in-app (`custom` lane) are byte-for-byte identical before and after. Re-running
against the *same* release is a complete no-op.

**Why this priority**: The corpus must track upstream corrections for the
life of the POC without ever endangering the party's homebrew ("500 Toads"
is in the reference export — the custom lane is load-bearing from session
one). An importer that can't be re-run safely is a one-shot script with a
database attached.

**Independent Test**: Import release A; create a `custom`-lane row via the
custom-content path; import release B; import release B again. Verify: A→B
changed only rows that differ upstream, B→B changed zero rows, and the custom
row is untouched across all runs.

**Acceptance Scenarios**:

1. **Given** a completed import of a pinned release, **When** the same command
   is re-run against the same release, **Then** zero rows are created,
   modified, or deleted.
2. **Given** `custom`-lane rows exist, **When** any import run executes
   (initial or re-run, success or failure), **Then** no custom row is
   created, modified, or deleted by the importer.
3. **Given** a newer pinned release in which an upstream row changed, **When**
   the re-run completes, **Then** that row reflects the new content and its
   provenance shows the new pack version and import date.
4. **Given** a newer pinned release in which an upstream row was removed,
   **When** the re-run completes, **Then** the stale row is reported in the
   run output for human review and is never silently deleted.
5. **Given** a previous run imported conditions, **When** a new run would
   import zero conditions, **Then** the run fails as an error (not a silent
   no-op) and the corpus is left exactly as it was.

---

### User Story 3 — Downstream consumers trust the expressibility split (Priority: P1)

The buff engine (E8) builds its seeded condition picker from the corpus:
conditions fully expressible in the engine's closed stat vocabulary arrive
with their condition→modifier mapping rows; everything else arrives as a
display-only row with an explicit tracked-manually classification. No
consumer ever has to guess which tier a condition is in, and no consumer ever
receives a modifier row the importer invented to make a condition "fit."

**Why this priority**: The engine is sacred (Constitution Article IV). The
importer feeds it, and the one unforgivable thing a feeder can do is
fabricate math the rules don't support. The tier split is corpus data owned
by this epic — E8 and E9 hardcode nothing.

**Independent Test**: After import, query the corpus for a condition known to
be fully expressible (e.g. frightened → −X status to all checks and DCs) and
one known to be inexpressible (e.g. concealed — a DC 5 flat check is not in
the stat vocabulary). Verify the first carries modifier mapping rows drawn
entirely from the closed vocabulary, and the second carries none and is
classified display-only.

**Acceptance Scenarios**:

1. **Given** an imported condition fully expressible in the stat vocabulary,
   **When** its corpus rows are inspected, **Then** its modifier mapping uses
   only vocabulary stats and modifier types, and valued conditions carry the
   value as a parameter (frightened 1 and frightened 2 share one mapping).
2. **Given** an imported condition the vocabulary cannot fully express,
   **When** its corpus row is inspected, **Then** it is explicitly classified
   display-only and has zero modifier mapping rows.
3. **Given** a condition only *partially* expressible (part of its text maps
   to the vocabulary, part does not), **When** it imports, **Then** it lands
   display-only — the importer never ships partial math.
4. **Given** any imported condition, **When** a consumer reads it, **Then**
   its tier (engine-math vs display-only) is unambiguous stored data, not
   something the consumer must derive.

---

### User Story 4 — The license verdict gates anything public (Priority: P2)

Before anything about the product is exposed publicly, the operator can
produce the license verdict: green only when the ORC/OGL notice file exists
in the repo, the upstream pack's license text is archived verbatim in the
repo, and every imported corpus row is covered by a license the notice file
documents. Red changes nothing for the private POC deploy — it gates public
exposure only.

**Why this priority**: The POC deploys privately regardless, so this is P2
for go-live — but the artifacts (notice file, archived license text) must
exist from day one because retrofitting provenance over an already-imported
corpus is exactly the kind of work that never happens.

**Independent Test**: With all license artifacts in place, produce the
verdict and observe green. Remove or corrupt any single artifact (notice
file, archived upstream license) and observe the verdict flip red — without
re-running the import.

**Acceptance Scenarios**:

1. **Given** the notice file and archived upstream license text are present
   and every imported lane is covered by the notice, **When** the verdict is
   produced, **Then** it is green.
2. **Given** any required artifact is missing, **When** the verdict is
   produced, **Then** it is red and names the missing artifact(s).
3. **Given** a red verdict, **When** the private POC deploy proceeds,
   **Then** nothing blocks it — the gate binds public exposure only.
4. **Given** the archived upstream license text, **When** it is compared to
   the upstream pack release it was taken from, **Then** it is verbatim and
   identifies the release it was archived from.

---

### Edge Cases

- **Fetch failure mid-run**: transient failures retry with backoff up to a
  bounded limit; a run that exhausts retries fails and leaves the corpus
  exactly as it was — a pack is never half-written.
- **Upstream schema drift**: the importer declares the pack schema version it
  understands; confronted with a schema it doesn't, it fails fast before
  writing anything rather than guessing at fields.
- **Zero-row regression**: a run that would import zero rows of a category
  that previously had rows (upstream moved the pack, the release tag is
  wrong, the parser broke) is an error, not a no-op — nothing is written.
- **Upstream row removal on re-run**: never auto-deleted; surfaced in the run
  report for a human to reconcile (a condition the party is actively tracking
  must not vanish mid-campaign).
- **Duplicate/renamed upstream rows**: identity is matched on stable upstream
  identifiers, not display names, so an upstream rename updates the existing
  row instead of forking a duplicate.
- **Custom-row collision**: a custom row whose name matches an upstream row
  (the party homebrewed "500 Toads"; a later pack prints an item of that
  name) coexists — the lanes keep them distinct; neither overwrites the
  other.
- **Run against a database that already has corpus rows from a newer pack**:
  re-importing an *older* pinned release is allowed (reproducibility) but its
  provenance stamps honestly — rows show the older pack version; the run log
  records the regression in freshness so it's visible.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1**: The system MUST import condition and item documents from a
  pinned release of the upstream community PF2e pack repository into the
  corpus tables. Spells, feats, and bestiary content MUST NOT be imported in
  this epic (importer-capable but deferred — no consumer exists).
- **FR-2**: Every imported row MUST carry a source lane of `imported`. The
  importer MUST NOT write rows in the `core` or `custom` lanes.
- **FR-3**: Re-running the importer against the same pinned release MUST
  change zero rows — no creates, no updates, no deletes, no provenance
  churn.
- **FR-4**: No importer run — successful, failed, or interrupted — MUST
  create, modify, or delete any `custom`-lane row.
- **FR-5**: Each pack's import MUST be transactional: a run that fails or is
  interrupted partway through a pack leaves zero rows written for that pack.
- **FR-6**: Fetch failures MUST be retried with backoff up to a bounded
  limit; exhausting retries MUST fail the run (non-success exit) rather than
  import partial data.
- **FR-7**: Every imported row MUST record the pack release identifier it
  came from and the date it was imported (per-row data-freshness
  provenance).
- **FR-8**: The importer MUST classify every imported condition into exactly
  one of two stored tiers: engine-math (fully expressible in the engine's
  closed stat vocabulary) or display-only. The classification MUST be stored
  corpus data, not derived by consumers.
- **FR-9**: Engine-math conditions MUST carry condition→modifier mapping rows
  expressed solely in the closed stat vocabulary (single stats, `skill:<name>`,
  and the blanket targets with their defined expansion sets) and the four
  modifier types. Valued conditions MUST carry the value as a parameter of
  one shared mapping, not one mapping per value.
- **FR-10**: Display-only conditions MUST have zero modifier mapping rows.
  The importer MUST NOT fabricate, approximate, or partially populate
  modifier rows for any condition — expressibility is all-or-nothing per
  condition.
- **FR-11**: Tier classification and modifier mappings MUST come from
  explicit, human-reviewable seed data checked into the repo. A condition
  with no seed mapping MUST default to display-only (the fail-safe
  direction).
- **FR-12**: Each run MUST log per-table row counts. A run that would import
  zero rows of a category that previously contained rows MUST fail as an
  error and write nothing.
- **FR-13**: The importer MUST declare the upstream pack schema version it
  supports and MUST refuse to run against a pack whose schema it does not
  recognize, failing before any write. Schema drift is handled by shipping a
  new importer version, never by runtime adaptation.
- **FR-14**: The importer MUST require an explicit pinned release identifier
  per run and MUST refuse to run against an unpinned or "latest" source.
- **FR-15**: Upstream row identity MUST be matched on stable upstream
  identifiers, so re-runs update changed rows in place and upstream renames
  do not fork duplicates. Rows present in the corpus but absent from a newer
  pinned release MUST be kept and reported, never silently deleted.
- **FR-16**: The repo MUST contain an ORC/OGL license notice file covering
  every source lane the corpus can contain.
- **FR-17**: The upstream pack's license text MUST be archived verbatim in
  the repo, identifying the pack release it was archived from.
- **FR-18**: The system MUST provide a license verdict — green only when the
  notice file (FR-16) is present, the archived upstream license (FR-17) is
  present, and every imported lane is covered by the notice; red otherwise,
  naming what is missing. The verdict MUST be producible on demand without
  re-running the import, and a red verdict MUST gate public exposure while
  leaving private POC deployment unblocked.

### Key Entities

- **Corpus row**: one imported game element (condition or item). Carries its
  content, source lane (`imported`), pack release identifier, import date,
  and stable upstream identity.
- **Condition tier classification**: per-condition stored verdict —
  engine-math or display-only — plus, for engine-math conditions, the
  modifier mapping (modifier type, vocabulary stat, value or value
  parameter).
- **Import run**: one execution of the importer — pinned release, timestamp,
  per-table counts, outcome, and the report of kept-but-stale rows.
- **License artifacts**: the in-repo ORC/OGL notice file and the archived
  upstream pack license text (with its source release), the inputs to the
  green/red verdict.

### Constraints (settled by the epic — non-negotiable)

- No runtime dependency on Archives of Nethys — no API, no scraping; AoN is
  for citation links in tooltips only. The corpus is seeded exclusively by
  import from the upstream pack repository.
- Pack source is pinned by release tag; "latest" is not a source.
- Schema drift between upstream releases is handled by importer versioning,
  not runtime adaptation.
- Upstream source: the Foundry VTT PF2E system packs (community-maintained,
  JSON) — the PRD's 2026-09-18 ruling approved this source and rejected
  ad-hoc accumulation.
- Constitution Article IV: the importer feeds the engine but must never
  fabricate for it — no modifier math beyond what the seed data and the
  closed vocabulary justify.
- Constitution Article V: the importer lives in the boring stack (the app's
  language and database); new dependencies need written PR justification.

### Assumptions

- **Invocation: one-shot CLI, not an admin endpoint.** The importer runs as a
  documented operator command (a `just` recipe alongside `dev`/`test`) against
  a target database. Sane default per grug: this job runs a handful of times
  per year by one operator; an HTTP endpoint adds auth surface and lifecycle
  code for zero benefit. Flagged for the clarify stage if Josh disagrees.
- **Corpus tables land with this epic's own migrations.** E4's declared
  dependency is E1 only and it runs parallel to E2 (schema); the corpus
  tables are therefore owned by this epic's migrations and reconcile with
  E2's hall at merge. Table *shape* beyond the fields named here is plan-stage.
- **"Importer-capable but deferred"** means: spells/feats/bestiary packs are
  neither fetched nor parsed in this epic, and no tables or flags are built
  for them. The only forward-compatibility promised is that adding a
  category later is a new importer version, not a redesign.
- **Remaster content only at POC** (PRD: PF2e Remaster only). The notice file
  still documents both ORC and OGL so a future legacy-pack import is a
  data decision, not a legal one.
- **Removals are reported, never auto-deleted** — the safe direction for a
  party mid-campaign; a human reconciles stale rows deliberately.
- **Tier seed data is human-authored and reviewed** (Josh/Dave), covering at
  minimum every Player Core condition, checked into the repo — same
  provenance discipline the PRD sets for spell outcome templates
  (never model-generated straight into the product). The prototype's
  42-condition map is the coverage checklist, not a structured source.
- **The in-app about/license view** that renders the notice file is a
  display concern owned by a UI epic (E9/E10); E4 ships the notice file as
  the canonical source and the verdict mechanics. Recorded here because the
  PRD's green definition mentions the in-app view — it is not E4's to build.
- **Items import wholesale** from the pinned packs (all item types);
  consumers (FG4 book value) filter and match downstream by name.
- **The import target** is the same Postgres the app uses — Asgard hall for
  real runs, the compose throwaway for dev — selected by the same env-var
  config discipline as E1.

## Success Criteria *(mandatory)*

- **SC-1**: A second run over the same pinned release changes zero rows
  (verified by row-level comparison before/after) and leaves every `custom`
  row byte-identical.
- **SC-2**: Killing the importer mid-pack (or cutting its network) leaves the
  corpus exactly as it was before the run — verified by re-running the
  interrupted import to completion and confirming counts match a clean run.
- **SC-3**: Every imported row answers "which pack release and when" from its
  own stored fields — verified by sampling rows after import.
- **SC-4**: Every imported condition is unambiguously tiered: a single query
  separates engine-math conditions (with vocabulary-only mappings) from
  display-only ones (with zero mappings), and no condition sits in both or
  neither.
- **SC-5**: The zero-row regression tripwire fires: pointing the importer at
  a source where a previously-populated category resolves to zero rows fails
  the run loudly and preserves all existing rows.
- **SC-6**: The license verdict flips correctly: green with all artifacts in
  place, red naming the culprit when any one is removed — reproducible on
  demand without an import run.
- **SC-7**: A full import from a clean checkout (empty database, documented
  command, pinned release) completes in under 10 minutes on a dev machine.
- **SC-8**: The run log for any execution answers "how many rows per table,
  what release, what outcome, what went stale" without inspecting the
  database.
