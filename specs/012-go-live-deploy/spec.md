# Feature Specification: Go-Live Deploy (E11)

**Epic**: E11 — Phase 2, P0 · depends on E6–E10 (all closed) · GitHub issue #13
**Created**: 2026-10-14
**Status**: Specify gate run 2026-10-14 (Korrin, front half of the SDD pipeline — spec, design, plan; no code, no infrastructure touched in this pass); design signed by Korrin 2026-10-08-style same-day (epic-internal P0 scope, grounds recorded in `design.md` §0)
**Input**: `docs/EPICS.md` Epic E11 (specify prompt + Constraints + AI Guardrails); PRD v3.6 — Hosting & Ops, Technical Metrics, Entry Point, Zero-new-infrastructure (§Non-Goals side constraints); `CONSTITUTION.md` Articles I/II/V; gh#13 carry-over comment (SC-5 browser half + E10 SC-1 live two-client observation); gh#179 (the deferred PWA browser checklist); repo facts verified on `main` at `c4ae46d` (cited per requirement)

---

## What this epic is, in one paragraph

Ship Hireling to the lab as a running service at `https://hireling.flinntech.com`: a stateless container built from git on Mimir (existing Unraid Docker host), fronted by the existing cloudflared tunnel pattern, state in the Asgard `hireling` hall (already provisioned, E2) — plus the procedures that make it operable: deploy/update/rollback runbook, session-night readiness checklist, a once-before-first-session backup-restore rehearsal, and the go-live checklist that retires gh#179. The application code is done (E1–E10 closed); this epic adds the image recipe and the operating documents, and draws an explicit line between what seats build and verify without touching Josh's hosts and what is a named handoff.

### Named handoffs — the line this spec draws (binding)

Three things in this epic reach Josh. They are **handoffs, not acceptance criteria we tick**:

- **H1 — Go-live itself.** The first real game session on `hireling.flinntech.com` is Josh's call. E11 delivers everything up to that moment; it does not schedule or declare the session.
- **H2 — Session-night readiness execution.** Mimir up, Asgard up, tunnel up, probe green — a P0 operational dependency **owned by Josh** (PRD FG2, Technical Metrics). We ship the checklist; he runs it.
- **H3 — Spend.** Zero new infrastructure is a settled constraint. If any requirement in this spec cannot be met on existing Mimir, Cloudflare, Authentik, and Asgard assets, work **stops** and the gap is reported (escalation path: task to Thrane) — nothing is designed around, nothing is bought.

Host-side execution steps (Authentik app registration, tunnel hostname route, Heimdall probe target, Mimir placement, corpus import against the production hall, restore-rehearsal execution, the gh#179 browser pass) are runbook steps executed on existing assets per their existing patterns. They are **ours to document exactly and Josh's to execute or delegate** — each names the asset, the action, and the verification.

## User Scenarios & Testing *(mandatory)*

E11's users: the six party accounts (arriving at a URL for the first time), Josh (operator and owner), Heimdall (the existing uptime monitor), and every future session night.

### User Story 1 — Deploy from git by the book (Priority: P0) 🎯 MVP

Josh (or a seat acting at his direction) takes the runbook, and with it alone: builds the image from a git checkout on Mimir, attaches the container to `asgard-net`, points it at the hall with secrets from the house store, and the app serves `https://hireling.flinntech.com`. No click-ops archaeology, no undocumented step, no state inside the container.

**Why this priority**: The epic's core sentence — stateless container on Mimir, rebuild-from-git, existing tunnel pattern. Everything else in E11 hangs off this working.

**Independent Test**: On any Docker host with the repo checked out, `docker build` succeeds; the resulting container, given the runbook's env set, boots against the Asgard hall (schema applies at boot, `db.rs::connect_and_migrate`), and `curl https://hireling.flinntech.com/healthz` returns `200 {"status":"ok","version":…}`. Every command in the runbook is copy-pasteable and each has a stated observable.

**Acceptance Scenarios**:

1. **Given** a clean git clone at a chosen commit on Mimir, **When** the runbook's build step runs, **Then** a single image is produced containing the release binary, the built `web/dist` bundle, and CA certificates — and nothing else writable.
2. **Given** the image, **When** the container starts with the runbook's environment (the `HIRELING_*` set + `RUST_LOG`), **Then** it applies embedded migrations at boot and serves on :3000; a release build missing its OIDC legs or cookie key refuses to boot (existing fail-closed, `src/lib.rs`).
3. **Given** the container, **When** it is stopped and removed, **Then** nothing is lost except its own logs — all persistent state lives in the Asgard hall (PRD Hosting & Ops).
4. **Given** a new git commit, **When** the update steps run (rebuild, replace container), **Then** the new version serves and the old container is gone; the rollback steps do the same in reverse from the previous tag.

### User Story 2 — A player opens the URL and lands at the table (Priority: P0)

Becky browses to `https://hireling.flinntech.com`, Authentik authenticates her, and she lands on the party screen with her sheet one tap away — the PRD's Entry Point, end-to-end, on the deployed origin, for the first time over the real tunnel and the real IdP.

**Why this priority**: This is the whole product arriving where its users are. It is also the deployment's first true end-to-end test: tunnel, TLS, OIDC redirect, cookie key, hall, corpus, WS sync — every layer at once.

**Independent Test**: From two different clients (two browsers/devices), complete Authentik login and open the party screen; commit a write on one and observe it on the other without reload — this also retires E10 SC-1's deferred live two-client sync observation (gh#13 carry-over comment).

**Acceptance Scenarios**:

1. **Given** the deployed origin with the Authentik app registered per the runbook (redirect URI `https://hireling.flinntech.com/api/auth/callback` — exact, from `config.rs::redirect_uri`), **When** a party member browses the URL, **Then** login completes and the party screen renders.
2. **Given** two authenticated clients, **When** one commits a synced write, **Then** the other reflects it without reload (the E7 WS contract over the deployed path).
3. **Given** the deployed origin, **When** the PWA install + offline-reload checklist (gh#179) is walked on it, **Then** its four items pass — install affordance, standalone window, offline read-only reload, second-build cache discard — and results are recorded on gh#179.

### User Story 3 — Heimdall watches, and an operator can tell dead-from-dependency (Priority: P1)

Heimdall probes `https://hireling.flinntech.com/healthz`. The container's own healthcheck hits the same endpoint locally. Because `/healthz` is dependency-free by design (`src/health.rs` — it answers when Postgres is down), a red probe means the serving path is down, and a green probe with errors elsewhere means a dependency problem — two different runbook entries.

**Why this priority**: Session uptime is a PRD Technical Metric measured by Heimdall. The healthcheck wiring is an epic AI-guardrail.

**Independent Test**: `docker inspect` shows the container `healthy` with the HEALTHCHECK wired to `/healthz`; Heimdall's probe against the public URL reports up; stopping the app turns the local healthcheck `unhealthy` within its configured window.

**Acceptance Scenarios**:

1. **Given** the running container, **When** Docker evaluates the image's HEALTHCHECK (`GET /healthz` on localhost), **Then** the container reports `(healthy)` while serving.
2. **Given** Heimdall configured per the runbook (probe URL `https://hireling.flinntech.com/healthz`), **When** the app is serving, **Then** the monitor shows the service up.
3. **Given** the hall unreachable, **When** the app is asked to boot, **Then** the boot fails closed (schema-before-traffic, `db.rs`) — the container restarts into the same failure, logs name the dependency, and the healthcheck never reports a lie.

### User Story 4 — An update replaces the shell without losing the table (Priority: P1)

A second build is deployed. The table's clients pick up the new shell; the service worker's release-keyed cache discards the prior release's precache on activation (E10's built behavior); nobody's data moves. If the new build is bad, the rollback steps restore the previous tag in minutes.

**Why this priority**: Rebuild-from-git is the settled deployment shape; the update and rollback paths are where a deploy procedure earns its keep. gh#179's fourth item (second-build cache discard) is verified here, on the deployed origin, in a real browser.

**Independent Test**: Deploy build A, observe `BUILD_ID` in the served `sw.js`; deploy build B, observe in DevTools `caches.keys()` that A's cache is gone after activation — the observation recorded on gh#179.

### User Story 5 — Session night is a checklist, not a scramble (Priority: P0)

Before the first session, and before any session night: the readiness checklist (Mimir up, Asgard up, tunnel up, probe green, corpus present) is walked and everything on it is green or named; the backup-restore rehearsal has been run once and its record exists — an unverified backup is a hope, not a backup (epic AI-guardrail).

**Why this priority**: PRD makes session-night infra health a P0 operational dependency; the guardrail makes the restore rehearsal mandatory before the first game session. The rehearsal record is what turns Asgard's backup rotation from a setting into a recovery capability.

**Independent Test**: The checklist exists as a document with a verification command and expected observable per item; the rehearsal procedure, executed once, produces a record (a comment on gh#13 or a doc artifact) naming the dump restored, the scratch hall used, the checks passed (row counts, a login, a roster read), and the teardown.

**Acceptance Scenarios**:

1. **Given** the readiness checklist, **When** an operator walks it on session night, **Then** every item resolves to green (command + expected observable) or names the failing asset and its owner.
2. **Given** Asgard's backup rotation, **When** the rehearsal procedure runs, **Then** the latest dump restores into a scratch hall, the app boots against it read-verified, and the scratch hall is torn down — once, before the first session, and recorded.
3. **Given** the go-live checklist, **When** everything on it is checked (deploy, login, two-client sync, gh#179's four items, rehearsal record), **Then** the deployment is *ready* — and the first session remains Josh's call (H1).

## Edge Cases

- **Hall unreachable at boot** — fail closed by design (`db.rs::connect_and_migrate` before serving); container restarts into a named failure; runbook's "app won't boot" entry points at Asgard, not at the image.
- **Tunnel up, app down (or the reverse)** — Heimdall sees the public probe; the container healthcheck sees the process. The runbook's triage table pairs the two signals with the asset that owns each.
- **First deploy before the corpus import** — the app boots and serves, but conditions/tooltips are empty; the runbook orders the one-time `import` operator job (`src/lib.rs::dispatch`) before first session. The license verdict stays yellow-green in a private deployment (E4 gate: it gates *public* features only — unchanged by this epic).
- **Cookie key rotated or mismatched** — sessions invalidate; single replica is the deployment invariant (hall `CONNECTION LIMIT 20`, app pool 10); the runbook states one replica, one key, and where the key lives (house secret store).
- **SIGTERM mid-broadcast** — the drain watch (E1) sends `bye` + Close 1001 to every live WS session and drains in-flight HTTP within the 10s window (`src/http.rs::DRAIN_TIMEOUT`); clients reconnect per the E7 contract. The container's stop grace period must exceed the drain window (FR-3).
- **Backup rotation silently missing the hall** — exactly what the rehearsal exists to catch; a failed rehearsal is a stop-the-line finding against Asgard's rotation, reported through Thrane to Josh, not papered over.
- **Unraid host reboot** — container restart policy (`unless-stopped`) + autostart restores service; statelessness means nothing else to recover; readiness checklist covers it.

## Requirements *(mandatory)*

### Functional Requirements

Everything below cites the repo fact it builds on, all verified on `main` at `c4ae46d`.

- **FR-1 — Container image.** A multi-stage `Dockerfile` (new, repo root) builds from a git checkout: stage one builds `web/dist` (npm) and the release binary (cargo); stage two carries binary + bundle + CA certificates on a minimal base, listening on `HIRELING_PORT` (default 3000), running as a non-root user, stateless (no volumes, no writable application paths). The `compose.yml` dev twin is untouched.
- **FR-2 — Healthcheck.** The image declares `HEALTHCHECK` wired to `GET /healthz` locally (endpoint: `src/health.rs`, `src/http.rs` — dependency-free by design), with interval/timeout/retries/start-period stated in the runbook and observable via `docker inspect`.
- **FR-3 — Signal path.** The binary is PID 1 (exec-form `ENTRYPOINT`, no shell wrapper) so Docker's SIGTERM reaches the existing drain path (WS `bye` + Close 1001, HTTP drain, 10s window — `src/http.rs::serve`); the runbook sets `stop_grace_period` above the drain window (≥ 15s) so `docker stop` never SIGKILLs mid-drain.
- **FR-4 — Logs.** The app logs to stdout (tracing, request IDs on every request via `x-request-id`, `RUST_LOG` filterable — existing). No log files in the image; the runbook names where Mimir shows container logs today (Unraid Docker log view / `docker logs`) and what a healthy boot looks like in them (the `starting`/`serving` lines from `lib.rs`/`http.rs`).
- **FR-5 — Secrets and environment.** The runbook carries the complete environment table — `RUST_LOG`, `HIRELING_PORT`, `HIRELING_DATABASE_URL`, `HIRELING_STATIC_DIR`, `HIRELING_OIDC_ISSUER`, `HIRELING_OIDC_CLIENT_ID`, `HIRELING_OIDC_CLIENT_SECRET`, `HIRELING_BASE_URL`, `HIRELING_COOKIE_KEY` (names + shape from `src/config.rs`) — with values sourced from the house secret store, never in git, never baked into the image (the image must build without secrets; `HIRELING_BASE_URL=https://hireling.flinntech.com`).
- **FR-6 — Deploy runbook.** `docs/deploy.md` (new) is the single operating document: prerequisites (hall provisioned per `db/provision.sql`, reachable over `asgard-net`); build-from-git on Mimir; container placement (network attach, env, restart policy, stop grace); Authentik app registration with the exact redirect URI `https://hireling.flinntech.com/api/auth/callback`; cloudflared hostname route per the existing pattern (dwarfcampaign precedent); Heimdall probe (`https://hireling.flinntech.com/healthz`); one-time corpus import (`import` subcommand against the hall) and the license posture (private deployment, gate stays yellow-green); update (rebuild from newer commit) and rollback (previous tag) paths. Every step names its observable.
- **FR-7 — Backup-restore rehearsal.** A procedure in the runbook: restore the hall's latest backup from Asgard's rotation into a scratch hall, boot the app against it, verify (row counts on the core tables, one login, one roster read), tear the scratch hall down. Executed once before the first session, recorded on gh#13. Execution requires Asgard access (H-side; procedure and verification queries are ours).
- **FR-8 — Session-night readiness checklist.** A section of the runbook: Mimir up, Asgard up, tunnel up, Heimdall probe green, corpus present, latest deploy known — each with a command and expected observable. Executed by Josh (H2).
- **FR-9 — Go-live checklist.** A section of the runbook folding the two carry-overs: gh#179's four browser items (install affordance, standalone window, offline read-only reload, second-build cache discard — run on the deployed origin, recorded on gh#179) and E10 SC-1's live two-client sync observation. Ready ≠ live: the first session is H1.
- **FR-10 — README pointer.** README's Production section links the runbook (it already names the hall and provisioning; the deploy half completes it).

### Key Entities

- **Container image** — the stateless artifact: release binary + built frontend + CA certs; rebuilt from git, never patched in place.
- **The hall** — database `hireling` on Asgard, the only persistent state (E2's provisioning, `db/provision.sql`); rides the existing backup rotation.
- **The runbook** — `docs/deploy.md`: deploy, update, rollback, readiness, rehearsal, go-live checklist.
- **The probe** — `/healthz`, one endpoint serving two consumers (Docker healthcheck locally, Heimdall publicly).

### Constraints (settled by the epic — non-negotiable)

- Zero new infrastructure: existing Mimir, Cloudflare, Authentik, Asgard assets only; no new vendors, no paid services, no registry (rebuild-from-git on Mimir is the settled shape).
- Private deployment: no public exposure; E4's license gate stays yellow-green — this epic changes nothing about it.
- The Asgard hall is the only persistent state; the container is stateless by construction.
- No CI/CD changes in this epic: grizzly-gate and the `gate` workflow are Vex's lane; adding docker-build-to-CI would be a dependency on her and is out of scope here (noted as a possible follow-up for Thrane to route, never assumed).
- Single replica. Six users, headroom sixty (PRD Performance) — no scaling work exists in this epic.
- Handoffs H1–H3 are binding (above); a requirement that cannot be met on existing assets stops the line instead of buying.

### Success Criteria

Checkable against a running container, not intent. Ours to tick unless marked *(handoff)*:

- **SC-1**: From a clean git clone, the runbook's build step produces the image; the container boots against the hall and `GET /healthz` answers `200 {"status":"ok",…}` — verified locally (`docker exec` / curl :3000) and through the tunnel (`https://hireling.flinntech.com/healthz`).
- **SC-2**: `docker stop` shows the drain in the container logs (WS `bye` + close, `in-flight requests drained`) and exit code 0 within the grace window — the epic's graceful-shutdown guardrail observed in the deployed shape.
- **SC-3**: `docker inspect` reports `(healthy)` with the HEALTHCHECK wired to `/healthz` while serving; Heimdall's probe against the public URL reports up. *(Heimdall wiring: handoff-side step, runbook-exact.)*
- **SC-4**: End-to-end login on the deployed origin (Authentik → party screen), and the two-client live sync observation recorded — the PRD Entry Point and E10 SC-1's manual half.
- **SC-5**: gh#179's four browser items pass on the deployed origin; results recorded on gh#179, which then closes on its own terms.
- **SC-6**: The backup-restore rehearsal executed once and recorded on gh#13 before the first session. *(execution: handoff-side; procedure + verification queries ours)*
- **SC-7**: `docs/deploy.md` exists and is complete per FR-6/FR-7/FR-8/FR-9; the README links it.
- **SC-8**: `just ci-local` green on the branch that adds the Dockerfile and docs (the repo gate; the image itself cannot be built bench-side — see the split below — so the gate certifies the repo, the first deploy certifies the image).

### Assumptions

- `asgard-net` is attachable from the hireling container on Mimir (single-host Docker topology; `db/provision.sql` states the bridge is the only reachability). **Verified at deploy; if false, it is a stop-the-line finding** — a cross-host network link would be new infrastructure (H3).
- The existing cloudflared pattern accommodates a second hostname under the same tunnel/zone without new spend (dwarfcampaign precedent; Cloudflare free-tier). Verified at tunnel configuration.
- The Authentik instance can register a new application with the runbook's redirect URI (existing asset, standard capability); the six static accounts exist (E3's assumption carried forward).
- The hall is provisioned (E2 closed with `db/provision.sql` checked in); the runbook's first step verifies rather than assumes it.
- Mimir builds linux/amd64 images from a git clone on-host; build caching (BuildKit cache mounts) keeps occasional rebuilds tractable — no registry, no cross-arch.
- gh#179 remains open until its four observations are recorded on it; this epic's SC-5 references it rather than re-specifying it.

### Out of scope (guarded)

- CI/CD pipeline changes of any kind — docker builds in the gate, image publishing, deploy automation (Vex's lane; a follow-up for Thrane to route if wanted, never assumed here).
- Any registry, image scanner, or new monitoring/logging/aggregation asset — Heimdall and Unraid's container logs are the existing assets and are sufficient.
- Public launch work: license-green push, public exposure, marketing surfaces (P2-lane concerns gated by E4's verdict).
- Load testing beyond the six-user session reality; HA, multi-replica, blue/green (PRD: success gets a redesign conversation, not this epic).
- Any application-code change. E11 adds a Dockerfile, a `.dockerignore`, and documents. If a gap found during deploy seems to need code, that is a new issue against the owning epic, not a rider on this one.
- Phone/tablet layout, dice roller, and every other Constitution Article I non-goal.

## What seats can build and verify without touching Josh's hosts (explicit, per the task ask)

**Buildable and verifiable bench-side (start immediately):**
- `Dockerfile` + `.dockerignore` (FR-1/2/3) — reviewed against the checklist; the bench has no Docker daemon (verified: no `docker`, no `podman` on the seats), so the image's *build and boot* verify at first deploy, not bench-side. Everything the image must contain and do is specified so that the first deploy is a verification, not an exploration.
- `docs/deploy.md` — deploy/update/rollback, env + secrets table, readiness checklist, rehearsal procedure, go-live checklist (FR-4–FR-9).
- README pointer (FR-10). `just ci-local` green (SC-8).

**Requires Josh's hosts or assets (handoff-side, runbook-exact, each with an observable):** image build + first boot on Mimir; Authentik app registration; cloudflared hostname; Heimdall probe target; corpus import against the production hall; restore-rehearsal execution; the gh#179 browser pass; session-night readiness execution; and the go-live call itself (H1).

## Clarify Log (settled 2026-10-14, from the existing corpus — no question required Josh)

1. **Who runs the host-side steps?** The PRD names Josh the owner of session-night readiness; the same reading extends to the assets only he administers (Authentik, tunnel, Mimir placement, Asgard restore). Settled: we ship procedures with exact values and observables; execution is his or delegated at his direction. Folded into the handoff framing and FR-6–FR-9.
2. **"Structured logs" — JSON?** The app's tracing output is already fielded (`key=value`, spans, request IDs) on stdout; the consumer is wherever Mimir containers log today (Unraid's Docker log view), which reads text, not a log pipeline. Settled: keep stdout tracing as-is; JSON output rejected as a solution looking for a consumer (Constitution Art. II). Folded into FR-4.
3. **gh#179 — fold or stay separate?** Its own body says it "rides the go-live pass" and closes on four recorded observations. Settled: folded as FR-9/SC-5 (the go-live checklist runs it on the deployed origin, results recorded on gh#179); the issue stays its own record — re-specifying its items in E11 would create a second place to check the same fact.
4. **Can the image be verified bench-side?** No Docker on the seats and the gate has no docker step (and adding one is Vex's lane). Settled: the first deploy on Mimir is the image's verification gate, structured by the runbook's observables (SC-1/2/3); CI docker build recorded as a rejected-for-now alternative with the routing note.
5. **Image base and healthcheck exec?** A design-space question with real trade-offs (distroless vs slim; curl vs a compiled probe). Settled in `design.md` D1 with alternatives recorded — slim + the platform's own tools, grug-brained, no new moving parts.
6. **Restore rehearsal — who executes?** Requires Asgard access. Settled: procedure + verification queries are ours (FR-7); execution is handoff-side and must complete before the first session (guardrail), recorded on gh#13.
