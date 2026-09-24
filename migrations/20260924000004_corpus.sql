-- E2 migration 4/5: rules corpus.
--
-- E2 owns ALL DDL including the corpus tables; E4 writes ROWS ONLY into
-- corpus_entries and carries zero migration files. Standing rule from the
-- specify round (design.md header).
--
-- One open-kinded table: conditions and items now; spells/feats/bestiary
-- land later as data, not schema change.
--
-- Field-level truth: specs/002-database-schema/data-model.md §5.

CREATE TABLE corpus_entries (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- `condition`, `item`, … — deliberately not an enum; open-ended.
    kind text NOT NULL CHECK (length(kind) > 0),
    name text NOT NULL,
    -- The lane split (PRD governing rule for content ownership): a fourth
    -- value is unstorable (US5.1).
    lane text NOT NULL CHECK (lane IN ('core', 'imported', 'custom')),
    -- The structured row; shape per kind is the writer's contract (E4 for
    -- imported, E9 for custom/core).
    data jsonb NOT NULL,
    -- Structured condition→modifier mappings in the FG3 shape
    -- ([{type, stat, value}]). NULL = display-only row (US5.4) — the tier
    -- test needs no prose parsing; the importer never fabricates mappings.
    modifiers jsonb,
    -- Upstream pack entry id — the idempotency key for E4 re-runs. Custom
    -- rows carry none, so re-runs structurally cannot touch them.
    source_id    text,
    pack_version text,
    imported_at  timestamptz,
    -- Creator for custom rows (US5.3); custom rows are first-class — nothing
    -- about their storage is second-class.
    created_by_sub text REFERENCES accounts (sub) ON DELETE RESTRICT,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

-- E4's re-run upsert key: "same pack, same row" reconciles idempotently.
CREATE UNIQUE INDEX corpus_entries_kind_source_id_key
    ON corpus_entries (kind, source_id) WHERE source_id IS NOT NULL;

CREATE INDEX corpus_entries_kind_lane_idx ON corpus_entries (kind, lane);
CREATE INDEX corpus_entries_name_idx ON corpus_entries (name);
