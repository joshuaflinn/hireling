# Design: Rules Corpus Importer (E4)

**Epic**: E4 — Phase 0, P0, depends on E1, parallel with E2/E3 · GitHub issue #6
**Created**: 2026-09-20 · **Status**: Draft for human review
**Consumes**: `spec.md` (approved), `contracts/foundry-packs.md` (captured fixtures), `data-model.md` (row contract handed to E2)

> Standing rule honored throughout: **E2 owns all DDL.** This document specifies
> ROW SHAPES ONLY, as an interface contract E2's migration branch implements.
> Column-level requests are enumerated in `data-model.md` and flagged as
> coordination points, never assumed.

## Decisions at a glance

| Fork | Decision | Why |
|---|---|---|
| CLI placement | Subcommand of the existing `hireling` binary (`hireling import --release <tag>`, `hireling license-verdict`), argv hand-parsed, no `clap` | Grug + Article V: one operator, a few runs a year. A subcommand keeps one binary, one config path (E1's env discipline), one logging setup. `clap` is a new dependency needing PR justification for two commands and three flags — hand-rolled parsing of `argv` is ~40 boring lines. Escape hatch: if the CLI grows past ~3 subcommands, adopt `clap` with written justification. |
| Fetch strategy | Release **asset** (`json-assets.zip`) from the pinned GitHub release, sha256-verified against the digest in the release API response; per-file raw fetch as documented fallback | One HTTP GET per run vs hundreds; the API response carries a `digest: sha256:…` per asset (captured, see contracts) so integrity is checkable without trusting the transport. Zip internal layout is `assumed-until-probed` (contracts doc); fallback = GitHub tree API + raw file fetches, slower but needs no zip assumptions. |
| Transform | Fetch-then-transform-**in-memory**; no staging tables | Conditions ≈ 50 docs, equipment ≈ low thousands, tens of MB worst case. A dev machine yawns. Staging tables add DDL (not ours to write), failure modes, and cleanup for zero benefit at this scale. |
| Idempotence | **Source-ID upsert with content-hash skip**: identity on upstream `_id`; sha256 of the raw source document stored per row; a row whose hash AND importer version are unchanged is not written | FR-3 (same release → zero writes) falls out: unchanged upstream doc → same hash → no write, no `updated_at` churn. FR-15 (rename updates in place) falls out: `_id` survives renames. A pure content-hash key would fork duplicates on rename; a pure source-ID upsert without hash would churn provenance on every run. |
| Expressible-vs-display split | Human-reviewed seed file `data/seed/condition-tiers.json`, keyed by upstream `_id` (slug + name carried for readability); unmapped condition → **display-only** + named in the run report | FR-8..FR-11. The fail-safe direction is display-only; the run report surfaces newly appeared unmapped conditions so the seed gets extended by PR. The importer NEVER reads Foundry `rules[]` arrays to derive math — they may inform a human authoring the seed, but auto-derivation is fabrication-shaped. |
| Transaction boundary | One DB transaction **per pack category** (conditions tx, equipment tx), opened only after fetch+validate of the whole category succeeds | FR-5. Fetch and validation are all-or-nothing before any write; the tx then covers only writes. Kill mid-tx → rollback → corpus untouched (SC-2). |
| Importer versioning | `IMPORTER_VERSION` integer constant in code, stamped on every row and run; importer declares the document shapes it supports and fails fast on anything else | FR-13. Schema drift = new importer version, never runtime adaptation. Tier-seed changes ship with a version bump so re-running the same release after a seed change updates tier columns honestly. |

## Pipeline stages

```
hireling import --release pf2e-8.5.1
 │
 ├─ 0. Arg & config check
 │     release tag present and matches ^pf2e-\d+\.\d+\.\d+$ (refuse "latest",
 │     refuse sf2e-* — the repo's /releases/latest endpoint returned a
 │     Starfinder tag; see contracts). DB URL from E1 env discipline.
 │
 ├─ 1. Fetch  (retried, backoff, bounded — FR-6)
 │     a. GET release metadata for the pinned tag (api.github.com)
 │     b. GET json-assets.zip; verify sha256 against the API-provided digest
 │     c. (operator step, see License gate) license texts already archived
 │        in-repo; import does NOT fetch them
 │
 ├─ 2. Extract & enumerate
 │     packs/pf2e/conditions/*.json → condition docs
 │     packs/pf2e/equipment/**/*.json → item docs
 │     Everything else in the archive is ignored (FR-1: no spells/feats/bestiary).
 │
 ├─ 3. Validate (BEFORE any write — FR-13)
 │     Every doc must match the declared shape for its category (required
 │     paths enumerated in contracts/foundry-packs.md). One bad doc fails
 │     the whole run. Zero docs in a category that previously had rows
 │     fails the whole run (FR-12 zero-row tripwire).
 │
 ├─ 4. Transform (pure — no I/O; unit-tested per Constitution)
 │     Per doc: extract corpus row fields (identity, name, slug, content,
 │     publication license/title/remaster), sha256 the raw doc bytes,
 │     look up the tier seed (conditions only):
 │       mapped engine-math  → tier + modifier rows (closed vocabulary only,
 │                             valued conditions share one parameterized
 │                             mapping — FR-9)
 │       mapped display-only → tier, zero modifier rows
 │       unmapped            → display-only, recorded for the run report
 │
 ├─ 5. Load — one transaction per category
 │     Upsert by upstream _id into lane 'imported':
 │       hash + importer_version unchanged → skip (no write)
 │       changed or new                    → INSERT/UPDATE, stamp
 │                                           pack_release, imported_at,
 │                                           importer_version
 │     Rows in the corpus but absent from this release → kept, collected
 │     into the stale report (FR-15; never deleted — SC/edge case).
 │     Lane filter on every statement: WHERE lane = 'imported'. Custom
 │     rows are invisible to the importer (FR-4).
 │
 └─ 6. Run report
       One import_runs row: release, importer_version, per-table counts
       (inserted/updated/skipped/stale), outcome, and the stale list +
       unmapped-condition list. Same content to stdout (structured log).
       SC-8: the report answers "how many per table, what release, what
       outcome, what went stale" without touching the DB.
```

`hireling license-verdict` shares stages 0 (config) only, then runs the
license-gate checks (below) and exits 0/1. It never imports.

## Source-of-truth mapping

| Product concept | Source of truth | Notes |
|---|---|---|
| Structured rules rows (conditions, items) | Foundry pack docs at the pinned release | Only source. No AoN, no hand-entry (PRD ruling 2026-09-18). |
| Tier classification + modifier mappings | `data/seed/condition-tiers.json` (human-authored, PR-reviewed by Josh/Dave) | The prototype's 42-condition map is the coverage checklist, NOT a structured source (spec assumption). |
| Display prose / tooltips | Dave's curation layer (E9/E15) | E4 stores the raw pack doc; prose is someone else's lane. |
| License coverage | In-repo NOTICE file + archived upstream license texts | See License gate. |
| Corpus table DDL | **E2's migrations** | `data-model.md` is E4's column-level request, not DDL. |

## Lane enforcement

Three layers, cheapest first:

1. **Code discipline**: every INSERT/UPDATE the importer issues sets and
   filters `lane = 'imported'` literally. There is no code path that names
   another lane.
2. **Contract request to E2** (coordination point): a `CHECK (lane IN
   ('core','imported','custom'))` on the corpus table and a partial unique
   index `UNIQUE (kind, upstream_id) WHERE lane = 'imported'`. The index is
   what makes the upsert race-free and makes a custom-row name collision
   harmless (custom rows aren't under the index).
3. **Test**: the custom-lane preservation test creates a `custom` row
   ("500 Toads"), runs every import scenario including failures, and asserts
   byte-identical survival (FR-4, SC-1).

## Provenance model

Every corpus row answers, from its own stored fields (SC-3):

- **Where from**: `upstream_id` + `pack_release` (the pinned tag).
- **When**: `imported_at` (this row's last write — skipped rows keep their
  original stamp; provenance advances only when content actually changed).
- **By what**: `importer_version`.
- **Under what license**: `publication_license` / `publication_title` /
  `remaster`, extracted from the doc's own `system.publication` block
  (captured in fixtures — the data carries its license per row).
- **What changed**: `content_hash` — the diff-detection field; comparing
  hash across releases is the whole update decision.

Every run answers from `import_runs`: release, importer version, when,
per-table counts, outcome, stale list, unmapped-condition list.

## License-gate mechanics (FR-16..FR-18)

Artifacts (all in-repo, all present from day one):

- `NOTICE.md` at repo root — names ORC and OGL 1.0a, states which corpus
  lanes each covers (`imported` → ORC for remaster / OGL 1.0a for legacy,
  `custom` → party homebrew / Paizo CUP paraphrase, `core` reserved).
- `licenses/foundry-pf2e/ORCLicense.md` and `…/OpenGameLicense.md` —
  archived **verbatim** from the pinned release's `static/licenses/`
  (captured: both exist at tag `pf2e-8.5.1`, shas in contracts).
- `licenses/foundry-pf2e/SOURCE.md` — the release tag the archive was
  taken from, the archive date, and sha256 of each archived file.
  Archiving is a documented `just` recipe (`just license-archive
  RELEASE=pf2e-8.5.1`) that downloads the two files from the pinned tag,
  writes them, and writes SOURCE.md. The operator commits the result.

`hireling license-verdict` checks, naming every failure:

1. `NOTICE.md` exists and mentions ORC, OGL, and every lane the corpus
   can contain.
2. Both archived license files exist and their sha256 matches SOURCE.md.
3. Every distinct `publication_license` value across imported rows in the
   DB is a license the NOTICE file covers. (A future OGL-only legacy pack
   is exactly the case this catches.)

Green only if all three pass. Red gates **public exposure only** — the
private POC deploy proceeds regardless (spec US-4). The verdict reads the
DB but never writes; it is runnable any time without re-importing.

## Failure / retry model

| Failure | Behavior |
|---|---|
| Fetch failure (network, 5xx, GitHub rate limit) | Retry with exponential backoff, bounded (5 attempts, jittered). Exhaustion → run fails non-zero, nothing written (FR-6). |
| Asset digest mismatch | Fail immediately, no retry — that's tampering or corruption, not transience. |
| Validation failure (schema drift, missing fields) | Fail before any write, naming the doc and the violated expectation (FR-13). |
| Zero-row tripwire | Fail before any write; corpus preserved (FR-12, SC-5). |
| Kill/crash mid-category | Transaction rolls back; the category is untouched. Re-running completes cleanly (SC-2). |
| Upstream row removed in newer release | Kept, listed stale in the run report. Never auto-deleted. Human reconciles (spec edge case; deliberate — a tracked condition must not vanish mid-campaign). |
| Re-import of an OLDER release | Allowed; provenance stamps honestly (older tag); run log notes the freshness regression (spec edge case). |
| DB unreachable / wrong hall | Fail at connect, before fetch. Same env-var discipline as E1; dev = compose throwaway, real = Asgard hall (spec assumption). |

## Testing strategy

Unit (pure transform layer, sibling-file tests per E1 convention):

- Parse + transform the **captured fixtures** (`frightened.json`,
  `wayfinder.json` — committed under test fixtures from
  contracts/foundry-packs.md, captured not authored).
- Tier classification: mapped engine-math → vocabulary-only mapping rows;
  mapped display-only → zero rows; unmapped → display-only + reported;
  partially-expressible → display-only (FR-10 all-or-nothing); valued
  condition → one parameterized mapping (frightened 1 and 2 share it).
- Content-hash skip: same doc twice → second produces no write-plan entry.
- Rename simulation: same `_id`, changed `name` → update plan, not insert.
- Seed validation: seed file referencing a stat outside the closed
  vocabulary fails at seed load, not at import.

Integration (compose throwaway Postgres, the E1 pattern):

- Clean import → rows exist, all lane `imported`, provenance fields set,
  counts non-zero (US-1).
- Same release twice → zero writes, zero `imported_at` churn (FR-3, SC-1).
- Release A → B fixture pair with one changed doc, one removed doc,
  one renamed doc → changed updated in place, removed kept + reported
  stale, renamed updated not duplicated (US-2).
- **Zero-row regression tripwire**: point a re-run at a fixture whose
  conditions directory is empty → run fails, corpus row-for-row identical
  (SC-5).
- Custom-lane preservation across success, failure, and interruption
  (kill via injected failure mid-transaction) (FR-4, SC-2).
- License verdict: green with artifacts; red naming the culprit with each
  artifact removed in turn; verdict runs without any import (SC-6).

Fixture discipline: all upstream JSON in tests is captured verbatim from
the pinned release (the contracts doc is the citation), never hand-authored
— except the deliberately-broken variants (empty dir, missing field),
which are mutations of captured fixtures, labeled as such.

## Alternatives considered

**A. Stream-to-staging-tables ETL.** Land raw docs in staging tables,
merge into corpus in SQL. Rejected: doubles the table surface E2 must own,
adds a merge layer for tens of MB of data that fits in memory, and splits
the transform across Rust and SQL for no scale reason. YAGNI.

**B. Content-hash-only identity (no upstream ID).** Rejected: an upstream
rename changes content → hash changes → the row forks a duplicate instead
of updating in place, violating FR-15. Upstream `_id` is the identity;
hash is the change detector. They answer different questions.

**C. Derive engine math from Foundry `rules[]` arrays.** The frightened
fixture carries a machine-readable `FlatModifier` rule. Rejected hard:
Foundry rule elements are a *different engine's* vocabulary (selectors
like `"all"`, value expressions like `"-@item.badge.value"`), coverage
varies per condition, and translating them automatically is exactly the
"modifier math the rules don't support" failure Article IV forbids. Humans
author the tier seed; Foundry rules may *inform* that authoring.

## Coordination points (handed to E2 / other epics)

1. **Corpus table DDL** — column-level contract in `data-model.md`. E2
   owns; E4 adapts to E2's final names. E4's code must not create tables.
2. **`import_runs` table** — same; E4 is its only writer at POC.
3. **Lane CHECK + partial unique index** — requested in data-model.md;
   enforcement value described above.
4. **E8/E9 read contract** — E8 consumes `tier` + mapping rows
   (engine-math) and the display-only badge; E9 reads display data. They
   hardcode nothing about tiers (spec US-3). This design's row shapes are
   their interface; changes to them are a negotiation, not a unilateral
   edit.
5. **Spec Assumptions supersession** — the spec's Assumptions section says
   "corpus tables land with this epic's own migrations"; the orchestrator's
   standing rule (E2 owns ALL DDL) supersedes it. The spec text should be
   amended at next touch to remove the contradiction. Noted, not
   re-litigated.
