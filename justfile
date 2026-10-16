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

# Install the frontend dependencies when they are absent or stale. Every
# recipe below that shells a local binary depends on this: `web/node_modules`
# is untracked, so every new worktree starts without it, and the leg dies at
# exit 127 (`svelte-check: not found`) with every later leg unrun — a gate
# that stops six legs in and reports one failure (gh#142).
#
# Absence is not the only rot. A `web/package-lock.json` bump landing in an
# already-installed tree used to sail through: `[ -d web/node_modules ]`
# tests existence, not freshness, so the new dependency was never installed
# and the failure surfaced downstream as a missing module (gh#174). The
# recipe therefore stamps the lockfile's sha256 into
# web/node_modules/.lockfile-sha256 after a successful install and
# reinstalls when the recorded hash no longer matches — a changed lockfile
# reinstalls, a matching tree is a silent no-op. The stamp lives INSIDE
# node_modules so removing the directory always forces a fresh install;
# `rm -rf web/node_modules` stays the manual escape hatch. Needs GNU
# coreutils sha256sum (Linux bench and the gate image both have it).
#
# CI installs deps in its own workflow step before the guard recipes; that
# bare `npm ci` writes no stamp, so the first web-* guard leg there
# reinstalls once to lay it — the price of not teaching gate.yml the stamp
# format (one more thing that could drift), and `npm ci` deletes and
# refetches node_modules wholesale, which is not something a check leg
# should do more than once.
web-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    lock_sha=$(sha256sum web/package-lock.json | cut -d' ' -f1)
    stamp=web/node_modules/.lockfile-sha256
    if [ -d web/node_modules ] && [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$lock_sha" ]; then
        exit 0
    fi
    if [ -d web/node_modules ]; then
        echo "web-deps: web/package-lock.json changed since the last install — reinstalling (npm --prefix web ci)"
    else
        echo "web-deps: web/node_modules absent — installing (npm --prefix web ci)"
    fi
    npm --prefix web ci
    printf '%s\n' "$lock_sha" > "$stamp"

# Build the frontend bundle into web/dist.
web-build: web-deps
    npm --prefix web run build

# Frontend unit tests (vitest + @testing-library/svelte, jsdom — see gh#41).
web-test: web-deps
    npm --prefix web test

# Svelte diagnostics over the frontend — the same check the CI gate runs
# as node:svelte-check.
web-check: web-deps
    npm --prefix web run check

# The gate image's node:tsc pass, replicated bench-side (gh#151). The
# image typechecks web/ with its own TypeScript; the version is a rules
# fact and is cited, not remembered: 6.0.3, read off the pinned digest
# sha256:25c6193d… (registry config blob, ARG TYPESCRIPT_VERSION=6.0.3,
# installed --save-exact). web/package.json pins exactly that, so this
# leg and web-check answer the question the image's node:tsc answers.
# The boundary is real: at f29e85d, tsc 5.9.3 (the old pin) is silent
# where 6.0.3 reports `state.js(398,6): error TS2871` — the same red the
# gate showed. TS2871-class strictness postdates 5.9.3; without this leg
# a tsc-only failure was structurally invisible bench-side.
web-tsc: web-deps
    cd web && ./node_modules/.bin/tsc --noEmit -p tsconfig.json

# The gate image's node:eslint pass, replicated bench-side over the image's
# OWN flat config — not a hand-typed replica. web/gate-eslint.config.mjs.tmpl
# is vendored verbatim from the pinned digest (etc/grizzly-gate/config/
# languages/node/eslint.config.mjs.tmpl) by scripts/fetch_gate_file.py, and
# scripts/check_gate_vendored.py reds when it drifts (`just check-vendored`).
# It is kept as a `.tmpl` — the same shape the image ships — so neither the
# gate's honest-map walk nor node:eslint sees a live `.mjs`; this leg
# materializes it into web/node_modules/ (auto-ignored by ESLint 9) and
# removes it after, reproducing the image's own "copy beside node_modules,
# then delete" contract (ADR-033).
# The image lints with --no-config-lookup and ES builtins only (no browser/
# node env), so platform globals must still be declared in-file
# (/* global */ header; convention: web/src/lib/sync/index.js).
#
# The vendored config reads two runtime knobs from process.env; this leg
# supplies them to reproduce the image's adapter contract (see the config's
# own header, ADR-033):
#   GATE_TSCONFIG  — the repo's tsconfig, absolute (the image passes its
#                    wrapped tsconfig: same strictness — web/tsconfig.json
#                    is strict:true — plus skip_dirs unioned into exclude).
#   GATE_SKIP_DIRS — the image's Ops-owned skip_dirs, hand-copied from the
#                    pinned digest's config/detect.toml (verified 15/15 at
#                    vendoring 2026-10-15); not re-read by any guard — gh#322
#                    files wiring it to the manifest (which needs a raw-byte
#                    vendoring format: non-JS files cannot carry the `//`
#                    provenance header).
# The eslint toolchain in web/package.json (eslint 9.39.4, typescript-eslint
# 8.62.0, react 7.37.5, react-hooks 7.1.1, svelte-plugin 3.20.0, plus
# svelte-eslint-parser 1.8.0 and typescript 6.0.3) matches the image's
# languages/node/package.json dependencies — verified at vendoring
# (2026-10-15), not re-read by any guard; gh#322 files wiring that check.
# svelte (5.57.1) and svelte-check (4.7.6) are the app's own versions, not
# the image's 5.56.4/4.7.1 — they are not part of the eslint toolchain.
#
# One constructional difference from the image, a written exemption: the
# image lints the whole project with -f json; the bench lints the same files
# (`.`) but drops the json formatter so findings are human-readable. The rule
# set, the file set and the env knobs are identical.
web-eslint: web-deps
    #!/usr/bin/env bash
    set -euo pipefail
    cd web
    trap 'rm -f node_modules/eslint.config.mjs' EXIT
    cp gate-eslint.config.mjs.tmpl node_modules/eslint.config.mjs
    GATE_TSCONFIG="$(pwd)/tsconfig.json" \
    GATE_SKIP_DIRS="node_modules,target,vendor,.venv,venv,__pycache__,.mypy_cache,.pytest_cache,.ruff_cache,.cargo,.gradle,.tox,dist,build,.svelte-kit" \
        ./node_modules/.bin/eslint --no-config-lookup --no-error-on-unmatched-pattern --config node_modules/eslint.config.mjs .

# The drift check over the vendored gate config (gh#314): the vendored file
# must still hash to what scripts/gate_vendored.tsv records, and (CI-side,
# --image-source) the pinned digest must still carry those bytes. The self-
# test re-proves every red direction; the real check is the guardrail.
check-vendored *args:
    python3 scripts/check_gate_vendored.py --self-test
    python3 scripts/check_gate_vendored.py {{args}}

# The runner-label drift check (gh#331): the gate workflow's `runs-on:`
# label and the label embedded in the cache key/restore-keys must agree.
# After PR #319 those four literals are hand-kept; a mismatch is a silent
# cache miss, not a red gate. The self-test re-proves every red direction;
# the real check is the guardrail.
check-runner-label:
    python3 scripts/check_runner_label.py --self-test
    python3 scripts/check_runner_label.py

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
web-css-guard: web-deps
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

# The gate image's scan:semgrep pass, mirrored over the repo root with the
# image's OWN rule directory (gh#320). gate-semgrep-rules/ is vendored verbatim
# from the pinned digest's etc/grizzly-gate/config/util/semgrep/rules/ by
# scripts/fetch_gate_file.py (directory mode) and git-ignored; `just
# vendor-semgrep` re-creates it and `just check-vendored` hashes it. The
# invocation below reproduces the image's own scan contract from
# etc/grizzly-gate/config/util/semgrep/manifest.toml: same --exclude set
# (plus gate-semgrep-rules itself, so the vendored rules are not scanned by
# the vendored rules), same --exclude-rule (the check_id is derived from the
# --config path, so the vendored path prefixes it), same --json --error
# --quiet. `--metrics=off` is a bench-only knob (the image runs offline).
# Needs the semgrep CLI on PATH (`pipx install semgrep`) and the vendored
# tree — a scan that skips is not a scan, so a missing CLI or tree fails
# loudly. scripts/semgrep_fail_closed.py then reads the JSON and reds on any
# warn/PartialParsing entry, the blind-spot class gh#332 names.
scan-semgrep:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "$(git rev-parse --show-toplevel)"
    if ! command -v semgrep >/dev/null 2>&1; then
        echo "scan-semgrep: semgrep CLI not found — install it (pipx install semgrep) or run the pinned gate image (CONTRIBUTOR.md)" >&2
        exit 1
    fi
    if [ ! -d gate-semgrep-rules ]; then
        echo "scan-semgrep: gate-semgrep-rules/ not vendored — run 'just vendor-semgrep'" >&2
        exit 1
    fi
    semgrep_json="$(mktemp)"
    trap 'rm -f "$semgrep_json"' EXIT
    set +e
    SEMGREP_SEND_METRICS=off semgrep scan --metrics=off --json --error --quiet \
        --exclude=dist --exclude=build --exclude=.svelte-kit --exclude=gate-semgrep-rules \
        --exclude-rule=gate-semgrep-rules.python.lang.correctness.common-mistakes.string-concat-in-list \
        --config gate-semgrep-rules \
        . > "$semgrep_json"
    rc=$?
    set -e
    if [ "$rc" -ge 2 ]; then
        echo "scan-semgrep: semgrep exited $rc (failed to run, not findings) — see output above" >&2
        exit "$rc"
    fi
    python3 scripts/semgrep_fail_closed.py "$semgrep_json"
    exit "$rc"

# Vendor the image's semgrep rule directory (gh#320) — the same
# scripts/fetch_gate_file.py mechanism that vendors the eslint config, in
# directory mode (trailing slash on IMAGE_PATH). Re-run after a digest bump
# or when check-vendored reds on the directory row's tree digest. The tree
# is git-ignored; `just scan-semgrep` reads it and `just check-vendored`
# hashes it, so it is reproduced by the same authority the gate runs, never
# by hand.
vendor-semgrep:
    python3 scripts/fetch_gate_file.py etc/grizzly-gate/config/util/semgrep/rules/ \
        --output gate-semgrep-rules --manifest scripts/gate_vendored.tsv

# Vite dev server for frontend-only iteration (proxies nothing; use `just dev`
# for the real full-stack path).
web-dev: web-deps
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
    # --report-time (MOR-213): the run log is the only place per-test timing
    # has ever existed — `--quiet` collapses the whole suite into one block
    # and a slow test is invisible in it. libtest's report-time is an
    # unstable flag; RUSTC_BOOTSTRAP unlocks the flag parse on the pinned
    # stable toolchain without changing codegen (no unstable feature is
    # compiled). A toolchain bump that breaks the flag parses fails this leg
    # loudly — never silently drops the timing.
    RUSTC_BOOTSTRAP=1 cargo test --workspace -- --report-time -Z unstable-options

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

# The disk floor for `disk-guard`, overridable three ways (precedence:
# `just disk-floor-gb=<n>` on the command line, then `DISK_FLOOR_GB` in the
# environment, then this 10G default). 10G is ~3x a measured full gate build — a leaked target-gate-*
# dir runs 2.2–3.7G (`du -sh` on this bench) — so the floor leaves headroom
# for one build plus the checkouts, node_modules and git objects that share
# the filesystem, without crying wolf on every bench holding a little
# history. CI runs the same default: the runner logged 75.4G free with the
# gate cache already restored, so no override is needed.
disk-floor-gb := env_var_or_default("DISK_FLOOR_GB", "10")

# Free-space guard: fail fast, before the expensive ci-local legs, when the
# filesystem holding the build output has less than disk-floor-gb free. The
# decision lives in diskops (std-only, zero deps), so it compiles in under
# a second even on a nearly-full disk — the only thing a completely full
# disk cannot do is compile anything at all, and that failure is its own
# message. Fires with the measured number and the remedy (free space, or
# lower the floor via the disk-floor-gb variable).
disk-guard:
    cargo run -q --package diskops -- guard --floor-gib {{ disk-floor-gb }}

# Reap leaked build dirs: sibling target-* dirs (external CARGO_TARGET_DIRs)
# and stale hand-cut hireling-* worktrees that pass the AGENTS.md staleness
# test (branch an ancestor of origin/main AND tree clean). Dry-run by
# default; --prune removes. Hard exclusions are never touched, regardless of
# liveness: MOR-<n> workspaces, dirty trees, and the shared hireling
# checkout. Liveness is decided per candidate from /proc/<pid>/cwd, so a
# permanent npm/node tooling process outside the sibling set does not block
# the sweep (gh#301 round 2 F1).
reap *args:
    cargo run -q --package diskops -- reap {{ args }}

# The guardrail guarding the guardrails (gh#151), the same script the gate
# workflow's drift-check step runs: every ci-local leg has a CI home; every
# pass declared in scripts/gate_passes.tsv is mirrored by a leg this
# justfile actually runs or carries its written exemption; and (CI-only,
# --image-log) the image's own stdout agrees with the declaration in both
# directions — nothing undeclared runs, and every mirrored pass is named
# (gh#163: exempt rows may be skipped without red). repo_side stays
# derived from the workflow's own text — never a hand-kept literal
# (MOR-142, MOR-130 fault class), and the workflow must still carry the
# --image-log step or the bench-side check itself reds (gh#163 F3). A
# guard proven to red is the evidence a parse cannot be: break-tests
# live in the gh#151 PR and the gh#163 self-test.
ci-drift:
    python3 scripts/ci_drift_check.py --self-test
    python3 scripts/ci_drift_check.py

# The full local gate. Run this before pushing. Two kinds of leg:
#
# Image mirrors — the pinned gate image runs the same check. The mapping
# (image pass → leg, or the written reason a pass is exempt) is declared
# once in scripts/gate_passes.tsv — the single repo-side authority; the
# buried `mirrored` literal this used to keep is gone (gh#151). Mirrored
# today: rust:fmt on fmt-check, rust:clippy on lint, rust:test on test,
# rust:deny on deny, node:svelte-check on web-check, node:tsc on web-tsc,
# node:eslint on web-eslint, scan:semgrep on scan-semgrep. The image
# remains the authority over what green means.
#
# Repo-side legs — no image pass runs these; CI gates them as guard steps
# in the gate job of .github/workflows/gate.yml, ahead of the image run:
# disk-guard, json-keys, boundary, web-test, web-css-guard,
# web-html-boundary, check-vendored, check-runner-label, and ci-drift itself. That job's drift-check step runs
# this same ci-drift recipe, so a new leg lands together with its CI home
# or the gate goes red — bench and CI run one check, not two that can drift.
#
# The closing line exists because `just` stops at the first failed
# dependency and this recipe had no body: a run that died six legs in
# printed one leg's failure and nothing about the six that never ran, and
# `just ci-local | tail` reports tail's exit 0 over it (gh#142). The line
# prints only when every leg passed, so a piped run reads green only when
# it is green. Note what it is not: the exit status is still the thing to
# check, and a reader who ignores both is still reading one leg's output.
ci-local: disk-guard json-keys fmt-check lint test deny boundary web-check web-tsc web-test web-css-guard web-html-boundary web-eslint check-vendored check-runner-label scan-semgrep ship-guard ci-drift
    @echo "ci-local: every leg above passed"

# Alias — same gate, the name the spec calls it by.
gate: ci-local

# Install the git hooks (once per clone, and once per new worktree is harmless).
hooks:
    ./.githooks/install.sh

# Update local main from origin without leaving the current branch.
sync:
    git fetch --prune origin
    # One shell for the check: set shell is ["bash", "-cu"], so each recipe
    # line is a separate invocation and an assignment never survives its line.
    current=$(git rev-parse --abbrev-ref HEAD); \
    if [ "$current" = "main" ]; then \
        git pull --ff-only origin main; \
    else \
        git fetch origin main:main; \
    fi

# Stage all changes, commit with MSG, and push the current branch.
# Refuses main: pushing main directly skips the PR gate, and the main-gate
# ruleset's bypass class (repository owners) is exactly who runs this recipe.
ship msg:
    branch=$(git rev-parse --abbrev-ref HEAD); \
    if [ "$branch" = "main" ]; then echo "ship: refusing to push main directly — open a PR" >&2; exit 1; fi
    git add -A
    git commit -m "{{ msg }}"
    git push -u origin HEAD

# Prove the ship self-guard through the production path (gh#101). The guard
# IS the behavior, so a test that does not invoke the recipe proves nothing:
# `just ship` runs against a throwaway `main` checkout whose remote points at
# a scratch bare repo (never origin — if the guard ever stops firing, the
# push lands there, not on the real remote), and asserts exit 1, the refusal
# message, and that the remote stayed empty. The guard fires before any git
# command, so no push is attempted. A guard that cannot fail is not a guard
# (MOR-130): with the guard removed a main-checkout ship commits the dirt
# file and pushes — proven in the gh#101 verification — so all three
# assertions red the moment the guard is gone.
ship-guard:
    #!/usr/bin/env bash
    set -euo pipefail
    scratch=$(mktemp -d)
    bare=$(mktemp -d)
    trap 'rm -rf "$scratch" "$bare"' EXIT
    git init -q -b main "$scratch"
    git init -q --bare "$bare"
    git -C "$scratch" remote add origin "$bare"
    cp justfile "$scratch/justfile"
    git -C "$scratch" add justfile
    git -C "$scratch" -c user.email=ship-guard@example.invalid -c user.name='ship-guard' \
        commit -q -m 'seed justfile'
    printf 'dirt\n' > "$scratch/dirt"
    set +e
    out=$(cd "$scratch" && just ship test 2>&1)
    rc=$?
    set -e
    if [ "$rc" -ne 1 ]; then
        echo "ship-guard: expected exit 1 from a main-checkout ship, got $rc" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
    if ! grep -qF "refusing to push main directly" <<< "$out"; then
        echo "ship-guard: refusal message missing from ship output:" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
    if [ -n "$(git --git-dir="$bare" for-each-ref --format='%(refname)')" ]; then
        echo "ship-guard: the push ran despite the guard — remote refs:" >&2
        git --git-dir="$bare" for-each-ref >&2
        exit 1
    fi
    echo "ship-guard ok: ship refused main (exit 1, message, no push)"
