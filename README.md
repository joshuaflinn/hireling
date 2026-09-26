# Hireling

Party-linked Pathfinder 2e character tracker. Pathbuilder imports, live buff
propagation across the party, shared inventory, PWA.

Status: scaffolding. Spec: `docs/PRD.md`. Law: `CONSTITUTION.md`.
Roadmap: `docs/ROADMAP.md` (includes the ideas inbox — drop anything, no
format police). Agent rules: `AGENTS.md` + `docs/toolkit-conventions.md`.

## Development

```sh
just dev       # build the frontend, run the backend (serves API + UI on :3000)
just db        # start the throwaway dev Postgres on :5432
just test      # tests (starts the throwaway Postgres first when docker exists)
just gate      # the full local gate (same checks as CI)
```

Every recipe is also a plain cargo/npm command — `just` is convenience, not
required:

```sh
npm --prefix web run build                                # build the frontend
cargo run                                                 # run the server
cargo test --quiet                                        # tests
cargo fmt --all                                           # format
cargo clippy --all-targets --all-features -- -D warnings  # lint
cargo deny check                                          # audit dependencies
docker compose up -d db                                   # dev Postgres
```

Configuration is env-var driven, with local-dev defaults — `HIRELING_PORT`
(3000), `HIRELING_DATABASE_URL` (the compose Postgres), `HIRELING_STATIC_DIR`
(`web/dist`), `RUST_LOG` (`info`). `GET /healthz` answers 200 with the build
version and never touches the database.

## Rules corpus import (operator)

The rules corpus is seeded by import from the Foundry VTT pf2e system packs,
pinned by release tag — never `latest`, never `sf2e-*`:

```sh
just import RELEASE=pf2e-8.5.1       # import conditions + items (idempotent re-runs)
just license-archive RELEASE=pf2e-8.5.1   # archive upstream license texts (commit the result)
just license-verdict                 # the license gate: exit 0 green / 1 red
```

Re-running the same release changes zero rows; changed rows update in place
(provenance advances); rows removed upstream are kept and reported stale —
never deleted. Party homebrew (`custom` lane) is structurally untouchable.
Every imported condition is tiered by the human-reviewed seed in
`data/seed/condition-tiers.json` (engine math vs tracked-manually); unmapped
conditions default display-only and are listed in the run report. The license
gate (`NOTICE.md`, `licenses/foundry-pf2e/`) gates public exposure only — the
private POC deploys regardless.

Git hooks enforce the gate on every commit — install them once per clone with
`./.githooks/install.sh`.

CI runs [grizzly-gate](https://github.com/Grizzly-Endeavors/grizzly-gate)
(standalone mode) on every PR. The gate is the first reviewer; a human is the
second.

## Database

The app migrates itself: on boot it applies the embedded `migrations/`
(`sqlx::migrate!`), so a fresh database comes up to current schema with just
`just db && just dev`. A database it cannot reach or migrate fails the boot
loudly, before traffic. Local development only ever touches the compose
throwaway (`docker compose down` destroys it) — never Asgard, the production
hall.

To drive the schema directly instead, install
[`sqlx-cli`](https://crates.io/crates/sqlx-cli):

```sh
cargo install sqlx-cli --locked --features postgres
just db-migrate   # sqlx migrate run — apply pending migrations
just db-revert    # sqlx migrate revert — undo the most recent migration
just db-reset     # drop + recreate the throwaway, then migrate from empty
```

Every migration is a reversible up/down pair, exercised by the test suite
(`tests/db.rs` cycles up and down three times and verifies the object set
each way).

The schema tests need a Postgres on :5432 (the compose throwaway). When none
is reachable they skip loudly on stderr — never silently green.

### Production (Asgard)

The production database is a dedicated hall on the house Postgres: database
`hireling`, role `hireling` capped at `CONNECTION LIMIT 20`, reachable only
over the `asgard-net` bridge (no published ports; rides Asgard's backup
rotation). Provisioning is a checked-in artifact — run once by the operator,
no click-ops:

```sh
psql -h asgard -f db/provision.sql   # then set the role password interactively
ALTER ROLE hireling PASSWORD '<from the house secret store>';  # never in git
```

Schema migrations are not run by hand in production: the deployed binary
applies its embedded migrations at boot, as it does locally.

## Layout

- `src/` — the Rust backend (axum, single binary)
- `migrations/` — reversible sqlx up/down pairs, applied at boot
- `db/` — production provisioning artifacts
- `web/` — the Svelte PWA (lands with the frontend skeleton)
- `docs/` — PRD, checker reports, ADRs (`docs/decisions/`), vendored tooling
  references (`toolkit.md`, `toolkit-conventions.md`)
- `docs/reference/lorum_ipsum_dashboard.html` — Dave's prototype, the sheet UX
  baseline and the import contract's reference export
