-- E2 migration 5/5: P1 shared inventory — present, unused at P0 (FR-12).
--
-- Fully consistent with this epic's party-scoping, timestamps, hard-delete,
-- and versioning rules. No P0 code path writes them; E12 activates them with
-- zero migrations for these core structures. Its spec owns adding stash
-- fields to the versioning scheme.
--
-- Field-level truth: specs/002-database-schema/data-model.md §6.

CREATE TABLE party_stash (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    party_id  bigint NOT NULL REFERENCES parties (id) ON DELETE CASCADE,
    item_name text NOT NULL,
    -- A stash row with zero quantity is deleted, not kept (E12's write rule;
    -- the schema backs it).
    quantity  int NOT NULL CHECK (quantity > 0),
    -- Per-item Bulk; PF2e "L" = 0.1 by convention, NULL = negligible/unknown.
    bulk      numeric(6, 1),
    notes     text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX party_stash_party_id_idx ON party_stash (party_id);

-- The append-only transfer/sale log. Only ever inserted into (FR-15, US6.2):
-- the trigger below makes UPDATE and DELETE impossible for any role,
-- including the table-owning app role.
CREATE TABLE claim_history (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    party_id    bigint NOT NULL REFERENCES parties (id) ON DELETE CASCADE,
    item_name   text NOT NULL,
    quantity    int NOT NULL CHECK (quantity > 0),
    -- Where it moved from / to — labels ("character:Lorum Ipsum", "stash").
    -- Format is E12's spec; the schema is deliberately dumb.
    origin      text NOT NULL,
    destination text NOT NULL,
    -- Who did it — settles "who took what" (US6.3).
    actor_sub   text NOT NULL REFERENCES accounts (sub) ON DELETE RESTRICT,
    -- The only timestamp; no updated_at — nothing ever updates.
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX claim_history_party_id_idx ON claim_history (party_id);
CREATE INDEX claim_history_actor_sub_idx ON claim_history (actor_sub);

-- One balance row per party. The movements are already in claim_history; a
-- second ledger would be complexity, not audit.
CREATE TABLE party_bank (
    party_id   bigint PRIMARY KEY REFERENCES parties (id) ON DELETE CASCADE,
    gp         int NOT NULL DEFAULT 0 CHECK (gp >= 0),
    sp         int NOT NULL DEFAULT 0 CHECK (sp >= 0),
    cp         int NOT NULL DEFAULT 0 CHECK (cp >= 0),
    version    bigint NOT NULL DEFAULT nextval('field_version_seq'),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Append-only enforcement (US6.2, FR-15): a trigger, not a REVOKE — the app
-- connects as the table-owning role (app-boot migrations), and REVOKE is
-- toothless against the owner. Role-independent, self-documenting, drops
-- cleanly below.
CREATE FUNCTION forbid_claim_history_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'claim_history is append-only: % is not permitted', TG_OP;
END;
$$;

CREATE TRIGGER claim_history_append_only
    BEFORE UPDATE OR DELETE ON claim_history
    FOR EACH ROW EXECUTE FUNCTION forbid_claim_history_mutation();
