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

# The prose boundary (E9, contracts/inert-html.md §3): `{@html}` is banned
# for prose repo-wide — every markup-carrying string renders through the one
# inert setter (util/inert-html.js adoptHTML) inside ConditionTip/AboutView.
# Same discipline as the engine `boundary` recipe: a loud grep gate, not an
# unaudited convention.
web-html-boundary:
    node web/scripts/check-html-boundary.mjs

# The compiler is the only thing that can see a scoped selector die: nodes
# minted in JS and adopted into the tree carry no svelte-<hash>, so
# `.badge.svelte-xxxx` never matches and vite-plugin-svelte strips the rule —
# shipped markup, unstyled, every test still green (MOR-124 F10, the
# css_unused_selector trap). Runs the build and fails if that signature
# fires; green means zero unused-selector warnings, not exit 0.
web-css-guard:
    #!/usr/bin/env bash
    log=$(mktemp)
    trap 'rm -f "$log"' EXIT
    if ! npm --prefix web run build >"$log" 2>&1; then
        cat "$log"
        echo "web-css-guard: the web build failed" >&2
        exit 1
    fi
    if grep -Ein "unused css selector|css_unused_selector" "$log"; then
        echo "web-css-guard: a scoped selector died in the bundle — wrap adopted-node selectors in .pop :global(...) (MOR-124 F10)" >&2
        exit 1
    fi
    echo "web-css-guard ok: no unused-selector warnings in the build"

# The gate image's scan:semgrep pass, replicated bench-side over the web
# tree with the registry rules it has enforced there (calibrated by fire
# against runs 37850972131 and 37981907115: missing-template-string-
# indicator on the emitted-worker template; package-dependencies-check
# on web/package.json — exact versions only, which the tree already
# kept; and 38080211231: html-in-template-string, the run that red-flagged
# the vetted anchor literal in inert-html.js while ci-local was green).
# The image's full rule set is wider; a digest bump that moves it
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
        --config https://semgrep.dev/r/javascript.lang.security.html-in-template-string \
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
#
# A guard that cannot fail is not a guard (MOR-130, Thrane's finding on
# #93: dead cargo — even rustup with no default toolchain — produced
# "boundary ok: 0 deps", exit 0, because the substitution discarded
# cargo's status and the trailing echo owned the recipe). pipefail is the
# fix: without it the pipeline's status is sed's, and cargo's death dies
# in the substitution. An empty dep set is refused for the same reason —
# it is cargo failing quietly, not a clean core.
#
# The ban is separator-anchored (^axum(-|_|$)) per Orsik's F4 on #93: the
# stacks arrive as family crates — axum-core, tower-service, tokio-util —
# and a whole-line match passes them green, which is the violation the
# guard exists for. Bare prefix is wrong the other way: svelteish and
# tokiotest are not the framework.
boundary:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! deps=$(cargo tree -p hireling-engine --edges normal --charset ascii | tail -n +2 | sed -E 's/^[|` -]+//; s/ v.*//; s/\(\*\)//' | sort -u); then
        echo "boundary: cargo tree exited non-zero — the engine dependency gate cannot run" >&2
        exit 1
    fi
    if [ -z "$deps" ]; then
        echo "boundary: cargo resolved zero dependencies — cargo is broken or hireling-engine moved; refusing to pass" >&2
        exit 1
    fi
    for banned in axum sqlx tokio tower hyper leptos svelte; do
        if grep -qE "^${banned}(-|_|$)" <<< "$deps"; then
            echo "BOUNDARY VIOLATION: hireling-engine depends on $banned" >&2
            exit 1
        fi
    done
    echo "boundary ok: $(wc -l <<< "$deps" | tr -d ' ') deps, serde-family only"

# Duplicate-key guardrail over tracked JSON (gh#49). JSON is last-wins — a
# duplicated manifest key silently replaces the pin above it and no parser
# (JSON.parse, serde_json, npm) warns. Exact detection via stdlib
# object_pairs_hook; zero dependencies. Fails loudly if python3 or git is
# missing: a guardrail that skips is not a guardrail.
json-keys:
    python3 scripts/check_json_dup_keys.py --self-test
    python3 scripts/check_json_dup_keys.py

# The full local gate. Run this before pushing. Two kinds of leg:
#
# Image mirrors — the pinned gate image runs the same check (rust:fmt on
# fmt-check, rust:clippy on lint, rust:test on test, rust:deny on deny,
# node:svelte-check on web-check, node:eslint on web-eslint, scan:semgrep
# on scan-semgrep). Replicated bench-side, after three pushes went to
# GitHub red while this recipe said green; calibrated against observed
# gate behavior and must move with any digest bump. The image remains
# the authority.
#
# Repo-side legs — no image pass runs these; CI gates them as guard steps
# in the gate job of .github/workflows/gate.yml, ahead of the image run:
# json-keys, boundary, web-test, web-css-guard, web-html-boundary. That
# job's drift-check step fails any ci-local leg no CI job runs, so a new
# leg lands together with its CI home.
ci-local: json-keys fmt-check lint test deny boundary web-check web-test web-css-guard web-html-boundary web-eslint scan-semgrep

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

# Stage all changes, commit with MSG, and push the current branch.
ship msg:
    git add -A
    git commit -m "{{ msg }}"
    git push -u origin HEAD
