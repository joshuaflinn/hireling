set shell := ["bash", "-cu"]

# Every recipe here is also documented in README.md as the raw cargo command,
# so `just` is convenience, not required. Nothing in CI depends on it.

default: ci-local

# Run the full local stack: build the frontend, then run the backend
# (which serves API + the built bundle on :3000).
dev: web-build
    cargo run

# Run the backend only, against an already-built frontend.
run *args:
    cargo run -- {{ args }}

# Build the frontend bundle into web/dist.
web-build:
    npm --prefix web run build

# Frontend unit tests (vitest + @testing-library/svelte, jsdom — see gh#41).
web-test:
    npm --prefix web test

# Svelte diagnostics over the frontend — the same check the CI gate runs
# as node:svelte-check. Needs `npm --prefix web ci` once per clone.
web-check:
    npm --prefix web run check

# The gate image's node:eslint pass, replicated bench-side. The image
# lints with --no-config-lookup and its own flat config — ES builtins
# only, no browser/node env — so every platform global must be declared
# in-file (/* global */ header; convention: web/src/lib/sync/index.js).
# The replica config is web/gate-eslint.config.mjs, calibrated
# one-for-one against gate run 37850972131 (44 findings before the
# header fixes, clean after, nothing extra). A gate-image digest bump
# that moves the rule set updates that config in the same PR. Needs
# `npm --prefix web ci` once per clone (eslint is a devDependency).
web-eslint:
    cd web && ./node_modules/.bin/eslint --no-config-lookup --config gate-eslint.config.mjs src tests scripts plugins vite.config.js

# The gate image's scan:semgrep pass, replicated bench-side over the web
# tree with the registry rules it has enforced there (calibrated by fire
# against runs 37850972131 and 37981907115: missing-template-string-
# indicator on the emitted-worker template; package-dependencies-check
# on web/package.json — exact versions only, which the tree already
# kept). The image's full rule set is wider; a digest bump that moves it
# updates this invocation in the same PR. Needs the semgrep CLI on PATH
# (`pipx install semgrep`) — a scan that skips is not a scan, so a
# missing CLI fails loudly instead.
scan-semgrep:
    #!/usr/bin/env bash
    if ! command -v semgrep >/dev/null 2>&1; then
        echo "scan-semgrep: semgrep CLI not found — install it (pipx install semgrep) or run the pinned gate image (CONTRIBUTOR.md)" >&2
        exit 1
    fi
    SEMGREP_SEND_METRICS=off semgrep scan --metrics=off --error \
        --config https://semgrep.dev/r/javascript.lang.correctness.missing-template-string-indicator \
        --config https://semgrep.dev/r/json.npm.security.package-dependencies-check \
        web

# Vite dev server for frontend-only iteration (proxies nothing; use `just dev`
# for the real full-stack path).
web-dev:
    npm --prefix web run dev

# Start the throwaway dev Postgres on :5432 (destroyed by `just db-down`).
db:
    docker compose up -d db

db-down:
    docker compose down

# Apply pending migrations with sqlx-cli (`cargo install sqlx-cli --locked
# --features postgres`). The app also migrates itself at boot — these are
# for driving the schema directly.
export DATABASE_URL := "postgres://hireling:hireling@127.0.0.1:5432/hireling"

db-migrate:
    sqlx migrate run

# Revert the most recent migration — keeps the down files honest.
db-revert:
    sqlx migrate revert

# Destroy and recreate the throwaway database, then migrate up from empty.
db-reset: db
    docker compose exec db psql -U hireling -d postgres -c \
        "DROP DATABASE IF EXISTS hireling WITH (FORCE)"
    docker compose exec db psql -U hireling -d postgres -c \
        "CREATE DATABASE hireling OWNER hireling"
    just db-migrate

# Import the rules corpus from a pinned upstream pack release (epic E4).
# Target database comes from HIRELING_DATABASE_URL. Re-runs are idempotent.
import RELEASE:
    cargo run -- import --release {{ RELEASE }}

# Archive the upstream pack license texts at a pinned release into
# licenses/foundry-pf2e/ (commit the result).
license-archive RELEASE:
    cargo run -- license-archive --release {{ RELEASE }}

# Check the license gate (exit 0 green / 1 red; gates public exposure only).
license-verdict:
    cargo run -- license-verdict

test:
    #!/usr/bin/env bash
    # The schema tests need the throwaway Postgres. Start it when docker is
    # available and nothing is listening yet; otherwise the tests skip
    # loudly (never silently green).
    # --workspace: the engine crate (E8) carries the WEx/property suites —
    # the PRD's top-risk tripwire (FR-10); a root-package workspace's bare
    # `cargo test` would run only the main crate and gate nothing that
    # matters.
    if ! pg_isready -h 127.0.0.1 -p 5432 -q; then
        if command -v docker >/dev/null 2>&1; then
            just db
            for _ in $(seq 1 30); do
                pg_isready -h 127.0.0.1 -p 5432 -q && break
                sleep 1
            done
        else
            echo "WARNING: no Postgres on :5432 and no docker — schema tests will SKIP loudly"
        fi
    fi
    cargo test --quiet --workspace

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

deny:
    cargo deny check

# The engine's boundary (SC-2): `cargo tree` must show the serde family
# only — no axum, sqlx, tokio, or any UI/transport/DB crate may appear in
# the portable core's normal edge set. A new dep that drags the framework
# in fails the gate here, loudly.
boundary:
    deps=$(cargo tree -p hireling-engine --edges normal --charset ascii | tail -n +2 | sed -E 's/^[|` -]+//; s/ v.*//; s/\(\*\)//' | sort -u); \
    echo "$deps" | grep -qv . && true; \
    for banned in axum sqlx tokio tower hyper leptos svelte; do \
        if echo "$deps" | grep -q "^$banned"; then \
            echo "BOUNDARY VIOLATION: hireling-engine depends on $banned"; exit 1; \
        fi; \
    done; \
    echo "boundary ok: $(echo "$deps" | grep -c .) deps, serde-family only"

# Duplicate-key guardrail over tracked JSON (gh#49). JSON is last-wins — a
# duplicated manifest key silently replaces the pin above it and no parser
# (JSON.parse, serde_json, npm) warns. Exact detection via stdlib
# object_pairs_hook; zero dependencies. Fails loudly if python3 or git is
# missing: a guardrail that skips is not a guardrail.
json-keys:
    python3 scripts/check_json_dup_keys.py --self-test
    python3 scripts/check_json_dup_keys.py

# The full local gate. Run this before pushing. Mirrors the grizzly-gate
# image check-for-check: Rust fmt/clippy/tests/cargo-deny, web
# svelte-check/unit tests/build, and — replicated, after three pushes
# went to GitHub red while this recipe said green — the image's
# node:eslint (web-eslint) and scan:semgrep (scan-semgrep) passes. The
# gate image remains the authority; these replicas are calibrated
# against its observed behavior and must move with any digest bump.
ci-local: json-keys fmt-check lint test deny boundary web-check web-test web-build web-eslint scan-semgrep

# Alias — same gate, the name the spec calls it by.
gate: ci-local

# Install the git hooks (once per clone, and once per new worktree is harmless).
hooks:
    ./.githooks/install.sh

# Update local main from origin without leaving the current branch.
sync:
    git fetch --prune origin
    current=$(git rev-parse --abbrev-ref HEAD)
    if [ "$current" = "main" ]; then \
        git pull --ff-only origin main; \
    else \
        git fetch origin main:main; \
    fi

# Merge the current branch into main, push main, and delete the branch.
merge:
    branch=$(git rev-parse --abbrev-ref HEAD)
    if [ "$branch" = "main" ]; then echo "already on main"; exit 1; fi
    git switch main
    git pull --ff-only origin main
    git merge --no-ff "$branch"
    git push origin main
    git branch -d "$branch"
    if git ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then \
        git push origin --delete "$branch"; \
    fi

# Stage all changes, commit with MSG, and push the current branch.
ship msg:
    git add -A
    git commit -m "{{ msg }}"
    git push -u origin HEAD
