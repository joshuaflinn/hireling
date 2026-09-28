-- E5 raised finding 2/2 (design §5.2, gate-approved): admit
-- 'character_import' to the audit_events.event enum.
--
-- FR-16 requires every import attempt audit-logged; E3's closed CHECK
-- admits none. Postgres cannot alter a CHECK in place: drop the constraint
-- (auto-named <table>_<column>_check by E2's inline definition) and re-add
-- it with the eighth kind appended. Down restores E3's seven exactly.

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
