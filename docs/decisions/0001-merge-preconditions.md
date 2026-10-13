# 0001 — Merge preconditions: gate at the head, not an up-to-date branch

## Context

The `main-gate` ruleset required the `gate` check with
`strict_required_status_checks_policy: true` — a PR head had to contain the tip of
`main` before the merge button worked. With several agents opening PRs in the same
hour, every merge invalidated every other open PR, and recovery cost a lap:
`gh pr update-branch` (which moves the head), a ~6-minute gate run, and a verdict
carry-over review round, because the head a verdict was bound to had moved. gh#131
paid that lap twice.

The strict policy also does not deliver what it promises. On 2026-10-13, gh#135
merged at 14:30:22Z and gh#131 at 14:30:28Z. gh#131's gate run (head `f80b529`,
started 14:19:49Z) never saw gh#135's merge commit `1f27e98`. GitHub recomputes PR
mergeability asynchronously, so the strict check had not caught up in those six
seconds, and a `BEHIND` head merged anyway. Strict narrows the stale window; it
cannot close it, because any concurrent merge reopens it and the enforcement is
racy at exactly the moment it is supposed to fire.

Nor is strict what puts `main`'s code in the tested tree. `gate` runs
`on: pull_request` with `actions/checkout` and no `ref:`, so it checks out
`refs/pull/N/merge` — the head merged with `main` as of the event. The open
question is never "is `main` in the tested tree", only "how old is that merge".

## Decision

1. `strict_required_status_checks_policy` is `false` on `main-gate`. `gate` stays
   required and must be `success` at the exact head being merged, read off the
   check-runs API rather than a pasted local log.
2. A `BEHIND` head is not a merge blocker by itself. Immediately before the press,
   compare what `main` moved on since the head's gate run started against the PR's
   own diff:

   ```sh
   gh pr diff <n> --name-only                           # the PR's paths
   git diff --name-only <gate-run-base>..origin/main    # what main moved on
   ```

   Disjoint paths → press on green. Overlapping paths → `gh pr update-branch`,
   re-gate, and carry the verdict over under the three conditions in `AGENTS.md`.
   The comparison is two commands and its answer is deterministic, which is two
   properties the strict check did not have.
3. `gate` triggers on `merge_group` as well as `pull_request`, so requiring a
   merge queue on `main` is a ruleset flip and not a workflow change. A queue is
   the only construction that closes the stale window: it builds each entry on the
   one before it, merges only on green, and never moves the PR head — so a verdict
   stays bound to the head that was reviewed and the carry-over rule stops being
   invoked at all.

## Alternatives rejected

- **Keep strict, merge faster.** The laps come from concurrency, not latency. Two
  agents merging in the same hour re-break each other however fast anyone reacts,
  and the six-second race above shows the check is unsound when it matters.
- **Keep strict, serialize merges by convention** — one merge-ready PR at a time.
  A throughput ceiling bought with discipline nobody can verify: a merge queue
  with a human standing in for the queue and no evidence trail.
- **Drop the `gate` requirement too.** The gate is the only thing in this repo
  that certifies a tree at all. It stays required, at the exact head.

## Consequences accepted

- A merge can land a combination of commits no gate run ever saw, when the two
  diffs touch disjoint paths. The `push: branches: [main]` gate run is the
  backstop: a red `main` is the standing maintenance issue's top item and blocks
  every further press until it is green.
- Requiring the merge queue adds a second gate run per merge (`pull_request` plus
  `merge_group`). That buys back the update-branch laps, which cost the same CI
  and a review round on top. The ruleset flip is tracked on gh#140.
