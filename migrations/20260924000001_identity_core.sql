-- E2 migration 1/5: identity core.
--
-- The global version sequence (machinery other migrations depend on — the
-- DEFAULT nextval() on version columns needs it to exist first, so it leads
-- instead of trailing), then accounts, parties, characters.
--
-- Field-level truth: specs/002-database-schema/data-model.md §2.
-- Rules: hard delete (FR-15); FKs + indexes on every party/account join path
-- (FR-5); created_at/updated_at on every entity table (FR-6).

-- One global sequence feeding every *_version column (design §4): per-field
-- monotonicity plus a total order across fields, so E7 catch-up is a single
-- `WHERE version > $last_seen` predicate.
CREATE SEQUENCE field_version_seq AS bigint;

-- One person known to the app. Natural-keyed on the IdP's stable `sub` claim
-- per the E3 cross-epic contract: every ownership binding below FKs to
-- `accounts.sub`; no surrogate id. Rows are upserted at login by E3 — E2
-- seeds none.
CREATE TABLE accounts (
    sub          text PRIMARY KEY,
    username     text NOT NULL,
    display_name text NOT NULL,
    -- Data for E3's authorization enforcement. The GM's read-only nature is
    -- server-enforced (E3), not schema-enforced — no write bans here.
    role         text NOT NULL CHECK (role IN ('player', 'gm')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- One campaign; the scoping root for nearly everything (FR-4). A second
-- party is an INSERT, never a migration.
CREATE TABLE parties (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name    text NOT NULL,
    -- The quartermaster designation is data: present now, activated by E12,
    -- no schema change then (spec edge case). Nullable cycle with characters
    -- — the FK is added once characters exists, below.
    quartermaster_character_id bigint,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- A roster member of exactly one party, owned by exactly one account.
-- payload_raw: the Pathbuilder export verbatim as received (text, not jsonb —
-- jsonb reorders keys; US3.1 says verbatim). base_sheet: E5's normalized
-- output, replaced wholesale on re-import; the schema holds it opaque.
CREATE TABLE characters (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    party_id    bigint NOT NULL REFERENCES parties (id) ON DELETE CASCADE,
    -- The ownership binding: one character per account, no accountless
    -- character — both directions enforced here (FR-7).
    owner_sub   text NOT NULL UNIQUE REFERENCES accounts (sub) ON DELETE RESTRICT,
    payload_raw text NOT NULL,
    base_sheet  jsonb NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    -- The same-party pin target: every character-touching edge elsewhere
    -- (effects source, effect targets, quartermaster) FKs through
    -- (party_id, id), so no child row can reach across the party roster
    -- boundary (FR-4).
    CONSTRAINT characters_party_id_id_key UNIQUE (party_id, id)
);

CREATE INDEX characters_party_id_idx ON characters (party_id);

-- Closing the nullable quartermaster cycle (party → character → party),
-- pinned to the party's own roster: the referencing pair (id,
-- quartermaster_character_id) must match characters (party_id, id), so
-- naming another party's character is unstorable (FR-4). When the pinned
-- character is hard-deleted, only the designation clears (PG15+ column-list
-- SET NULL) — the party row itself stays.
ALTER TABLE parties
    ADD CONSTRAINT parties_quartermaster_character_id_fkey
    FOREIGN KEY (id, quartermaster_character_id)
    REFERENCES characters (party_id, id)
    ON DELETE SET NULL (quartermaster_character_id);
