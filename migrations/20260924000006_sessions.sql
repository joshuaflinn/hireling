-- Server-held session bindings; the sole source of acting identity.
-- Expiry is evaluated from the two deadlines (idle + absolute); deletion is
-- invalidation. There is no status column by design.
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    account_sub TEXT NOT NULL REFERENCES accounts (sub) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    absolute_expires_at timestamptz NOT NULL,
    CHECK (expires_at > created_at)
);

CREATE INDEX sessions_account_sub_idx ON sessions (account_sub);
