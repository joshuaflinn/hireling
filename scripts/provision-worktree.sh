#!/usr/bin/env bash
# Provision a freshly created execution worktree.
#
# The harness's builtin provision step runs this file on every worktree it
# provisions: it is resolved from the shared checkout and executed with
# cwd = the new worktree. A non-zero exit FAILS workspace provisioning, and
# this script runs on the critical path of every task start — so it does one
# copy, reports every failure loudly, and always exits 0. A skipped copy costs
# a toolchain re-download; a failed provision costs the whole task start.
#
# What and why: the shared checkout's .claude/settings.json points RUSTUP_HOME
# and CARGO_HOME at the durable toolchain install. `.claude/` is gitignored, so
# a new worktree does not have it, and Claude Code is launched with
# --setting-sources=project,local — it reads <worktree>/.claude only. Without
# this copy every run that touches cargo re-resolves both homes against a
# throwaway HOME and re-downloads the pinned toolchain. See the bench janitor,
# which restores the source file in the shared checkout if it goes missing.
#
# settings.local.json is deliberately NOT copied: the harness writes that file
# itself. No host-absolute paths here: this copies whatever the shared
# checkout holds, it does not restate where the toolchain lives.

set -u
# Deliberately no set -e: every failure below is answered with a loud line and
# exit 0, never with a failed provision.

SRC_BASE="${PAPERCLIP_WORKSPACE_REPO_ROOT:-}"
DEST_DIR="$PWD/.claude"

if [ -z "$SRC_BASE" ]; then
    echo "provision-worktree: PAPERCLIP_WORKSPACE_REPO_ROOT is not set; cannot locate the shared checkout's .claude/settings.json — skipping (runs here will re-resolve the rustup/cargo homes)" >&2
    exit 0
fi

SRC="$SRC_BASE/.claude/settings.json"

if [ ! -f "$SRC" ]; then
    echo "provision-worktree: $SRC is MISSING — runs in this worktree will re-resolve the rustup/cargo homes against a throwaway HOME and re-download the toolchain. Restore the file in the shared checkout (hireling-bench-janitor does this on its cron)." >&2
    exit 0
fi

if [ -e "$DEST_DIR/settings.json" ] && [ "$SRC" -ef "$DEST_DIR/settings.json" ]; then
    echo "provision-worktree: $DEST_DIR/settings.json already is the shared checkout's file; nothing to do"
    exit 0
fi

if ! mkdir -p "$DEST_DIR"; then
    echo "provision-worktree: could not create $DEST_DIR — skipping (runs here will re-resolve the rustup/cargo homes)" >&2
    exit 0
fi

if ! cp "$SRC" "$DEST_DIR/settings.json"; then
    echo "provision-worktree: copying $SRC to $DEST_DIR/settings.json FAILED — skipping (runs here will re-resolve the rustup/cargo homes)" >&2
    exit 0
fi

echo "provision-worktree: copied $SRC -> $DEST_DIR/settings.json"
exit 0
