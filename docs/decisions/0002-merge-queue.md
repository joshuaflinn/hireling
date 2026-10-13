# 0002 — Merge queue: unavailable on a user-owned repository; carry-over pays the lap

## Context

The `main-gate` ruleset (`24735400`, at version `v53013064`) requires the `gate`
check with `strict_required_status_checks_policy: true` — a PR head must contain
the tip of `main` before the merge button works. That policy has a measured cost
under concurrent merges. As of 2026-10-13 the board held 8 open PRs, 2 `BEHIND`,
1 `BLOCKED`: every merge invalidates every other open PR, and recovery is a
lap — `gh pr update-branch` (which moves the head), a gate run (measured 5m19s
to 6m58s across the eight runs of 2026-10-13 17:46–18:20 UTC), and a verdict
carry-over round, because the head a verdict was bound to has moved. gh#131
paid that lap on 2026-10-13 — one head move (to `f80b529`), one gate run, one
carry-over round.

The cost is structural, not a latency problem. While more than one PR is open,
any landing re-opens strict's requirement for every other PR; reacting faster
does not change that, it only charges the lap sooner.

GitHub's merge queue is the off-the-shelf remedy: it builds each entry on the
one before it, merges only on green, and never moves the PR head. It is not
available here. `joshuaflinn/hireling` is owned by a personal account
(`owner.type: "User"`), and GitHub grants the merge queue only to
organization-owned repositories — public repositories under an organization, or
private ones under an organization on GitHub Enterprise Cloud. A personal
account is outside both arms of the grant, and no setting enables it.

Evidence, gathered 2026-10-13 (gh#140):

- `PUT /repos/joshuaflinn/hireling/rulesets/24735400` carrying a `merge_queue`
  rule returns `422 Validation Failed`, `"Invalid rule 'merge_queue': "`, for
  every parameter set tried — including GitHub's own documented defaults.
- The rejection is not a payload defect. The same `parameters` object validates
  against `repository-rule-merge-queue` in GitHub's published OpenAPI
  description, and a control ruleset carrying `{"type": "deletion"}` posted and
  deleted cleanly over the same path with the same credentials
  (ruleset `25019747`).
- `repository(owner: "joshuaflinn", name: "hireling") { mergeQueue(branch: "main") }`
  returns `null` with `viewerCanAdminister: true`.

## Decision

1. `gate` triggers on `merge_group` as well as `pull_request` and
   `push: branches: [main]`. The trigger is inert today — GitHub publishes
   `merge_group` only to a repository that can require a queue — and it stays
   so that enabling the queue is one ruleset call if ownership ever moves to an
   organization.
2. `main` does not require a merge queue, because it cannot. Strict's lap is
   paid by the *Verdict carry-over* rule in `AGENTS.md`: a head moved only by a
   clean merge of `origin/main`, with every reviewed blob byte-identical and
   `gate` green at the new head, re-binds the existing verdict without a new
   review round. That removes the review round from the lap; the gate run
   remains.
3. `strict_required_status_checks_policy` stays `true`. Citation of record:
   ruleset `24735400` version `v53013064` — a version, not a live reading.
4. Moving the repository to a GitHub organization is the only path to the queue,
   and it is not a CI decision — it re-grants access for every collaborator and
   every integration, so it belongs to the owners. Recorded on gh#140.

### The queue configuration, decided and parked

Settled so the flip is a single API call and not a fresh round of deliberation.
Each choice carries the alternative it beat.

| Parameter | Value | Rejected alternative |
| --- | --- | --- |
| `merge_method` | `MERGE` | `SQUASH` — `main`'s history is merge commits (`fcef915`, `9253253`); a queue on `SQUASH` would silently split the history into two shapes at the flip. |
| `grouping_strategy` | `ALLGREEN` | `HEADGREEN` — gates only the group's head commit, so an entry whose own gate run failed can land inside a green group. That is the gate requirement with a hole in it. |
| `max_entries_to_build` | `5` | `1` — pure serial, no speculative builds: every entry waits a full gate run after the one before, which gives back most of what the queue buys. `20`+ — speculative CI for a queue that at six users will rarely hold two entries. |
| `min_entries_to_merge` | `1` | `2` with a wait window — batches two merges onto one gate run, but at this concurrency the queue usually holds exactly one entry, so the wait would be pure added latency on every merge for a batch that never forms. |
| `max_entries_to_merge` | `5` | `1` — would land one PR per group even with several green entries ready, re-serializing what the speculative builds already paid for. |
| `min_entries_to_merge_wait_minutes` | `0` | `5` (GitHub's default) — a flat five-minute tax on every merge to wait for a second entry that this repo's merge rate will not produce. |
| `check_response_timeout_minutes` | `30` | `60` (GitHub's default) — the queue is serial, so a hung gate parks everything behind it for the full window; 30 halves that. `15` — only ~2× the measured 6m58s worst case, which a cold image cache plus runner backlog could trip, and a timeout ejects the entry for a human to re-queue. |

## Alternatives rejected

- **Drop strict** (`strict_required_status_checks_policy: false`) — rejected by
  ruling (MOR-272): strict stays `true`. The evidence that argued for dropping
  it — `BEHIND` heads merging while strict was believed on — was produced while
  the flag was off: on ruleset `24735400`, version `v53011491` set
  `strict_required_status_checks_policy: false` at 2026-10-13 10:30:15 EDT and
  version `v53013064` restored `true` at 10:41:10 EDT. gh#135 (merged
  10:30:22 EDT, `1f27e98`) and gh#131 (merged 10:30:28 EDT, `4f4d652`) both
  landed inside that window — with no check to be stale. The measured lap cost
  above is real and survives; the remedy for it is carry-over, not the removal.
- **Serialize merges by convention** (one merge-ready PR at a time, a human in
  the queue's seat) — rejected. A throughput ceiling bought with discipline
  nobody can verify: it has no evidence trail and can land a combination no
  gate run saw.
- **Build a merge queue in Actions** (a workflow that holds the order, rebases,
  re-gates and presses) — rejected for a six-user POC. It is a scheduler with
  persistent state, and its failure mode is a stuck lane nobody is watching.
  The carry-over rule costs one gate run and no new machinery.

## Consequences accepted

- Strict's gate run per concurrent merge is still charged. Carry-over removes
  the review round from that lap, not the CI.
- The `merge_group` trigger never fires while the repository is user-owned. It
  is one line and it is the whole difference between a ruleset call and a
  workflow change later, so it stays.
- *Verdict carry-over* is load-bearing rather than a rarely-invoked escape
  hatch. Its three conditions are the thing standing between a moved head and
  an unreviewed merge, so they get checked literally every time.
