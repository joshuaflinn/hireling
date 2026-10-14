# Design: Go-Live Deploy (E11)

**Spec**: `specs/012-go-live-deploy/spec.md` (requirements, handoffs H1–H3, bench/host split)
**Signed**: Korrin, 2026-10-14 — epic-internal P0 scope; no P0/P1/P2 line moved, no CONSTITUTION change, no new dependency, no spend. The two-signature cases in my charter do not apply; Thrane's assignment ("carry it into sdd-design + sdd-plan if the spec comes out clean enough") is the lane's authorization, and Thrane reads the result on the card.

---

## 1. Context and shape of the problem

E1–E10 are closed; the application is complete and un-deployed. E11 adds the *shipping container* (literal) and the *operating documents*. The application half of every guardrail already exists on `main` — this design's discipline is to **consume, not rebuild**:

| Guardrail (epic) | Already on `main` | E11 adds |
| --- | --- | --- |
| Healthcheck wired to `/healthz` | `src/health.rs` (pure, dependency-free), routed in `src/http.rs` | Dockerfile `HEALTHCHECK` + Heimdall wiring doc |
| Graceful shutdown drains WS | `src/http.rs::serve` — SIGINT/SIGTERM → drain watch → `bye` + Close 1001 per session, HTTP drain, 10s `DRAIN_TIMEOUT` | exec-form ENTRYPOINT (PID 1) + `stop_grace_period` > window |
| Structured logs where Mimir logs | tracing to stdout, `x-request-id` on every request, `RUST_LOG` filter | nothing in code; runbook names the consumer and the healthy-boot lines |
| Verified backup-restore rehearsal | hall rides Asgard's rotation (`db/provision.sql`) | the rehearsal procedure + verification queries + the execution record |

The genuinely new artifacts are exactly four: `Dockerfile`, `.dockerignore`, `docs/deploy.md`, and a README pointer. No application code changes — a code gap found at deploy is a new issue against the owning epic, not a rider (spec Out-of-scope).

## 2. Decisions, with alternatives

### D1 — Image: debian-slim runtime, multi-stage, healthcheck via a builder-built probe binary

**Decision.** Stage 1 (`rust:1-bookworm`-family builder + node for the web build): `npm ci && npm run build` in `web/`, then `cargo build --release`. Stage 2 (`debian:bookworm-slim`): install `ca-certificates`; copy the binary and `web/dist`; create a non-root user; `EXPOSE 3000`; `ENTRYPOINT ["/usr/local/bin/hireling"]` (exec form — PID 1, SIGTERM reaches the drain path); `HEALTHCHECK` runs a tiny static probe binary (`/usr/local/bin/healthcheck` → connects to `127.0.0.1:$HIRELING_PORT/healthz`, exit 0 on 200), built in stage 1 as a second `cargo build` of a ~30-line `src/bin/healthcheck.rs`-style crate member (no deps beyond std + a minimal HTTP client hand-rolled over `std::net::TcpStream` — it speaks one GET and reads one status line).

**Alternatives rejected.**
- *distroless runtime* — smallest and safe, but carries no shell and no probe tool; a healthcheck exec would still need shipping a binary, and debugging a sick container (`docker exec` anything) is impossible. The byte savings are irrelevant at six users; operability is not (Constitution Art. II: the boring choice).
- *`curl` or `wget` in the runtime image* — means `apt-get install` in the runtime stage (network fetch per rebuild, layer churn) or a busybox sidecar; a std-only probe binary is one more small build target, zero runtime packages, and the same file everywhere.
- *Healthcheck via the main binary subcommand* (e.g. `hireling healthcheck`) — attractive (no second binary), but couples the probe to the full app binary and its settings-loading; the probe must work when the app is wedged, and the simplest wedge-detector is one that shares no code with the thing it watches. Revisit if maintainers hate the second target.

### D2 — Rebuild-from-git on Mimir, BuildKit cache mounts, no registry

**Decision.** The runbook's build step is `docker build` from a git clone on Mimir, with BuildKit cache mounts (`--mount=type=cache,target=/usr/local/cargo/registry` and `target/`, plus npm's cache) so an occasional rebuild costs minutes, not an hour. No registry, no CI publishing — the settled constraint (zero new infrastructure) and the settled shape (rebuild-from-git).

**Alternatives rejected.** *GHCR/registry publishing from CI* — free tier exists, but it is a new asset, a new auth path, and CI docker builds are out of scope (Vex's lane); rejected for this epic, noted as a Thrane-routable follow-up. *cargo-chef* — better layer caching for frequent rebuilds; this app rebuilds occasionally, cache mounts suffice; a new tool in the hot path is not grug. *Building on a bench and shipping a tarball* — no docker on benches (verified), and it would couple deploys to a seat's toolchain.

### D3 — Container lifecycle: exec-form PID 1, `stop_grace_period: 15s`, `restart: unless-stopped`

**Decision.** The binary is PID 1 via exec-form `ENTRYPOINT`; no shell wrapper, no `tini` — tokio reaps its own children and the app spawns no orphans worth a supervisor. Docker's 10s default SIGKILL window equals the app's `DRAIN_TIMEOUT` (10s) — a race the app loses half the time; the runbook sets `stop_grace_period` to 15s so the drain completes, and the runbook's stop-observation step (SC-2) proves it. Restart policy `unless-stopped` + Unraid autostart covers host reboots.

**Alternatives rejected.** *`init: true` (docker-init/tini)* — harmless zombie insurance but another moving part for a process model that has no zombies; documented as an operator option, not the default. *Lowering `DRAIN_TIMEOUT` in code* — touches application code for a deployment-shape problem; 15s of grace is free.

### D4 — Logs: stdout tracing as-is; Unraid's Docker log view is the consumer

**Decision.** No log-format change. The app writes fielded tracing lines (request IDs, spans) to stdout; Docker captures them where Mimir containers already log (Unraid's per-container log view / `docker logs`). The runbook documents: `RUST_LOG=info` default, what a healthy boot looks like (`starting` → `serving`, from `lib.rs`/`http.rs`), where request IDs appear, and the grep for session-night triage.

**Alternatives rejected.** *JSON logs (`tracing-subscriber` json feature)* — a consumer doesn't exist (no log pipeline on the existing assets), and Unraid's viewer reads text; a format switch is code churn serving nobody (Art. II). *Log shipping to anywhere new* — zero-new-infrastructure constraint; out of scope by law.

### D5 — First deploy is the image's verification gate; the runbook makes it a checklist, not an exploration

**Decision.** Benches have no Docker and the gate has no docker step, so the image cannot be built or booted before Mimir. The design accepts that honestly: SC-1/2/3 are *deploy-time* checks with exact commands and expected observables, so the first deploy produces evidence (healthz output, drain log lines, `docker inspect` health status) rather than judgment calls. The Dockerfile itself gets bench-side review against a written checklist (spec FR-1/2/3 terms) and `just ci-local` stays green on the branch.

**Alternatives rejected.** *Docker-in-gate* — Vex's lane; a CI asset change is out of scope and would silently couple every future PR to an image build. *docker on the benches* — provisioning ask for Thrane, not something this epic assumes; recorded as a possible throughput improvement, not a dependency.

### D6 — gh#179 folds as the go-live checklist's PWA section; the issue remains the record

**Decision.** `docs/deploy.md`'s go-live checklist carries gh#179's four items verbatim (referenced, not re-specified) plus E10 SC-1's two-client observation. Results are recorded **on gh#179**, which closes on its four observations — its own body's terms. E11's SC-5 points at that record.

**Alternatives rejected.** *Copying the four items into E11's acceptance and closing gh#179 by reference* — two places checking the same fact invites one of them to rot (the exact failure mode gh#179 exists to prevent: an "honestly outstanding" checklist item that nobody ran).

### D7 — One runbook file, `docs/deploy.md`, six sections

**Decision.** Deploy prerequisites → build & first deploy → update & rollback → session-night readiness → backup-restore rehearsal → go-live checklist. One file, one anchor, linked from README's Production section (which already owns the hall/provisioning half). Every step: command, expected observable, and the asset that owns it.

**Alternatives rejected.** *Split files per procedure* — six files for a six-user deployment is inventory without benefit; a single scrollable runbook matches how it will actually be used at 18:50 on a session night. *Wiki/on-repo split* — the repo is the record of the work; the wiki is not.

## 3. File map

| Path | New/changed | Contents |
| --- | --- | --- |
| `Dockerfile` | new | multi-stage build, non-root, HEALTHCHECK, exec ENTRYPOINT (D1/D3) |
| `.dockerignore` | new | keep the context to source + manifests (no `.git`, no `target/`, no `node_modules`) |
| `src/bin/healthcheck.rs` (or equivalent crate member) | new, ~30 lines | std-only localhost `/healthz` probe (D1) — the only code-shaped artifact in E11 |
| `docs/deploy.md` | new | the runbook (D7): six sections per FR-6/7/8/9 |
| `README.md` | changed | Production section links the runbook (FR-10) |
| `.specify/feature.json` | changed | point at `specs/012-go-live-deploy` (pipeline convention) |

## 4. Data flow (one direction, no forks)

git (Mimir clone) → docker build (web/dist + release binary + probe) → container (PID 1 binary, env from house store) → asgard-net → hall (schema at boot). Inbound: browser → Cloudflare edge → cloudflared (existing pattern) → container :3000. Observability: /healthz ← (local probe binary, Heimdall via tunnel); stdout logs → Docker/Unraid log store. Persistent state exists in exactly one place: the hall.

## 5. Error handling

Boot failures fail closed (existing): missing auth env in release, unreachable hall, unbindable port → process exits with a named error; container restart policy loops it; healthcheck stays absent/starting, never lying (US3.3). Runtime dependency loss is surfaced by the *absence* of updates, not a second health path — `/healthz` is process-liveness by design, and the runbook's triage table maps (public probe, local health, boot log) triples to owning assets. Deployment failures during first deploy are expected to iterate through the runbook's observables — each failure class names its asset (image vs tunnel vs IdP vs hall) before any fix is attempted.

## 6. Testing strategy (production-path rule, per AGENTS.md — scoped to what E11 can actually execute)

- Bench: `just ci-local` green (the repo gate) — includes the healthcheck probe binary compiling under the existing lint/test discipline (a std-only binary adds trivial surface; unit-test its status-line parse against a canned response).
- Bench: doc-verification — the runbook's every command is reviewed for copy-paste correctness (paths, ports, URIs verified against `src/config.rs`, `db/provision.sql`, and the cloudflared/Authentik patterns it cites).
- Deploy-time (the image's gate, D5): SC-1 healthz through both paths; SC-2 drain observed in logs on `docker stop`; SC-3 healthcheck status flip; SC-4 end-to-end login + two-client sync; SC-5 gh#179 browser pass; SC-6 rehearsal record. These are the spec's success criteria, executed on the deployed origin, recorded on gh#13/gh#179.
- No test theatre: nothing in E11 pretends to be verifiable bench-side that is not. The reachability sweep in the PR body states exactly which criteria verified where.

## 7. Self-review

- Does anything here buy or build infrastructure? No — every artifact is repo content; every host-side action rides an existing asset, named. H3 stands.
- Does anything touch application behavior? One addition (the probe binary, dead code to the app); zero modifications. A deploy that seems to need a code change files an issue instead.
- Is the drain race (default 10s stop vs 10s drain) handled? Yes — D3 makes grace 15s and SC-2 observes the drain completing.
- Is the "verify without hosts" line honest? Yes — D5 names the limit and the compensating structure instead of hand-waving a bench-side docker check that cannot run.
- Weakest point: the healthcheck probe binary is the only new code and it is un-runnable until first deploy. Mitigation: std-only, unit-parsed, reviewed line-by-line, and SC-3's first observation. If it misbehaves on Mimir, the HEALTHCHECK line is a one-line runbook edit away from using the documented curl fallback while it's fixed — recorded in the runbook, not improvised.
