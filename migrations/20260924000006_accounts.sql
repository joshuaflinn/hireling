-- One row per allowlisted human, keyed by the provider's `sub` — the settled
-- cross-epic keying contract (specs/003-authentik-oidc/data-model.md).
-- E2 owns this table going forward; E3 creates it because E3 merges first
-- and every auth table keys on it.
CREATE TABLE accounts (
    sub TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('player', 'gm')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
