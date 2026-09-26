-- Test-fixture schema for the E4 importer integration suite.
--
-- This stands in for E2's migration `20260924000004_corpus.sql` (plus the
-- `accounts` table that migration 1 creates and migration 4 FKs into). It is
-- copied from E2's published DDL so the importer is exercised against the
-- real table shape; when E2's migrations land on main, this file is swapped
-- for `sqlx::migrate!` against the real thing. E4 owns no product DDL.

CREATE TABLE accounts (
    sub          text PRIMARY KEY,
    username     text NOT NULL,
    display_name text NOT NULL,
    role         text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE corpus_entries (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL CHECK (length(kind) > 0),
    name text NOT NULL,
    lane text NOT NULL CHECK (lane IN ('core', 'imported', 'custom')),
    data jsonb NOT NULL,
    modifiers jsonb,
    source_id    text,
    pack_version text,
    imported_at  timestamptz,
    created_by_sub text REFERENCES accounts (sub) ON DELETE RESTRICT,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX corpus_entries_kind_source_id_key
    ON corpus_entries (kind, source_id) WHERE source_id IS NOT NULL;

CREATE INDEX corpus_entries_kind_lane_idx ON corpus_entries (kind, lane);
CREATE INDEX corpus_entries_name_idx ON corpus_entries (name);
