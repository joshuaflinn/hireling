-- E12 activation down (specs/014-shared-inventory design D1): restores the
-- exact pre-E12 state — the five changes of the up file, reversed.

-- D1.5 reversed: the non-unique party index back, the identity constraint
-- gone.
CREATE INDEX party_stash_party_id_idx ON party_stash (party_id);

ALTER TABLE party_stash
    DROP CONSTRAINT party_stash_party_id_item_name_key;

-- D1.4 reversed: the eight kinds migration 20260924000009 left.
ALTER TABLE audit_events DROP CONSTRAINT audit_events_event_check;

ALTER TABLE audit_events ADD CONSTRAINT audit_events_event_check CHECK (event IN (
    'login_success',
    'login_allowlist_denied',
    'logout',
    'forbidden_character_write',
    'forbidden_effect_write',
    'forbidden_custom_write',
    'forbidden_gm_write',
    'character_import'
));

-- D1.3 reversed.
ALTER TABLE claim_history
    DROP COLUMN coins_gp,
    DROP COLUMN coins_sp,
    DROP COLUMN coins_cp;

-- D1.2 reversed.
ALTER TABLE parties
    DROP COLUMN quartermaster_version;

-- D1.1 reversed.
ALTER TABLE party_stash
    DROP COLUMN version;
