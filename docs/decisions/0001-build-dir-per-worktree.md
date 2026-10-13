# 0001 — One cargo build directory per worktree

## Context

The bench runs many worktrees of this repo side by side, and several seats
compile at once. One shared `CARGO_TARGET_DIR` across all of them is cheaper on
disk and makes `just ci-local` non-deterministic.

Cargo's exclusive lock on the build directory covers one invocation, not a gate
run: it is released between legs and again while test binaries execute. So a
second seat's cargo can rebuild the dependency graph underneath a rustdoc
doctest that is still holding `--extern` paths into `debug/deps`. Observed:

```
   Doc-tests hireling
error[E0463]: can't find crate for `sqlx`
 --> src/auth/account.rs:5:5
error: doctest failed, to rerun pass `-p hireling --doc`
```

with `--extern sqlx=.../debug/deps/libsqlx-0f8129b2cbb9e138.rlib` naming an
rlib that no longer existed, and `Blocking waiting for file lock on build
directory` earlier in the same run. The code under test was fine — the same
tree went green with a private target directory.

`just ci-local` green is the handover standard (AGENTS.md, *Gate yourself
before the PR*), and the evidence standard is to paste the output. A gate whose
output depends on who else is compiling certifies nothing, and it fails in both
directions: a seat can paste a red gate it did not cause, or the same
contention can let one seat's artifacts satisfy another seat's build.

## Decision

Each worktree builds into its own `./target`, which `.gitignore` already
covers. The bench-local `.cargo/config.toml` that sits one level above the
worktrees sets no `target-dir` and sets `incremental = false`.

This is bench configuration, outside the repo. CI is unaffected — the gate
container points `CARGO_TARGET_DIR` into its own mounted volume.

## Alternatives rejected

- **Serialize cargo across seats** behind a bench-wide lock. Turns parallel
  review seats into a queue; one cold gate blocks every other seat for its
  whole duration.
- **Keep the shared directory and retry on `E0463`.** Treats the symptom. The
  same contention can also make one seat's artifacts satisfy another's build,
  which is a false green — worse than the false red, because nothing surfaces
  it.
- **`sccache` with a size-capped shared cache** — deferred, not rejected. A
  cache of immutable compilation units is safe to share in a way a mutable
  build directory is not. This is the answer if cold-build wait becomes the
  complaint; a return to one target directory is not.

## Consequences accepted

- Disk cost scales with live seats instead of being fixed. A worktree that has
  run a full gate settles at 2.2G (`du -sh target`, measured across four live
  seats on 2026-10-13, all four identical). `incremental = false` is what keeps
  that number small: incremental artifacts are the largest single component of
  a debug tree and they buy nothing for a full gate run from a clean tree,
  which is what these seats mostly do.
- A new or freshly reaped worktree pays a cold build.
- Reaping worktrees is load-bearing, not housekeeping: merged-and-clean
  hand-cut `hireling-*` siblings on sight, `MOR-<n>` siblings once their task
  is closed (AGENTS.md, *Workflow*). Per-seat cost scales with live seats, so
  stale seats are the thing that breaches the disk floor.
