-- Raise a Shield's live toggle (gh#82). PR #73's extractor
-- (`engine_host::extract::ac_of`) reads the flag from the sheet and gates
-- the shield bonus on it; until now nothing could write it, so every
-- shield rendered permanently lowered. The toggle is live character state
-- like `level_adjust`: per-field CAS version, party-sync diff, and a
-- recompute fan-out on apply.
--
-- NULLABLE on purpose: NULL means "never toggled live" and the sheet's own
-- `ac.shieldRaised` (the import's state) governs — PR #73's reader stays
-- the truth for untouched characters. `false` is a real live state ("the
-- player lowered it"), not the default; a NOT NULL DEFAULT false column
-- would silently override an import that says the shield is raised.
ALTER TABLE character_vitals
    ADD COLUMN shield_raised         boolean,
    ADD COLUMN shield_raised_version bigint NOT NULL DEFAULT nextval('field_version_seq');
