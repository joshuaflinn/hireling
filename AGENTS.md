# AGENTS.md — Hireling

Instructions for any AI coding agent working in this repo. Read `CONSTITUTION.md`
first — it is law. Then `docs/PRD.md` — it is the spec. Rust engineering
conventions (lint policy, test layout, error handling, git workflow) live in
`docs/toolkit-conventions.md` — vendored from Bear's rust-toolkit, enforced by
the pre-commit hooks and CI. They are not advisory. Where this file and
`docs/toolkit-conventions.md` disagree, this file wins: the vendored conventions
govern Rust engineering practice — lint policy, test layout, error handling —
not this repo's branch, review, and merge workflow, which Hard rule 3 and
*Workflow* below own outright.

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
3. **Branches and PRs.** Inside a harness-provisioned workspace, the
   provisioned branch (`MOR-<n>`) is the PR branch — it is tied to its task
   1:1, so do not cut a prefixed branch over it. One exception: the branch
   is not yours — it is another open PR's head, or it carries another
   task's commits — see the collision rule in *Workflow*. The
   conventional prefixes
   (`feat/`, `fix/`, `refactor/`, `docs/`, `ci/`, `chore/`), referencing the
   issue where one exists (`feat/12-buff-engine`), govern branches cut by
   hand outside a provisioned workspace — a human in a clone, a hotfix off
   the shared checkout. Never commit to `main`.
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
- **Review of record.** The verdict lands on GitHub too, not only on the
  board. Before the merge ask, the reviewer posts the verdict on the PR as a
  review comment (`gh pr review <n> --comment -F -`, verdict on stdin):
  accepted or rejected, a link to the review task, and the head SHA the
  verdict is bound to. GitHub bars approving your own PR and every agent
  pushes as one shared identity — so the record is a review comment, and
  required approvals on `main` stay at 0 for the same reason. A merged PR
  with an empty review timeline is a missing record: restore it before the
  next merge ask.
- **Claim before you code.** Only start an issue that is *unassigned*, and
  self-assign it on GitHub before writing any code. If a human holds it, it's
  theirs — pick another.
- **Work in your provisioned workspace.** The harness provisions every run a
  workspace: `<project>/MOR-<n>`, a sibling worktree on branch `MOR-<n>` cut
  from `origin/main`. That branch is the PR branch; the run's own harness
  prompt forbids switching or re-pointing it, so keep all commits on it.
  Never cut your own worktree for your own task — verify where you are with
  `git rev-parse --show-toplevel` and work there — with one exception, a
  branch collision. The keep-all-commits instruction carries a premise: the
  provisioned branch is *this* task's PR branch, 1:1. Check the premise at
  pickup (`git branch --show-current`, then `gh pr list --head <branch>
  --state open`): if the branch is already another open PR's head, or it
  carries another task's commits, the premise has failed — committing would
  move a review head that a verdict may be bound to. Do not commit to it;
  hand-cut a sibling worktree `hireling-<n>-<slug>` on a prefixed branch
  (`docs/<n>-<slug>`-style, Hard rule 3) cut fresh from `origin/main` —
  never a `MOR-<n>` name, that namespace is harness-owned and carries the
  stricter removal rule — note the collision and its evidence in the PR
  body, and report it on your tracking task. It is a provisioning defect,
  not an engineer error. The shared `hireling`
  checkout is reference only: read from it, never run in it, and let it
  drift behind `origin/main`. Hand-cut worktrees — a human in a clone, a
  hotfix, a branch collision — go in sibling directories, never run scratch
  dirs (those get cleaned). Removal has a hard edge: a sibling named for an
  *open* task
  (`MOR-<n>`) can be clean and an ancestor of `origin/main` and still be a
  live harness-owned workspace — never remove one unless that task is
  closed. Hand-cut `hireling-*` siblings keep the old staleness test:
  branch an ancestor of `origin/main` and tree clean means merged, nothing
  in it can be lost, and any agent may remove such a sibling on sight.
  Never `worktree remove --force`; a dirty tree belongs to its owner.
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
