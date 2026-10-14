-- E12 migration: shared-inventory activation (specs/014-shared-inventory
-- design D1). E2 shipped the party_stash / claim_history / party_bank tables
-- unused (migration 20260924000005) and parties.quartermaster_character_id
-- present but dormant (20260924000001); E7 FR-13 named this epic as the one
-- to add the version columns. Five changes, all reversible — the down file
-- restores the pre-E12 state, with one caveat on the audit CHECK documented
-- there (as in migration 20260924000009's down).

-- D1.1: the stash row's version unit (spec FR-2.2) — the shipped table had
-- none. One row is one version unit (the inv/effect precedent); the global
-- sequence keeps E7's single catch-up predicate working across fields.
ALTER TABLE party_stash
    ADD COLUMN version bigint NOT NULL DEFAULT nextval('field_version_seq');

-- D1.2: makes the quartermaster designation a live-synced party field (spec
-- FR-2.3/FR-5). The column holding the value (quartermaster_character_id,
-- FK-cycled, SET NULL on delete) is untouched.
ALTER TABLE parties
    ADD COLUMN quartermaster_version bigint NOT NULL DEFAULT nextval('field_version_seq');

-- D1.3: money movements ride the same claim log (the party_bank comment is
-- the ruling — a second ledger would be complexity, not audit). NULL coins =
-- item movement; any value = money movement carrying the SIGNED amounts, so
-- a withdrawal stores negatives. For coin rows `quantity` carries a
-- meaningless 1 (its CHECK (> 0) still stands); the item_name of a money row
-- carries a deterministic label — the sold item's name for a sale, the fixed
-- text `bank adjustment` for a manual adjustment. No note column exists or
-- is added (spec Clarify Log #5: actor logging, not notes). The three
-- columns are one fact — the money movement's signed total — so they stand
-- or fall together: claim_history_coins_all_or_none makes partial population
-- (some denominations set, others NULL) impossible, because such a row sits
-- ambiguously between the item-movement predicate (all three NULL) and a
-- money movement. The structural backing lives here, in the one-way file,
-- rather than in the write path, so no future writer can mint the ambiguous
-- shape after the cheap moment has passed.
ALTER TABLE claim_history
    ADD COLUMN coins_gp int,
    ADD COLUMN coins_sp int,
    ADD COLUMN coins_cp int,
    ADD CONSTRAINT claim_history_coins_all_or_none
        CHECK (num_nonnulls(coins_gp, coins_sp, coins_cp) IN (0, 3));

-- D1.4: audit values for the three new denial surfaces. Postgres cannot
-- alter a CHECK in place: drop and re-add under the same auto-named
-- constraint (the migration 20260924000009 precedent). Down restores the
-- eight kinds that migration left.
ALTER TABLE audit_events DROP CONSTRAINT audit_events_event_check;

ALTER TABLE audit_events ADD CONSTRAINT audit_events_event_check CHECK (event IN (
    'login_success',
    'login_allowlist_denied',
    'logout',
    'forbidden_character_write',
    'forbidden_effect_write',
    'forbidden_custom_write',
    'forbidden_gm_write',
    'character_import',
    'forbidden_stash_write',
    'forbidden_bank_write',
    'forbidden_party_write'
));

-- D1.5: FR-1's declared identity — the item name within the party — gets
-- structural backing. Until now the only key was the surrogate id plus a
-- non-unique party index, so a check-then-insert race on stash_new could
-- mint two rows for one name, after which every (party_id, item_name) op is
-- ambiguous against a wire contract that addresses one row with one version.
-- Mirrors character_inventory_live's composite PK
-- (migrations/20260924000002_live_state.sql); makes the race impossible
-- rather than unlikely. The backing index serves the old index's prefix
-- (party_id) queries, so party_stash_party_id_idx is dropped here.
ALTER TABLE party_stash
    ADD CONSTRAINT party_stash_party_id_item_name_key UNIQUE (party_id, item_name);

DROP INDEX party_stash_party_id_idx;
