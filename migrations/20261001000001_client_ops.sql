-- E7 migration: the client_ops idempotency ledger.
--
-- One row per processed client write operation (specs/007-party-sync
-- data-model.md), written in the same transaction as the field write it
-- records. The replay-dedupe key for ack-lost writes (FR-8): a client that
-- never saw its ack resends the same op_id; the server answers from this
-- table without re-writing.
--
-- `already_applied` is deliberately absent from the CHECK: it is a read
-- answer, never a stored outcome. Rows are append-only — no updated_at.
-- `request` holds the exact client request ({target, base_version, value}):
-- it is what tells a genuine replay apart from reuse of the id for a
-- different request — the latter is refused, never committed a second
-- time (the id is reserved through this table's PK before the field CAS
-- in the same transaction).

CREATE TABLE client_ops (
    op_id             text PRIMARY KEY,
    account_sub       text NOT NULL REFERENCES accounts (sub) ON DELETE RESTRICT,
    field_path        text NOT NULL,
    request           jsonb NOT NULL,
    outcome           text NOT NULL CHECK (outcome IN ('applied', 'superseded', 'rejected', 'forbidden')),
    resulting_version bigint,
    created_at        timestamptz NOT NULL DEFAULT now()
);

-- The pruning/inspection query path for E11's go-live checklist.
CREATE INDEX client_ops_account_sub_created_at_idx ON client_ops (account_sub, created_at);
