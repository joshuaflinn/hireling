# 0002 — Merge queue: the update-and-re-test lap is the queue's work

## Context

The `main-gate` ruleset (`24735400`, at version `v53013064`) requires the `gate`
check with `strict_required_status_checks_policy: true` — a PR head must contain
the tip of `main` before the merge button works. That policy has a measured cost
under concurrent merges. As of 2026-10-13 the board held 8 open PRs, 2 `BEHIND`,
1 `BLOCKED`: every merge invalidates every other open PR, and recovery is a
human lap — `gh pr update-branch` (which moves the head), a ~6-minute gate run,
and a verdict carry-over review round, because the head a verdict was bound to
has moved. gh#131 paid that lap twice in one afternoon.

The cost is structural, not a latency problem. While more than one PR is open,
any landing re-opens strict's requirement for every other PR; reacting faster
does not change that, it only charges the lap sooner.

## Decision

1. `gate` triggers on `merge_group` as well as `pull_request` and
   `push: branches: [main]`, so enabling the merge queue on `main` is a ruleset
   flip and not a workflow change. The ruleset flip is tracked on gh#140 and is
   Thrane's hand, after one `merge_group` run is confirmed green.
2. `main` then requires the merge queue, so the update-and-re-test lap becomes
   the queue's work, not a human's: the queue builds each entry on the one
   before it, merges only on green, and never moves the PR head — a verdict
   stays bound to the head that was reviewed, and the carry-over rule stops
   being invoked at all.
3. `strict_required_status_checks_policy` stays `true`. Citation of record:
   ruleset `24735400` version `v53013064` — a version, not a live reading. The
   queue is what makes strict's lap cost acceptable: serialized groups mean a
   head is not invalidated mid-review by a concurrent landing, so the lap the
   policy charges stops being charged to a human.

### Ordering: the trigger lands before the queue is required

A queue entry publishes no `pull_request` event. Requiring the merge queue on
`main` before `merge_group` triggers `gate` parks every entry forever on a
check that never runs. The trigger lands in this PR; the ruleset flip is
gh#140, after one `merge_group` run is confirmed green.

## Alternatives rejected

- **Drop strict** (`strict_required_status_checks_policy: false`) — rejected by
  ruling (MOR-272): strict stays `true`. The evidence that argued for dropping
  it — a `BEHIND` head merging "with strict enabled" — was produced while the
  flag was flipped `false` (versions `v53011491` through `v53013064`): both
  merges cited happened inside that window, when there was no check to be
  stale. The measured lap cost above is real and survives; the remedy for it is
  the queue, not the removal.
- **Serialize merges by convention** (one merge-ready PR at a time, a human in
  the queue's seat) — rejected. A throughput ceiling bought with discipline
  nobody can verify: it has no evidence trail and can land a combination no
  gate run saw. The queue is the same serialization with the machine holding
  the order.

## Consequences accepted

- Two gate runs per merge (`pull_request` plus `merge_group`). That buys back
  the update-branch laps, which cost the same CI plus a review round on top.
- The queue's own re-test is the strict check's enforcement: an entry that goes
  stale while queued is rebuilt and re-gated by the queue automatically, not
  bounced to a human.
