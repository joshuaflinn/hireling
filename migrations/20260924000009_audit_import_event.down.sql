-- Down for 20260924000009_audit_import_event: restore E3's seven-kind CHECK.
-- Existing character_import rows would block the constraint re-creation —
-- this down is only runnable while no import has been audited, which is the
-- honest reverse of a migration whose up ran before any import existed.

ALTER TABLE audit_events DROP CONSTRAINT audit_events_event_check;

ALTER TABLE audit_events ADD CONSTRAINT audit_events_event_check CHECK (event IN (
    'login_success',
    'login_allowlist_denied',
    'logout',
    'forbidden_character_write',
    'forbidden_effect_write',
    'forbidden_custom_write',
    'forbidden_gm_write'
));
