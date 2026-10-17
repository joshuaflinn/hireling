# Plan: give the gh#332 blind-spot guard a CI home (gh#369)

**Task**: MOR-787 · **Issue**: gh#369 · **Branch**: `ci/369-semgrep-fail-closed-ci-home`,
cut fresh from `origin/main` at `f0947b6`.
**Found by**: Orsik, reviewing PR #368 (gh#360 / MOR-785).

## The defect, stated plainly

`scripts/semgrep_fail_closed.py` is the guard gh#332 exists for: semgrep records a
target it could not parse as an `errors[]` entry and exits 0 with `"results": []`, so a
rule that never read its file is indistinguishable from a rule that read it and found
nothing. This guard reads the same JSON and reds on any non-`info` level.

It has never run in CI. `git grep -n semgrep_fail_closed .github/` returns nothing. Its
only caller is the `scan-semgrep` justfile recipe (`justfile:227`, added by PR #368),
and `scan-semgrep` has **no `run: just` step in `gate.yml`** — its CI home is the
image's own `scan:semgrep` pass, which runs its own scan and never invokes this script.
So the guard executes on a bench and nowhere else. At the time gh#369 was filed, its
`--self-test` did not run on a bench either; PR #368 wired that in, which is why this
card unblocked.

The second half is worse than the first. `ci_drift_check.py` direction A accepts a leg
that has *either* an image mirror *or* a repo-side guard step. `scan-semgrep` has a
mirror row, so direction A printed `all 18 ci-local legs are gated` over a tree where
the guard named above ran nowhere. The check that exists to catch an ungated leg
certified the hole instead of finding it.

## Slice A — the shipping half

1. A `gate.yml` step after `Run grizzly-gate` that runs the fail-closed guard over the
   image's own captured stdout (`$RUNNER_TEMP/gate-image-output.log`), **plus** its
   `--self-test`, so both the guard and its proof-of-red execute on every PR.
2. `scripts/gate_passes.tsv:27` and the `check-vendored` preamble (`gate.yml:252-256`)
   lose the "does not run in CI at all" claim. A doc line contradicting the shipped
   workflow is the next reader's trap.

## Slice B — the interesting half

Direction A must not be able to call a leg fully gated when its only home is a mirror
that runs a *different* pass than the recipe the leg carries. Design and self-test.

## Declined alternatives

- **A second repo-side semgrep scan in CI** (Korrin's standing caution): duplicates the
  image's pass and doubles the gate's slowest leg. Not built. The guard needs the
  image's JSON, which is already captured in `$RUNNER_TEMP/gate-image-output.log` and
  already read by `ci_drift_check.py --image-log` — a second reader of an existing
  artifact, not a second scan.
- **A `run: just scan-semgrep` step in CI.** Rejected: that *is* the full second scan.
- **Leaving direction A alone.** Rejected: Korrin's item 2. A fix that leaves the
  false-green reading intact lets the next blind spot ship the same way.
