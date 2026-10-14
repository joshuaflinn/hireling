# Deploy Runbook — Hireling on Mimir

**Epic**: E11 (spec `specs/012-go-live-deploy/spec.md`, design D1–D7) · GitHub issue #13
**Shape**: stateless container built from git on Mimir (existing Unraid Docker host), fronted by the existing cloudflared tunnel pattern (dwarfcampaign precedent), state only in the Asgard `hireling` hall. No registry — rebuild-from-git is the settled deployment shape (design D2).
**Handoffs (binding)**: H1 — the first real session is Josh's call, not this document's. H2 — §4 session-night readiness is executed by Josh. H3 — every step here runs on existing assets; if a step cannot, work **stops** and the gap is reported, nothing is bought.

Every step below names: the command, the expected observable, and the asset that owns it. Values that are secrets appear as *names and shapes only* — the house secret store is where values live; `src/config.rs` is the source of truth for the environment surface (FR-5).

---

## §1 Prerequisites

**Host prerequisite (Mimir):** none beyond Docker itself. Every Asgard-touching `psql`/`pg_restore` call in §1.1 and §5 steps 2/3/7 runs dockerized on the bridge, per §4 item 2's idiom: `docker run --rm --network asgard-net postgres:16 psql …`. There is no host-client path: Asgard publishes no ports, and Docker's DNS resolves bridge names only for containers attached to `asgard-net`, so `psql -h asgard` from a Mimir shell fails name resolution regardless of what client is installed. Calls that read a local file (`provision.sql` in §1.1, the dump in §5 step 3) bind-mount it into the same throwaway container — absolute paths only; Docker reads a relative `-v` source as a named volume.

### 1.1 The hall is provisioned — verify, don't assume

**Asset**: Asgard (house Postgres instance, postgres 16, reachable only over the `asgard-net` bridge, no published ports — `db/provision.sql`).

```sh
docker run --rm --network asgard-net postgres:16 \
  psql -h asgard -U postgres -tAc "SELECT datname FROM pg_database WHERE datname = 'hireling'"
```

Expected: one row, `hireling`. If empty, provision once per `db/provision.sql` (run by the operator as a superuser; the file is bind-mounted from the checkout into the throwaway container), then set the role password interactively — the password is never in the repo. `\password` prompts twice, never echoes, and keeps the plaintext out of argv, psql history, and the server log:

```sh
docker run --rm --network asgard-net -v <abs-path-to-checkout>/db/provision.sql:/tmp/provision.sql postgres:16 \
  psql -h asgard -U postgres -f /tmp/provision.sql
docker run --rm -it --network asgard-net postgres:16 psql -h asgard -U postgres
# at the psql prompt:
#   \password hireling
```

Expected: role `hireling` exists at `CONNECTION LIMIT 20`, database `hireling` owned by it. Schema is **not** applied here — the app migrates itself at boot (`src/db.rs::connect_and_migrate`, schema before traffic).

### 1.2 `asgard-net` is attachable from Mimir

**Asset**: Mimir (Docker host).

```sh
docker network inspect asgard-net --format '{{.Name}} {{.Driver}}'
```

Expected: `asgard-net bridge`. **If this fails, stop the line (H3)** — a cross-host network link would be new infrastructure, and the spec forbids buying one.

### 1.3 House secret store entries

**Asset**: the house secret store. Create one env file (used by `docker run --env-file` in §2); *names and shapes only* here, values from the store. Source of truth for the surface: `Settings::from_env` **and** `AuthSettings::from_env` together (`src/config.rs`) — this table is the only operator-facing record of the env surface there will be.

```sh
# hireling.env — store OUTSIDE any git checkout; mode 600.
# Full-line '#' comments only: `docker run --env-file` takes everything
# after the first '=' as the value, verbatim — no trailing annotations.
# Required, always — the app refuses to boot without them (debug *and* release).
# Comma-separated provider subs; per-entry seat names optional.
HIRELING_ALLOWLIST=<sub1>,<sub2>=Becky,...
# Must be a member of the allowlist; unset never is.
HIRELING_GM_SUB=<one of the allowlist subs>

# Required in release — a release binary missing any of these refuses to boot
# (AuthSettings::validate_release, fail-closed, src/lib.rs).
HIRELING_OIDC_ISSUER=https://auth.flinntech.com/application/o/hireling/
HIRELING_OIDC_CLIENT_ID=<Authentik confidential-client id>
HIRELING_OIDC_CLIENT_SECRET=<from the house secret store>
HIRELING_BASE_URL=https://hireling.flinntech.com
HIRELING_COOKIE_KEY=<64 hex chars = 32 bytes for the signed session cookie>

# Required in production — the in-code default is a localhost dev hall
# (src/config.rs), which fails at boot's migrate step on Mimir, not earlier.
HIRELING_DATABASE_URL=postgres://hireling:<password>@asgard:5432/hireling

# Optional, defaults shown (src/config.rs, src/auth/session.rs).
RUST_LOG=info
HIRELING_PORT=3000
# Resolves to /app/web/dist in the image — leave default.
HIRELING_STATIC_DIR=web/dist
HIRELING_SESSION_IDLE_SECS=86400
# Must be >= idle.
HIRELING_SESSION_ABSOLUTE_SECS=604800
# HIRELING_DEV_SESSIONS: do not set — the dev-session route is compiled out of
# release builds; it has no effect here.
```

### 1.4 Authentik application

**Asset**: Authentik (house IdP). Create an application + confidential client named `hireling`, and register the redirect URI **exactly**:

```
https://hireling.flinntech.com/api/auth/callback
```

That string is what `src/config.rs::redirect_uri()` derives from `HIRELING_BASE_URL` — a mismatch fails every login at the provider, not in the app. The issuer URL (§1.3's `HIRELING_OIDC_ISSUER`) is the application's issuer as Authentik presents it, e.g. `https://auth.flinntech.com/application/o/hireling/`. Take client id/secret from Authentik into the house secret store.

Observable: the application exists in Authentik with that exact redirect URI.

### 1.5 Tunnel hostname

**Asset**: cloudflared (existing tunnel, dwarfcampaign pattern — PRD Hosting & Ops). Add an ingress hostname `hireling.flinntech.com` whose service is the container over the bridge:

```
hostname: hireling.flinntech.com
service:  http://hireling:3000
```

`hireling` resolves on `asgard-net` (the container name from §2.2); the app publishes **no ports** — the tunnel is the only door. Observable: `curl -s -o /dev/null -w '%{http_code}\n' https://hireling.flinntech.com/healthz` returns `200` once §2 is done (and only then).

---

## §2 Build & first deploy

### 2.1 Build the image from git

**Asset**: Mimir.

```sh
git clone https://github.com/joshuaflinn/hireling && cd hireling
git checkout <commit-or-tag>            # the commit you intend to serve
DOCKER_BUILDKIT=1 docker build -t hireling:<tag> .
```

`<tag>` convention: the short commit sha (`git rev-parse --short HEAD`) — rollback (§3) picks the previous one. `DOCKER_BUILDKIT=1` guarantees the BuildKit cache mounts (cargo registry, cargo target, npm cache) that keep occasional rebuilds at minutes, not an hour (design D2). No registry exists; the tag lives only on Mimir.

Expected: one image, three stages — `web` (node:22-bookworm-slim) built `web/dist`, `build` (rust:1.99.0-bookworm, matching `rust-toolchain.toml` exactly) compiled `hireling` and `healthcheck`, runtime (`debian:bookworm-slim`) carries binary + bundle + CA certificates as the non-root `hireling` user. `docker image ls hireling` shows it.

### 2.2 Run the container

**Asset**: Mimir.

```sh
docker run -d \
  --name hireling \
  --restart unless-stopped \
  --network asgard-net \
  --stop-timeout 15 \
  --env-file /path/to/hireling.env \
  hireling:<tag>
```

`--stop-timeout 15` is `stop_grace_period: 15s` in compose terms (FR-3, design D3): the app drains WS (`bye` + Close 1001) and in-flight HTTP within a 10s cap (`src/http.rs::DRAIN_TIMEOUT`); 15s gives the final drain log line 5s of margin before Docker's SIGKILL. `--restart unless-stopped` + Unraid autostart covers host reboots. Single replica, always — the hall is capped at 20 connections and the app pools 10; two replicas against one hall is not a configuration this runbook supports.

**Boot observables** (each one, in order):

```sh
docker logs -f hireling
```

Expected JSON lines: `starting` (with `log_level`, `version` — `src/lib.rs`) then `serving` (`src/http.rs`) once migrations applied and :3000 bound. Migration or config failures name themselves here and the container restarts into the same failure — a boot that loops means read the log, it names the asset (hall unreachable → Asgard; missing env → §1.3).

```sh
docker inspect --format '{{.State.Health.Status}}' hireling
```

Expected: `starting` immediately after run, `healthy` within the start period. The image's HEALTHCHECK runs `/usr/local/bin/healthcheck` (`GET /healthz` on `127.0.0.1:$HIRELING_PORT`) at `--interval=30s --timeout=5s --retries=3 --start-period=60s` (design D1). **First deploy only:** record how long boot actually took (from `docker run` to first `healthy`) and tune `--start-period` in the Dockerfile toward the observed value in a follow-up PR — 60s ships as the migrate+bind estimate, not a law.

```sh
curl -s https://hireling.flinntech.com/healthz
```

Expected: `{"status":"ok","version":"<crate version>"}` — the public path through the tunnel. The local path is the healthcheck itself; `docker exec hireling /usr/local/bin/healthcheck && echo probe-ok` exercises it by hand.

### 2.3 Logs (FR-4)

**Asset**: Mimir — Unraid's per-container log view, or `docker logs hireling`. The app logs fielded JSON to stdout (tracing, `x-request-id` on every request, `RUST_LOG`-filterable). A healthy boot is the `starting` → `serving` pair from §2.2; healthy steady-state is request lines with status codes, no `error` levels. There are no log files in the image and none to rotate — stdout is the whole story (design D4).

### 2.4 One-time corpus import

**Asset**: the hall (via the running container's operator job).

```sh
docker exec hireling /usr/local/bin/hireling import --release pf2e-8.5.1
```

The parser requires exactly `--release <pf2e-N.N.N>` (`src/import/args.rs`); `pf2e-8.5.1` is the canonical tag pinned in `README.md`. The job downloads the pack release from GitHub (TLS via the image's CA certificates), parses, and writes the corpus to the hall named by `HIRELING_DATABASE_URL` (main.rs dispatches the `import` subcommand against the container's env). Re-runs are idempotent.

Observable: `docker logs hireling` shows the import's summary lines; §4's corpus check returns a non-zero `corpus_entries` count.

**License posture:** this is a private deployment — E4's license gate stays yellow-green and that is correct; the gate restricts *public* features only, and this epic changes nothing about it. (`license-verdict` exists for the day public exposure is on the table; running it now is informative, not required — and when it runs, it runs **from a checkout, not in the container**: it reads `NOTICE.md` and `licenses/` off disk, and neither reaches the image: `licenses/` is dockerignored, and `NOTICE.md` is never COPYed.)

### 2.5 Point Heimdall at the public probe

**Asset**: Heimdall (house uptime monitor — PRD Technical Metrics). Add a monitor for `https://hireling.flinntech.com/healthz` per the house pattern. Observable: the monitor reports up while the app serves. §4's triage table is where this signal earns its keep — it watches the public path, the container's own healthcheck watches the process, and the pair tells dead-from-dependency apart from dead.

---

## §3 Update & rollback

### 3.1 Update to a newer commit

**Asset**: Mimir.

```sh
cd /path/to/hireling
git fetch origin && git checkout <new-commit>
DOCKER_BUILDKIT=1 docker build -t hireling:<new-tag> .
curl -s https://hireling.flinntech.com/sw.js | grep BUILD_ID    # note OLD build id
docker stop hireling && docker rm hireling
```

`docker stop` observable (FR-3, SC-2's shape): the log's last lines name the drain — WS `bye` + close per session, then `in-flight requests drained; exiting` (`src/http.rs`), and the container's exit code is 0 within the 15s grace. Then re-run §2.2's `docker run` verbatim with `hireling:<new-tag>`.

Post-update observable: `curl -s https://hireling.flinntech.com/sw.js | grep BUILD_ID` shows a **new** content hash; connected clients pick up the new shell and the service worker discards the prior release's precache on activation (E10's built behavior — its browser-side proof is §6/gh#179 item 4).

### 3.2 Rollback to the previous tag

**Asset**: Mimir. The previous image tag is still on the host — rollback is the same three commands with it:

```sh
docker stop hireling && docker rm hireling
# §2.2's docker run verbatim, with hireling:<previous-tag>
curl -s https://hireling.flinntech.com/sw.js | grep BUILD_ID
```

Expected: old BUILD_ID back, `healthy` within the start period, healthz `200`. Data never moves in either direction — the hall is untouched by deploys; a rollback is a shell swap, not a schema swap (migrations are forward-only; a schema-touching rollback is an incident, not this section).

---

## §4 Session-night readiness (FR-8) — executed by Josh (H2)

Walk top to bottom before any session. Every item is green or names the failing asset and its owner; a red item on session night is a no-go for sync play, not a shrug.

| # | Check | Command (from Mimir unless noted) | Green looks like | Owning asset |
|---|-------|-----------------------------------|------------------|--------------|
| 1 | Mimir up | `docker info` | Server Version line prints | Mimir |
| 2 | Asgard up | `docker run --rm --network asgard-net postgres:16 pg_isready -h asgard` | `accepting connections` | Asgard |
| 3 | App container up | `docker ps --filter name=hireling` | `Up`, no restart loop | Mimir |
| 4 | Local health green | `docker inspect --format '{{.State.Health.Status}}' hireling` | `healthy` | the app |
| 5 | Tunnel + public probe green | `curl -s https://hireling.flinntech.com/healthz` | `{"status":"ok",...}` | cloudflared |
| 6 | Heimdall probe green | Heimdall dashboard — monitor target `https://hireling.flinntech.com/healthz` (§2.5) | hireling monitor `up` | Heimdall |
| 7 | Corpus present | `docker exec <house-postgres-container> psql -U hireling -d hireling -tAc 'SELECT count(*) FROM corpus_entries'` | a non-zero count | the hall |
| 8 | Deployed version known | `curl -s https://hireling.flinntech.com/healthz` | read the `version` field | the app |

**Triage — two probes, two different failures** (design §5): the container healthcheck sees the process; Heimdall sees the public path. `/healthz` is dependency-free by design (`src/health.rs`) — it answers even with Postgres down.

- Red 4 + green 6, or green 4 + red 6 → the differing side owns the fault (container vs tunnel/edge). Fix before session.
- Green 4 **and** 6 but players report errors → dependency problem, not liveness: check 2 and 7, and `docker logs hireling` for Postgres errors — the log names Asgard.
- App won't boot at all (3 shows restarts) → read `docker logs hireling`; it fails closed naming the dependency (hall → Asgard/§1.1; env → §1.3; port → §2.2).
- Probe misbehaving while the app is fine → documented fallback (design §7): re-run §2.2 with `--no-healthcheck` added and lean on Heimdall (item 6) until the probe is fixed in a PR. Not a silent edit — it is a declared operating mode of this runbook.

---

## §5 Backup-restore rehearsal (FR-7) — once, before the first session

An unverified backup is a hope, not a backup. This proves Asgard's rotation can actually restore the hall. **Execution is handoff-side** (requires Asgard access); this procedure and its verification queries are ours. Record the run — dump restored, scratch hall used, checks passed, teardown — as a comment on gh#13.

1. **Pick the dump.** Latest rotation dump of the `hireling` hall from Asgard's existing backup schedule. **Asset**: Asgard (operator knows the rotation's location/format; `pg_dump` custom or plain).
2. **Create a scratch hall.** `docker run --rm --network asgard-net postgres:16 psql -h asgard -U postgres -c "CREATE DATABASE hireling_restore_scratch OWNER hireling"` — owner must be the app role so the container can read it.
3. **Restore.** Plain SQL dump (`<dump>` is a Mimir-side path — `gunzip` runs on the host): `gunzip -c <dump> | docker run --rm -i --network asgard-net postgres:16 psql -h asgard -U hireling -d hireling_restore_scratch`. Custom format (`<abs-path-to-dump>` is bind-mounted into the container): `docker run --rm --network asgard-net -v <abs-path-to-dump>:/dumps/backup postgres:16 pg_restore -h asgard -U hireling -d hireling_restore_scratch /dumps/backup`. Expected: restore completes without error.
4. **Row counts on core tables** (identity, live state, corpus, sessions):
   ```sh
   docker exec <house-postgres-container> psql -U hireling -d hireling_restore_scratch -tAc \
     "SELECT 'accounts', count(*) FROM accounts UNION ALL
      SELECT 'characters', count(*) FROM characters UNION ALL
      SELECT 'corpus_entries', count(*) FROM corpus_entries UNION ALL
      SELECT 'sessions', count(*) FROM sessions"
   ```
   Expected: counts match the production hall's same query (run it on `hireling` and compare).
5. **Boot the app against the scratch hall.** Outside session night — this borrows the public origin for a few minutes. `docker stop hireling`, then §2.2's `docker run` with two env files — `--env-file hireling.env --env-file scratch.env` — where `scratch.env` (mode 600, outside any git checkout, same rules as §1.3) holds only the overriding `HIRELING_DATABASE_URL=postgres://hireling:<password>@asgard:5432/hireling_restore_scratch`. Docker applies `--env-file` left to right and the last value wins, so scratch.env's URL replaces hireling.env's — and the credential stays out of `docker run` argv, which `docker inspect`, `ps`, and shell history all read. Everything else identical, so OIDC legs work — the tunnel still fronts this container.
6. **Verify through the real paths.** One login end-to-end (Authentik → party screen, §6 item 1's walk) and one roster read (open a character sheet and see its values). Expected: the restored hall serves them.
7. **Teardown.** `docker stop` the scratch container; re-run §2.2 verbatim (no override) and confirm §4 items 4–5 green again; then `docker run --rm --network asgard-net postgres:16 psql -h asgard -U postgres -c "DROP DATABASE hireling_restore_scratch"`.
8. **Record on gh#13**: dump file + timestamp, scratch hall name, the four counts, login/roster result, teardown confirmation.

A failed rehearsal is a stop-the-line finding against Asgard's rotation — reported through Thrane to Josh — not something to paper over (spec Edge Cases).

---

## §6 Go-live checklist (FR-9)

Everything below checked ⇒ the deployment is **ready**. Ready ≠ live: the first real session is **Josh's call** (handoff H1).

- [ ] **End-to-end login on the deployed origin** (the PRD Entry Point): browse `https://hireling.flinntech.com`, complete Authentik authentication, land on the party screen.
- [ ] **Two-client live sync** (E10 SC-1's manual half): from two different browsers/devices, complete login on both; commit a write (e.g. tick a checkbox) on one and observe it on the other without reload.
- [ ] **gh#179's four browser items**, run on the deployed origin and recorded on gh#179 (that issue stays the record; this list references, it does not re-specify): install affordance, standalone window, offline read-only reload, second-build cache discard (§3.1's BUILD_ID move is the deploy-side setup for the fourth).
- [ ] **Backup-restore rehearsal record present** on gh#13 (§5 executed once).
- [ ] **Ready ≠ live acknowledged**: schedule the first session.
