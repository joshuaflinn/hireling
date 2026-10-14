# Go-Live Deploy (E11) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use sdd-implement (subagent-driven, recommended) to implement this plan task-by-task, or its Inline Execution mode. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the deployable shape of Hireling: a stateless container image built from git, and `docs/deploy.md` as the single operating document (deploy, update/rollback, session-night readiness, backup-restore rehearsal, go-live checklist folding gh#179 + E10 SC-1's live observation). No application-code change beyond a std-only healthcheck probe binary.

**Architecture:** Packaging epic. Everything the app must do at deploy time already exists on `main` (`/healthz`, drain watch + WS `bye`/1001, app-boot migrate, release fail-closed auth). E11 adds `Dockerfile`, `.dockerignore`, the probe binary, `docs/deploy.md`, a README pointer. First deploy on Mimir is the image's verification gate — benches have no Docker.

**Spec:** `specs/012-go-live-deploy/spec.md` (FR-1–FR-10, SC-1–SC-8, handoffs H1–H3, bench/host split) · `design.md` (D1–D7). Executors read spec + design + this plan.

**Lane/heat proposal:** One lane, one seat, one PR. Task order below is the order; total is a small PR — splitting it costs more than it saves.

## Global Constraints

- Branch: the harness-provisioned `MOR-<n>` branch (one issue, one branch, one PR; never `main`). Claim gh#13 on GitHub before the first commit.
- Every commit message ends with `Co-Authored-By: Paperclip <noreply@paperclip.ing>` — exactly that line, no agent names.
- Zero new infrastructure (spec constraint; H3). No CI changes — nothing in `.github/`, no gate edits (Vex's lane).
- No application-behavior change. The probe binary is the only new code and it is std-only.
- Secrets never in the repo, the image, or the docs — names and shapes only (`src/config.rs` is the source of truth).
- `just ci-local` green before the PR; paste the output in the PR body with the reachability sweep stating which SCs verify bench-side at the implementation PR (SC-7, SC-8) and which verify at deploy (SC-1–SC-6). The spec PR itself verifies none — do not claim any.
- The PR does **not** close gh#13 — the epic closes when the deployed reality passes its criteria; the PR body says so and links the Paperclip task.

## Tasks

### Task 1 — Healthcheck probe binary (the only code)

- [ ] Add `src/bin/healthcheck.rs`: std-only; reads `HIRELING_PORT` (default 3000); opens `TcpStream` to `127.0.0.1:port`; writes `GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n`; parses the status line; exit 0 on `200`, non-zero otherwise, with a one-line stderr reason. Note: `clippy::indexing_slicing` and `string_slice` are denied workspace-wide — status-line parsing is exactly where a seat reaches for `&line[9..12]`; parse with `split(' ')`/`split_once(' ')` and `starts_with`, no slices.
- [ ] Unit-test the status-line parse against canned responses (`200 OK` → ok; `500`, garbage, EOF → not ok). Two-fixture rule: at least one passing and one failing fixture. Test placement follows the sibling-`tests/` convention (`docs/toolkit-conventions.md`): tests live at `src/bin/tests/healthcheck.rs`, declared at the bottom of the binary as `#[cfg(test)] #[path = "tests/healthcheck.rs"] mod tests;` — not in a `#[cfg(test)]` block inside the impl file.
- [ ] `cargo build --release` compiles it; clippy-deny block and fmt pass (it rides the workspace lint set).
- **Verify:** `cargo test healthcheck && cargo clippy --workspace --all-targets` (or the workspace equivalents inside `just ci-local`).

### Task 2 — Dockerfile + .dockerignore (D1/D2/D3)

- [ ] Multi-stage per design D1's pinned tags: stage `web` `node:22-bookworm-slim` → `web/dist` (`npm ci`, `npm run build`); stage `build` `rust:1.99.0-bookworm` → the release binary + Task 1's probe, with BuildKit cache mounts (cargo registry, target, npm cache); runtime `debian:bookworm-slim` + `ca-certificates` + non-root user + binary + bundle + probe, copied from both build stages.
- [ ] `EXPOSE 3000`; `ENTRYPOINT` exec-form (PID 1); `HEALTHCHECK CMD ["/usr/local/bin/healthcheck"]` with `--interval=30s --timeout=5s --retries=3 --start-period=60s` (design D1; start-period covers boot-time migrate + bind — `db.rs::connect_and_migrate` before `http.rs::serve`).
- [ ] `.dockerignore`: no `.git`, `target/`, `node_modules/`, workspace scratch; keep source + manifests so the context is small.
- [ ] Self-review the Dockerfile against spec FR-1/2/3 line by line (bench cannot build it — review is the bench gate; first deploy is the build gate).
- **Verify:** `just ci-local` (unchanged repo surface stays green) + the FR-1/2/3 checklist in the PR body.

### Task 3 — `docs/deploy.md` (D7; FR-4–FR-9)

- [ ] **§1 Prerequisites:** hall provisioned (`db/provision.sql` — verify, don't assume), `asgard-net` attachability (stop-the-line if absent — H3), house secret store entries for the env table, Authentik app creation with redirect URI `https://hireling.flinntech.com/api/auth/callback` (exact, from `src/config.rs::redirect_uri`).
- [ ] **§2 Build & first deploy:** git clone/checkout on Mimir; `docker build`; container run (network, env table from FR-5, `stop_grace_period: 15s`, `restart: unless-stopped`); cloudflared hostname per the existing pattern; Heimdall probe `https://hireling.flinntech.com/healthz`; one-time corpus import (`import --release pf2e-8.5.1` — the parser requires exactly `--release <pf2e-x.y.z>` (`src/import/args.rs`); canonical tag per `README.md`) + license posture note (private → gate stays yellow-green); record the observed first-boot time and tune `--start-period` toward it per design D1; **observables:** healthz via both paths, healthy boot log lines, `docker inspect` health.
- [ ] **§3 Update & rollback:** rebuild from newer commit; verify BUILD_ID moved in served `sw.js`; rollback = previous tag, same steps; observables per step.
- [ ] **§4 Session-night readiness (FR-8, H2):** Mimir up, Asgard up, tunnel up, probe green, corpus present, deployed version known — each a command + expected observable.
- [ ] **§5 Backup-restore rehearsal (FR-7):** restore latest rotation dump into scratch hall; boot app against it; verify (core-table row counts, one login, one roster read); teardown; record on gh#13. Executed before first session (handoff-side; procedure ours).
- [ ] **§6 Go-live checklist (FR-9):** end-to-end login (PRD Entry Point), two-client live sync (E10 SC-1 manual half), gh#179's four items referenced verbatim with results recorded on gh#179, rehearsal record present. Ends with: ready ≠ live; first session is Josh's call (H1).
- [ ] Every command copy-paste-checked against the cited sources; every step names its owning asset.
- **Verify:** doc review against FR-4–FR-9 (checklist in PR body); `just ci-local`.

### Task 4 — README pointer + pipeline bookkeeping (FR-10)

- [ ] README *Production (Asgard)* section: link `docs/deploy.md` as the deploy half.
- [ ] `.specify/feature.json` → `specs/012-go-live-deploy`.
- **Verify:** `just ci-local`; link resolves.

### Task 5 — PR

- [ ] Branch up to date with `origin/main` before the review ask; `just ci-local` output pasted; reachability sweep stating bench-verified at the implementation PR (SC-7, SC-8) vs deploy-verified (SC-1–SC-6) criteria — the spec PR verified none; PR body links the Paperclip task, states it does not close gh#13, and carries the review ask to the round per AGENTS.md (announce → verdict → merge ask to Thrane as a task, never a card).

## Self-review

- No task touches `.github/`, buys anything, or edits application behavior — H3 and the out-of-scope list hold.
- The bench/host split is stated where it bites (Task 2 review-vs-deploy, Task 5 sweep) — no criterion is claimed verified bench-side that is not.
- gh#179 is never re-specified, only referenced; it stays the record for its four observations.
- Sequencing is strict because it is one small PR: probe → image → docs → pointer → PR. Parallelizing would produce merge overhead exceeding the work.
