-- Append-only audit trail: logins and ownership-relevant rejections.
-- Insert-only by rule: no application code updates or deletes these rows,
-- which is the deliberate exception to the updated_at convention.
-- actor_sub carries no FK on purpose: an allowlist-denied login presents a
-- sub with no account row, and audit rows must never be blocked by account
-- lifecycle.
CREATE TABLE audit_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at timestamptz NOT NULL,
    actor_sub TEXT,
    event TEXT NOT NULL CHECK (event IN (
        'login_success',
        'login_allowlist_denied',
        'logout',
        'forbidden_character_write',
        'forbidden_effect_write',
        'forbidden_custom_write',
        'forbidden_gm_write'
    )),
    target TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('allowed', 'denied')),
    request_id TEXT
);

CREATE INDEX audit_events_occurred_at_idx ON audit_events (occurred_at);
CREATE INDEX audit_events_actor_sub_idx ON audit_events (actor_sub);
