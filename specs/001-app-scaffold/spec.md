# Feature Specification: App Scaffold & Dev Stack (E1)

**Epic**: E1 — Phase 0, P0, blocks everything · GitHub issue #3
**Created**: 2026-09-18
**Status**: Draft
**Input**: `docs/EPICS.md` Epic E1 specify prompt + Constraints + AI Guardrails (decomposed from PRD v3.6)

## User Scenarios & Testing *(mandatory)*

E1 ships no product features. Its "users" are the builders of the fourteen
downstream epics — Josh, Dave, and the AI coding agents they drive. Every
story below is about how fast and how safely those builders can move.

### User Story 1 — Clone to running app (Priority: P1) 🎯 MVP

A developer clones the repo and, with only the documented prerequisites
installed, runs one documented command sequence to get: the backend serving
the API, the frontend shell visible in a browser, and a throwaway Postgres
available for later epics to target.

**Why this priority**: This is the entry point for every later epic. If
clone-to-running is slow, fragile, or requires tribal knowledge, all of
Phase 0/1 stalls behind it.

**Independent Test**: On a clean checkout, follow only the README. Verify
the shell renders in a browser and `/healthz` returns 200 with a version
string. No product functionality required.

**Acceptance Scenarios**:

1. **Given** a clean clone on a machine with the documented prerequisites,
   **When** the developer runs the documented setup and dev commands,
   **Then** the backend serves the frontend shell at the documented local
   address.
2. **Given** the app is running, **When** anyone requests `GET /healthz`,
   **Then** it returns HTTP 200 with a body containing a version string.
3. **Given** Postgres is not running, **When** `/healthz` is requested,
   **Then** it still returns HTTP 200 (the endpoint is dependency-free).
4. **Given** the documented env vars are unset, **When** the backend starts,
   **Then** it comes up with documented local-dev defaults.

---

### User Story 2 — Verify before claiming done (Priority: P1)

A developer or agent can run one command each for the full test suite and
for a local run of the same CI gate that guards PRs — before pushing.

**Why this priority**: Constitution Article III requires agents to verify
with pasted evidence before claiming done. That is only possible if the
gate and tests run locally with zero extra setup.

**Independent Test**: Run the test recipe and the gate recipe on a clean
checkout; both exit 0 and their output matches what CI would report for the
same commit.

**Acceptance Scenarios**:

1. **Given** a checkout, **When** the developer runs the test recipe,
   **Then** all tests execute and the exit code reflects pass/fail.
2. **Given** a checkout, **When** the developer runs the gate recipe,
   **Then** the same checks as CI run locally and report the same
   pass/fail verdict CI would give that commit.
3. **Given** a change that breaks a lint or test, **When** the recipes run,
   **Then** they fail with output that identifies the failure (no silent
   green).

---

### User Story 3 — Proven frontend delivery path (Priority: P2)

A developer sees a placeholder page at the root route, proving the full
frontend pipeline (author → build → static bundle → served by the backend)
works end to end, so E6's real sheet UI lands on a proven delivery path
instead of scaffolding of its own.

**Why this priority**: The backend-for-frontend serving model (single
binary, no SvelteKit) is a settled constraint; this story proves it before
real UI depends on it.

**Independent Test**: Build and run; load `/` in a browser and confirm the
placeholder route renders, served by the backend binary (no separate dev
server in the loop for the built path).

**Acceptance Scenarios**:

1. **Given** the app is built and running, **When** a browser loads `/`,
   **Then** it receives the frontend bundle and renders the placeholder
   route.
2. **Given** a request for any non-API path that doesn't match a static
   asset, **When** the backend responds, **Then** it serves the shell
   (SPA fallback) rather than a 404.

---

### Edge Cases

- **Postgres absent**: The backend starts and serves API + frontend
  normally; `/healthz` still answers 200.
- **Termination signal mid-request**: The backend drains in-flight
  requests before exiting (graceful shutdown with a bounded wait).
- **Port already in use**: The backend fails fast with a clear message;
  the port is overridable via env var.
- **Missing correlation ID on incoming request**: The backend generates
  one; when present, it is propagated. Every request log line carries one
  either way.
- **Frontend bundle not yet built**: The dev recipe builds it in the right
  order; a contributor never has to know the ordering by heart.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-1**: The system MUST provide a single backend executable that serves
  both the JSON API and the built frontend static assets.
- **FR-2**: The backend MUST expose `GET /healthz` returning HTTP 200 with
  a version string in the response body. The endpoint MUST NOT depend on
  the database or any other external service.
- **FR-3**: The repo MUST contain a frontend shell project that builds to a
  static bundle and renders exactly one placeholder route.
- **FR-4**: Local development MUST provide a throwaway Postgres via a
  single compose command. No developer machine or external database is
  required for the dev loop.
- **FR-5**: The repo MUST provide `just` recipes for at minimum: `dev`
  (run the full local stack), `test` (the full test suite), and a local
  gate run matching CI.
- **FR-6**: All configuration that differs between dev and prod (database
  URL, listen port) MUST come from environment variables, with documented
  local-dev defaults.
- **FR-7**: The backend MUST emit structured JSON logs. Every request log
  line MUST carry a correlation ID — propagated from the incoming request
  when present, generated otherwise.
- **FR-8**: The backend MUST shut down gracefully on termination signals,
  draining in-flight requests within a bounded window.
- **FR-9**: The scaffold MUST extend the existing repo structure in place —
  the vendored toolkit crate at the root, its lint configuration, and the
  grizzly-gate CI configuration are extended, never replaced.
- **FR-10**: The scaffold MUST ship zero product features: no character
  data, no auth, no rules engine, no import pipeline. Placeholders only.

### Key Entities

None. E1 persists no product data. Configuration keys (database URL,
listen port, log settings) are the only named state.

### Constraints (settled by the epic — non-negotiable)

- Rust + axum backend, single binary; the backend owns all routing.
- Svelte (Vite) frontend shipped as a static bundle — no SvelteKit.
- The vendored rust-toolkit crate `hireling` at repo root and the
  grizzly-gate CI (gate-config.json, pinned image) are extended, not
  replaced. Repo: github.com/joshuaflinn/hireling.
- Constitution Article V: new dependencies require written justification in
  the PR. "It's popular" is not one.

### Assumptions

- The frontend project lives in a `web/` subdirectory; the backend crate
  remains at repo root.
- Default dev listen port is 3000, overridable via env var.
- The version string served by `/healthz` is the crate version baked in at
  build time.
- E1 wires **no** database access in the backend. The compose Postgres
  exists so E2 has a target; `/healthz` never touches it.
- Production containerization, tunneling, and deploy are E11 — out of
  scope here.
- Local Postgres carries no seed data; schema and migrations are E2.

## Success Criteria *(mandatory)*

- **SC-1**: A developer new to the repo goes from clone to running app
  (shell visible in a browser, `/healthz` returning 200) in under 10
  minutes following only the README.
- **SC-2**: `/healthz` responds in under 100ms locally, and still returns
  200 with Postgres stopped (verified by stopping the container and
  re-requesting).
- **SC-3**: The local gate recipe reports the same verdict CI gives the
  same commit (8/8 equivalent).
- **SC-4**: The full test suite runs green via one command in under 2
  minutes on a dev machine.
- **SC-5**: A later-epic developer can add a new API route and a new
  frontend page touching only route/component files — no build-plumbing
  changes (verified by a trivial spike before E1 is called done).
- **SC-6**: Graceful shutdown is verified: an in-flight slow request
  completes after SIGTERM, and the process exits within 10 seconds.
