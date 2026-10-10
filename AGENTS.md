# AGENTS.md — Hireling

Instructions for any AI coding agent working in this repo. Read `CONSTITUTION.md`
first — it is law. Then `docs/PRD.md` — it is the spec. Rust engineering
conventions (lint policy, test layout, error handling, git workflow) live in
`docs/toolkit-conventions.md` — vendored from Bear's rust-toolkit, enforced by
the pre-commit hooks and CI. They are not advisory.

## What this is

Hireling: a party-linked Pathfinder 2e character tracker. PWA. Pathbuilder JSON
imports, real-time party sync, a modifier engine that does PF2e buff/condition math
with per-number provenance, shared party inventory (P1). Friends-and-family POC for
six users. Full spec: `docs/PRD.md`. UX baseline: `docs/reference/lorum_ipsum_dashboard.html`.

## Stack (settled — do not re-litigate)

- **Backend:** Rust, axum, single binary. WebSocket for party sync; REST for auth/import/bootstrap.
- **Database:** Postgres — a `hireling` hall on the house shared instance (Asgard).
  Migrations via `sqlx migrate`, checked in. Local dev: throwaway postgres in compose.
- **Frontend:** Svelte (Vite), hand-rolled CSS, PWA (service worker + manifest).
  No component library. No SvelteKit.
- **Auth:** Authentik OIDC. No password code in this repo, ever.

## Hard rules

1. **Scope discipline.** The PRD's non-goals are binding: no character builder, no
   combat/round/initiative tracking, no GM tooling beyond read-only, no dice roller,
   no auto-expiry or aura/positioning automation. If a task smells like these, stop
   and flag it — do not build it.
2. **The modifier engine** is pure, isolated, and test-first. Base stats + active
   effects in; derived stats + provenance out. No I/O, no framework imports.
3. **Branches and PRs.** Feature branches with conventional prefixes
   (`feat/`, `fix/`, `refactor/`, `docs/`, `ci/`, `chore/`), referencing the
   issue where one exists (`feat/12-buff-engine`). Never commit to `main`.
   Never force-push shared branches. Open a PR; the author never merges their own
   work. Thrane holds the merge button — merge asks go to Thrane, not to Josh.
4. **Verify before claiming done.** Run the build and the tests. Report actual
   output. If you couldn't verify something, say exactly that.
5. **Write it down.** Decisions with rejected alternatives go in the PR description.
   If you discover the PRD is wrong, say so — don't code around it silently.

## Workflow

- Work comes from **GitHub Issues** on this repo. One issue, one branch, one PR.
  Small PRs win.
- **Board of record.** GitHub is the record of the work; your task tracker
  (Paperclip, whatever harness you run in) is the record of who is doing it.
  Each issue stays canonical — no mirroring, and no second verdict on done
  anywhere else.
- **Claim before you code.** Only start an issue that is *unassigned*, and
  self-assign it on GitHub before writing any code. If a human holds it, it's
  theirs — pick another.
- **Three sync points, nothing continuous.** Pickup: self-assign and comment
  the branch name. PR open: `Closes #N` plus a link back to your tracking task.
  Merge: GitHub closes the issue.
- **Gate yourself before the PR.** Run `just ci-local` (fmt / clippy-deny block /
  tests / cargo-deny) and paste the result. CI runs **grizzly-gate** (standalone
  mode, pinned image) on every PR — a red gate is a red PR, no exceptions. The
  lint/test configs are vendored from Bear's rust-toolkit; if a lint fights you,
  propose tuning it in a PR, don't `#[allow]` around it.
- **Sweep for reachability before you hand over.** Walk the spec's requirement
  list and, for each one, name the production-path test that proves it. Where
  the requirement landed in a library module, grep for a caller:
  `grep -rn "<export_name>" src web/src`. Hits confined to the defining module
  mean the requirement is unbuilt — a green unit test does not change that.
  Report the sweep in the PR body alongside the gate output.
- Priority order is P0 → P1 → P2 as tagged in the PRD's functional requirements.
- Feedback on the PRD itself goes to the PRD (via issue or PR against `docs/PRD.md`),
  not into code comments.

## Style

- Grug-brained (see Constitution Article II). Flat code, locality of behavior,
  name your intermediate variables.
- Integration tests are the sweet spot. The modifier engine gets exhaustive unit
  tests; everything else gets a curated end-to-end suite.
- **A test for a spec'd behavior must exercise the production path that owns
  it.** Testing the module underneath that path proves the module, not the
  behavior. Five instances of the same rule:
  - *Observability* (audit, log, metric): send an HTTP request through the
    configured router for HTTP behavior, or call the public orchestration
    function for non-HTTP behavior; then assert the persisted record or
    captured emission. Calling the recorder or emitter helper directly cannot
    catch a missing call from that production path.
  - *A user-visible affordance*: render it through the component, in the state
    the spec describes. `web/` runs `vitest run` against jsdom (config in
    `web/vite.config.js`); mount with `@testing-library/svelte`'s `render()`
    and assert what the user can see — by role, by text, by placeholder. A
    render that only passes the default or empty value never enters the branch
    the spec is about, so exercise the non-default state.
  - *An affordance that lives in the wiring* — anything riding an `$effect` or
    an event handler: drive it. `fireEvent.input`, `.click`, `.keyDown` against
    the mounted component, then assert the DOM the spec promises. jsdom runs
    effects and handlers, so "the store is correct and unit-tested" and "the
    helper accepts an `onCommit`" are not proof — a store nothing subscribes to
    and a callback nothing passes are unverified however many unit tests they
    carry. The event is what proves the handler is wired.
  - *A field the spec sources from imported data*: prove it with two fixtures
    that differ in that field, asserting two different results. One fixture
    asserting one literal also passes against a hardcoded constant.
  - *A number the game rules fix* (the level a feature is gained at, a DC, a
    dice step): assert both sides of the threshold — `threshold - 1` and
    `threshold` — with different results. One fixture on one side passes
    against every wrong threshold on that side. See
    [Rules exactness](#rules-exactness) for the full rule, citations included.
- Log generously on the backend: major branches, request IDs.
- Commit messages: imperative, one line, what + why if non-obvious.

## Rules exactness

A green gate cannot catch a rules error: no lint, no clippy rule and no test
count distinguishes a correct Pathfinder 2e number from a wrong one. PR #73
proposed a class-feature table wrong in eight of eight rows with `just ci-local`
green and 364 tests passed — review caught it before merge; the gate could not
have. The only guardrails that work are a citation that can be checked and an
assertion that moves when the number moves. Both are required for any constant,
table row, or branch that encodes a rules fact.

The rules corpus is conditions + equipment only (`src/import/model.rs`); there
is no corpus for class features, so class-feature tables and level thresholds
are hand-maintained by necessity. "Derive it from the corpus" is not available
here — which is why the citation rule carries the weight it does.

1. **Cite per row, not per table.** Any constant, table row, or branch that
   encodes a Pathfinder 2e rules fact names the source book and page for *that
   value* in its doc comment. A single citation covering a whole table is not
   acceptable — it certifies rows nobody checked, and a wrong citation is
   worse than none because the next reader stops checking. Where a value's
   book differs from the table's (Magus is *Impossible Magic*, not *Player
   Core*), the row says so. Write the citation off the source page — never
   from memory, and never from a review comment.
2. **A boundary assertion per asserted value.** A test guarding a rules
   threshold asserts both sides of it — at `threshold - 1` and at `threshold`,
   with different results — through the production path that owns the value.
   One fixture on one side of a threshold passes against every wrong threshold
   on that side, so it guards nothing. This is the rules-threshold instance of
   the two-fixture rule in [Style](#style): a sourced number is proven by two
   inputs that differ in it, asserting two different results.

## Humans

- **Josh** — PM, owner. **Dave** — co-dev, owner. Both build agentically.
- **Vex** — Josh's engineering agent (PM/eng on this project).
- Constitution amendments need both owners. Everything else, one owner's PR
  approval suffices.
