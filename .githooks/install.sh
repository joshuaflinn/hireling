#!/usr/bin/env bash
#
# Symlink the tracked hooks into this repo's hooks directory.
#
# Symlinks rather than copies so an edit to a hook takes effect immediately and
# stays under version control — a copied hook silently drifts from the one in
# the repo, which is the worst of both worlds.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

# Ask git for the hooks directory rather than assuming `$REPO_ROOT/.git` is one.
# Inside a linked worktree `.git` is a *file* pointing at the real gitdir, so the
# assumption fails outright — and hooks live in the common dir shared by every
# worktree regardless.
GIT_COMMON_DIR="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --git-common-dir)"
HOOK_DIR="$GIT_COMMON_DIR/hooks"
mkdir -p "$HOOK_DIR"

# Link from the MAIN worktree's .githooks, never from $SCRIPT_DIR.
#
# One hooks dir is shared by every worktree, so the link target has to outlive
# any single one of them. Installing from a throwaway worktree pins every
# agent's commit gating to that worktree: edit its .githooks and you have
# changed the hook for the whole repo with no review, and `git worktree remove`
# dangles the symlink — at which point git treats the hook as absent and every
# commit silently stops being gated. A guardrail whose removal is indetectable
# is worse than none. The main worktree is the one path that is always present,
# so resolve it from git instead of trusting the caller's cwd.
MAIN_WORKTREE="$(git -C "$REPO_ROOT" worktree list --porcelain | sed -n '1s/^worktree //p')"
HOOK_SRC="$MAIN_WORKTREE/.githooks"
[ -d "$HOOK_SRC" ] || {
    echo "ERROR: no .githooks in the main worktree ($MAIN_WORKTREE)" >&2
    exit 1
}

for hook in pre-commit commit-msg; do
    chmod +x "$HOOK_SRC/$hook"
    ln -sfn "$HOOK_SRC/$hook" "$HOOK_DIR/$hook"
    echo "installed $hook -> $HOOK_SRC/$hook"
done

# core.hooksPath, if set, wins over .git/hooks and would make the symlinks above
# dead weight. Clear it so there is exactly one place hooks come from.
git -C "$REPO_ROOT" config --unset core.hooksPath 2>/dev/null || true

echo "Hooks installed. Bypassing them with --no-verify is forbidden."
