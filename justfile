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

# Frontend unit tests (dependency-free, node's built-in runner).
web-test:
    npm --prefix web test

# Svelte diagnostics over the frontend — the same check the CI gate runs
# as node:svelte-check. Needs `npm --prefix web ci` once per clone.
web-check:
    npm --prefix web run check

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

test:
    #!/usr/bin/env bash
    # The schema tests need the throwaway Postgres. Start it when docker is
    # available and nothing is listening yet; otherwise the tests skip
    # loudly (never silently green).
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
    cargo test --quiet

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

deny:
    cargo deny check

# The full local gate. Run this before pushing. Covers every check this
# repo owns that the grizzly-gate image also runs: Rust fmt/clippy/tests/
# cargo-deny, plus web svelte-check, unit tests, and build. (The gate's
# eslint/tsc and security scans exist only in the pinned image.)
ci-local: fmt-check lint test deny web-check web-test web-build

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
