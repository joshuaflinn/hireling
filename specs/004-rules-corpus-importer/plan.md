# Plan: Rules Corpus Importer (E4) — implementation

**Status**: Built against the approved spec + brainstorm design · **Implements**: `spec.md`, `design.md` (with the E2 reconciliation below), `contracts/foundry-packs.md` (as corrected by its §7 probe)

This is the plan-stage artifact for the epic's feature pipeline
(specify → clarify → brainstorm → plan → dev). Specify and brainstorm ran
under the pre-approved-defaults ruling; this document records HOW the build
realized those decisions and where implementation probed reality and
adjusted. No scope left the P0 line.

## Module layout (thin shell / pure core, per toolkit conventions)

| Module | Role | Tested by |
|---|---|---|
| `src/import/args.rs` | argv parsing, pin validation (`^pf2e-N.N.N$`), hand-rolled (no clap, per design) | unit |
| `src/import/model.rs` | document parse + declared-shape validation, canonical content hash, publication/slug/isValued extraction | unit |
| `src/import/seed.rs` | tier-seed load + closed-vocabulary validation (fails at load, not mid-run) | unit |
| `src/import/transform.rs` | **pure heart**: docs + existing rows + seed → write plan (insert/update/skip/stale/unmapped) | unit, exhaustive |
| `src/import/pack.rs` | zip extraction (size-capped), pack enumeration, duplicate-id check | unit (in-memory zips) |
| `src/import/fetch.rs` | release metadata, sha256-verified download, bounded retry + jitter; digest mismatch = no retry | exercised by the live run |
| `src/import/store.rs` | sqlx load layer; every statement sets/filters `lane = 'imported'` | integration |
| `src/import/license.rs` | notice parse, archive sha256 verify, DB license-coverage check | unit + integration |
| `src/import/report.rs` | run report (JSON + human render) — FR-12/SC-8 without an `import_runs` table | exercised |
| `src/import/mod.rs` | orchestration: connect → fetch → extract+validate ALL → plan ALL → one tx per category → report | integration |

Subcommands: `hireling import --release <tag>`, `hireling license-archive
--release <tag>`, `hireling license-verdict`. No arguments still serves the
app. `just import|license-archive|license-verdict` wrap them.

## Reconciliation with E2 (the settled coordination point)

`design.md` and `data-model.md` proposed `rules_corpus`,
`condition_modifier_mappings`, and `import_runs` **as a request to E2**.
E2's answer (its schema branch, published while E4 built) is one
`corpus_entries` table: `data` jsonb (shape owned by the writer — E4 for
imported rows), `modifiers` jsonb (NULL = display-only), `source_id` upsert
key with a partial unique index, `pack_version`/`imported_at`, and **no**
`import_runs`. Per the design's own rule ("E2's migrations are authoritative
and E4 adapts"), the importer targets E2's shape:

- Per-row provenance (content_hash, importer_version, publication, slug,
  tier, is_valued) lives in `data.import`; the verbatim upstream document
  lives in `data.upstream`. FR-7/SC-3 hold from the row's own fields.
- Modifier mappings live in `modifiers` as FG3-shape rows with one honest
  extension: `value_kind` (`"condition_value"` on the shared parameterized
  mapping for valued conditions, FR-9). NULL = display-only stays the
  canonical tier test, and `data.import.tier` also states it explicitly
  (stored data, FR-8).
- The run log is the structured report (stdout JSON + human render + tracing
  event). FR-12 says "log", SC-8 says "without inspecting the database" —
  both satisfied; a DB-persisted `import_runs` table was E4's proposal that
  E2 declined, and E4 does not unilaterally add DDL.
- E4 ships **zero migrations**. Integration tests apply a fixture schema
  (`tests/schema.sql`) copied from E2's published `20260924000004_corpus.sql`
  (plus an `accounts` stub for its FK), labeled as standing in for the real
  migration; swap to `sqlx::migrate!` when E2 lands.

## Probe corrections (implementation reality)

See `contracts/foundry-packs.md` §7: the zip holds one JSON **array per
pack** (not a per-document tree); equipment `type` spans nine Foundry item
types; `level`/`price` are not universal (kits/legacy docs) so they are not
required paths; equipment mixes ORC and OGL licenses (NOTICE covers both).
All probed at `pf2e-8.5.1` with the asset digest verified.

## Tier seed at ship

`data/seed/condition-tiers.json` ships with the two conditions the spec's
scenarios name: **frightened** (engine-math, valued, one parameterized
`status → all_checks_and_dcs` mapping — verified against the captured
document's own rules text) and **concealed** (explicit display-only with the
reason). Everything else defaults display-only (fail-safe, FR-11) and is
named in every run report's unmapped list until the owners extend the seed
by PR. Deliberately NOT model-authored: 40 more condition mappings from
memory would be exactly the fabrication Article IV forbids; the run report
makes the coverage gap visible instead.

## Verification

- 65 unit tests (pure layers, captured fixtures + labeled mutations).
- 9 integration tests against a real Postgres (throwaway database per test,
  compose-throwaway pattern): clean import + provenance, same-release no-op
  byte-compare, A→B rename/stale/update-in-place, custom-lane survival
  across success AND failure, zero-row tripwire, schema-drift-fails-before-
  any-write, unmapped→display-only + report, older-release freshness note,
  license verdict green/red flips.
- A live `just import RELEASE=pf2e-8.5.1` run against a real database, and
  a live `license-archive` + `license-verdict`, recorded in the PR.
