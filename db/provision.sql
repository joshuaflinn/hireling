-- Production provisioning for the Hireling database hall (E2, FR-2).
--
-- Run ONCE by the operator, as a superuser, against the house Postgres
-- instance (Asgard):
--
--     psql -h asgard -f db/provision.sql
--
-- What DDL cannot express (recorded here by design, per design.md §1):
--
--   * The hall is reachable ONLY over the `asgard-net` bridge — no published
--     ports, no public exposure. Network reachability is enforced by the
--     bridge, not by this file.
--   * The hall rides Asgard's existing backup rotation; no separate backup
--     job is provisioned here.
--   * The role's CONNECTION LIMIT 20 is the deployment invariant; the app's
--     pool keeps deliberate headroom under it (10, see src/db.rs).
--
-- Schema migrations are NOT run here: the app applies its embedded
-- migrations at boot (sqlx::migrate!), so `cargo run` (or the deployed
-- binary) brings the schema up on first start.

-- Idempotent role creation (CREATE ROLE has no IF NOT EXISTS). No password
-- here on purpose: the operator sets the real credential at provisioning
-- time (below) — never in this file, never in the repo.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'hireling') THEN
        CREATE ROLE hireling LOGIN CONNECTION LIMIT 20;
    END IF;
END
$$;

-- Operator step after provisioning (interactive, not scripted):
--     ALTER ROLE hireling PASSWORD '<secret from the house secret store>';

-- Idempotent database creation, owned by the role (the app applies its own
-- migrations at boot, so the role must own its hall).
SELECT 'CREATE DATABASE hireling OWNER hireling'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'hireling')\gexec

-- The app receives the connection string (with the credential) via the
-- HIRELING_DATABASE_URL environment variable — never committed.
