#!/usr/bin/env python3
"""check-git-worktree: the checkout this build runs in is a working tree (gh#377, MOR-811).

The class fault this closes: `core.bare = true` was set on the project base
checkout twice in four hours. Every harness lane provisions with `git worktree
add`, and git refuses that command the moment the parent repo reports
`--is-inside-work-tree` is `false` — so five runs died and two seats errored,
each burning a worktree and a build pass to a flag nobody could see. Thrane
cleared the flag by hand three times before the watch was moved from the flag
to the guard.

Scope is the bleeding, not the cause: this guard reds *before* a lane dies. It
does not hunt what writes `core.bare` — that investigation is a separate watch
and stays on MOR-211.

Two assertions, both reading the **value** of the config, never the file:

1. `git rev-parse --is-inside-work-tree` prints `true`;
2. `git config --get core.bare` is *not* `true`.

Why the value and not `.git/config`'s mtime (the trap MOR-211 records): mtime
is rewritten by every `git fetch` and every branch checkout, so it carries no
signal at all here — a fetch refreshes it while the flag is broken, and a hand
edit followed by any git command buries the one event worth seeing. The mtime
oracle cannot distinguish "cleaned up" from "never broken"; the value reads the
state that actually breaks `git worktree add`.

Both assertions are fail-closed (Article IV: a guard that cannot fail is not a
guard, MOR-130). A missing git, a repo that is not a git repository at all, a
git whose output is unparseable, a non-repository cwd and an unreadable cwd all
red. None of them green: an unreadable question is never answered as "fine".
`core.bare` is deliberately *not* case-folded — the false forms git itself
accepts (`False`, `OFF`, `no`) all still resolve `--is-inside-work-tree` to
`true`, so the second assertion is an honest *witness* for the operator and the
first stays the authority. `--self-test` re-proves every red and green
direction, including both directions of the real defect, against throwaway
`git init` fixtures. The fixtures are what make the self-test honest: the
production check runs in a cwd the test does not own, and no case ever toggles
the flag on the real base checkout.

Zero dependencies, stdlib only — same discipline as scripts/check_json_dup_keys.py,
scripts/check_runner_label.py and scripts/ci_drift_check.py.
"""

# ruff: noqa: T201, S607 -- a CLI guardrail: printing its verdict is the
# product; the self-test spawns this very script with fixed argv and no shell,
# and resolves git by name on PATH exactly as a build recipe would.

import argparse
import os
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterable
from pathlib import Path
from typing import NoReturn

GIT = "git"

# What `--is-inside-work-tree` must print for the tree to be usable.
INSIDE_WORK_TREE = "true"
# The one `core.bare` value that breaks `git worktree add`. Deliberately exact:
# see the module docstring for why the other truthy spellings stay witnesses.
BARE_TRUE = "true"


def fail(message: str) -> NoReturn:
    """Print the verdict to stderr and exit 1 — the only red this guard knows."""
    print(f"worktree guard: {message}", file=sys.stderr)
    sys.exit(1)


def run_git(cwd: Path, args: list[str]) -> tuple[int, str, str]:
    """Run git with a cleaned environment, returning (exit, stdout, stderr).

    Never raises: every git failure is a verdict this guard has to be able to
    print. The environment is stripped of `GIT_DIR`, `GIT_WORK_TREE` and
    `GIT_CEILING_DIRECTORIES` so the answer describes the checkout on disk.
    An inherited `GIT_DIR` (a harness setting it for its own purposes) would
    otherwise let a broken or absent directory answer for the repo that is
    actually about to be built in — the same wrong-question fault class as
    reading check-runs at a superseded head.
    """
    env = {k: v for k, v in os.environ.items() if k not in {"GIT_DIR", "GIT_WORK_TREE", "GIT_CEILING_DIRECTORIES"}}
    try:
        proc = subprocess.run(  # noqa: S603 -- fixed argv, no shell, cwd is an explicit Path
            [GIT, *args],
            cwd=str(cwd),
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as exc:
        fail(f"{GIT} could not be run ({exc}) — the guard cannot certify a build it cannot inspect")
    return proc.returncode, proc.stdout.strip(), proc.stderr.strip()


def inside_work_tree(cwd: Path) -> bool:
    """Assertion 1: does git report a working tree here? Red on anything else."""
    exit_code, out, err = run_git(cwd, ["rev-parse", "--is-inside-work-tree"])
    if exit_code != 0:
        fail(
            f"`git rev-parse --is-inside-work-tree` exited {exit_code} in {cwd} —"
            " cannot certify this checkout as a working tree"
            + (f": {err}" if err else "")
            + " — a directory that is not a git repository at all fails here too"
        )
    if out != INSIDE_WORK_TREE:
        fail(
            f"`git rev-parse --is-inside-work-tree` printed {out!r}, not"
            f" {INSIDE_WORK_TREE!r}, in {cwd} — `git worktree add` refuses this"
            " checkout, so every lane provisioned from it dies before it builds"
            f" (check `git config --get core.bare` in {cwd})"
        )
    return True


def core_bare_value(cwd: Path) -> str | None:
    """Read `core.bare` as git resolves it. None means unset (implicit false)."""
    exit_code, out, err = run_git(cwd, ["config", "--get", "core.bare"])
    if exit_code == 1 and not out:
        # exit 1 with no output is git's "no such key" — not an error here.
        return None
    if exit_code not in (0, 1):
        fail(
            f"`git config --get core.bare` exited {exit_code} in {cwd} —"
            f" cannot read the bare flag"
            + (f": {err}" if err else "")
        )
    return out


def assert_not_bare(cwd: Path) -> None:
    """Assertion 2: `core.bare` is not `true`. Unset is fine; anything else warns."""
    value = core_bare_value(cwd)
    if value is None:
        return
    if value == BARE_TRUE:
        fail(
            f"`git config --get core.bare` is {BARE_TRUE!r} in {cwd} — this"
            " checkout is marked bare, `git worktree add` refuses it, and every"
            " lane provisioned from it dies at checkout. Clear it with"
            f" `git -C {cwd} config --unset core.bare`."
        )


def check(cwd: Path) -> None:
    """Run both assertions over cwd, then print the green verdict."""
    inside_work_tree(cwd)
    assert_not_bare(cwd)
    print(f"worktree guard ok: {cwd} is a working tree (core.bare is not {BARE_TRUE!r})")


# ---------------------------------------------------------------------------
# self-test — throwaway fixtures only; never touches the real checkout
# ---------------------------------------------------------------------------
#
# Two kinds of case, and the split is the point. The red direction needs a
# checked-out repo whose flag is broken, because `--is-inside-work-tree` is
# what the lane dies on: for that fixture both `rev-parse --is-inside-work-tree`
# and `rev-parse --is-bare-repository` must read `true`, which only a repo with
# a real index and working directory can show. A bare `git init --bare` has no
# working tree to be inside of and reports `false` for the first question, so
# proving the defect against it would prove nothing about the failure the guard
# exists for. `git config core.bare true` over a normal `git init` reproduces
# the symptom exactly as it was observed on the base checkout.

def make_repo(root: Path, name: str) -> Path:
    """A throwaway `git init` with a commit — the fixture every case builds on."""
    repo = root / name
    repo.mkdir(parents=True)
    subprocess.run(  # noqa: S603, S607 -- fixed argv, no shell, inside a private tempdir
        [GIT, "init", "-q", "-b", "main", str(repo)],
        capture_output=True,
        text=True,
        check=True,
    )
    (repo / "tracked").write_text("fixture\n", encoding="utf-8")
    subprocess.run(  # noqa: S603, S607 -- fixed argv, no shell, inside a private tempdir
        [GIT, "-C", str(repo), "add", "tracked"],
        capture_output=True,
        text=True,
        check=True,
    )
    subprocess.run(  # noqa: S603, S607 -- fixed argv, no shell, inside a private tempdir
        [
            GIT, "-C", str(repo),
            "-c", "user.email=worktree-guard@example.invalid",
            "-c", "user.name=worktree-guard",
            "commit", "-q", "-m", "seed fixture",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    return repo


def set_bare(repo: Path, value: str | None) -> None:
    """Set or unset `core.bare` on a throwaway fixture — never a real checkout."""
    argv = [GIT, "-C", str(repo), "config", "core.bare", value] if value is not None else [
        GIT, "-C", str(repo), "config", "--unset", "core.bare"
    ]
    subprocess.run(  # noqa: S603, S607 -- fixed argv, no shell, inside a private tempdir
        argv, capture_output=True, text=True, check=False
    )


def _self_test_case(
    script: Path, cwd: Path, name: str, args: list[str], *, expect_red: bool, expect_stderr: str = ""
) -> None:
    """Run the CLI in a fixture cwd and require the expected verdict."""
    proc = subprocess.run(  # noqa: S603 -- fixed argv (this script + its flags), no shell
        [sys.executable, str(script), *args],
        capture_output=True,
        text=True,
        check=False,
        cwd=str(cwd),
    )
    was_red = proc.returncode != 0
    if was_red != expect_red:
        detail = (proc.stderr or proc.stdout).strip() or "no diagnostic output"
        print(
            f"self-test [{name}]: expected {'red' if expect_red else 'green'},"
            f" was {'red' if was_red else 'green'}: {detail}",
            file=sys.stderr,
        )
        sys.exit(1)
    if expect_stderr and expect_stderr not in proc.stderr:
        print(
            f"self-test [{name}]: verdict text missing {expect_stderr!r}: {proc.stderr.strip()}",
            file=sys.stderr,
        )
        sys.exit(1)


def self_test() -> None:
    """Re-prove every direction — the defect both ways included — on fixtures."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    def run_case(
        cwd: Path, name: str, args: list[str] | None = None, *, expect_red: bool, expect_stderr: str = ""
    ) -> None:
        ran.append(name)
        _self_test_case(script, cwd, name, args or [], expect_red=expect_red, expect_stderr=expect_stderr)

    with tempfile.TemporaryDirectory(prefix="git-worktree-guard-selftest-") as tmp:
        root = Path(tmp)

        good = make_repo(root, "good")
        # Green: a real working tree with the flag unset.
        run_case(good, "green-working-tree", expect_red=False)
        # Green: core.bare = false — the explicitly-false spelling is not the bug.
        set_bare(good, "false")
        run_case(good, "green-core-bare-false", expect_red=False)

        # Red — the real defect. core.bare = true over a checked-out repo is
        # what the base checkout carried; both rev-parse questions answer
        # `true` there, which is precisely why `git worktree add` refuses.
        broken = make_repo(root, "bare-flag-true")
        assert inside_work_tree_fixture(broken) == "true", "fixture must be a working tree first"
        set_bare(broken, "true")
        run_case(
            broken,
            "red-core-bare-true",
            expect_red=True,
            expect_stderr="core.bare",
        )
        # The same fixture, and the guard's primary assertion must red on it
        # too — that is the symptom a lane actually dies on.
        run_case(
            broken,
            "red-not-inside-work-tree",
            ["--assert", "inside-work-tree"],
            expect_red=True,
            expect_stderr="is-inside-work-tree",
        )

        # Red: a directory that is not a git repository at all fails closed.
        plain = root / "not-a-repo"
        plain.mkdir()
        run_case(
            plain,
            "red-not-a-git-repository",
            expect_red=True,
            expect_stderr="is-inside-work-tree",
        )

        # Red: an explicit --path that does not exist. Reached as a *child of
        # a real cwd*, because a missing --path is what the recipe can actually
        # hand the guard (the repo root vanishing mid-run), and cwd itself can
        # only be missing from outside this process. The guard must red, not
        # certify a directory it cannot read.
        run_case(
            good,
            "red-explicit-path-missing",
            ["--path", str(root / "absent")],
            expect_red=True,
            expect_stderr="could not be run",
        )

        # Red: git that cannot be run at all. A stub directory on PATH holding
        # no `git` reproduces a missing/renamed binary, which used to be the
        # silent-green shape of every cargo-based guard in this repo (MOR-130).
        no_git = root / "no-git-on-path"
        no_git.mkdir()
        ran.append("red-git-unavailable")
        _git_missing_case(script, good, no_git)

        # Red on an explicit --path that is broken, even though cwd is fine:
        # the recipe passes the repo root, and the check must answer for the
        # path it is handed rather than the one it happens to sit in.
        run_case(
            good,
            "red-explicit-path-broken",
            ["--path", str(broken)],
            expect_red=True,
            expect_stderr="core.bare",
        )
        run_case(
            plain,
            "green-explicit-path-good",
            ["--path", str(good)],
            expect_red=False,
        )

    print(f"worktree guard self-test: {len(ran)} cases, all pass")


def inside_work_tree_fixture(repo: Path) -> str:
    """Raw `--is-inside-work-tree` output for a fixture (no verdict, no exit)."""
    proc = subprocess.run(  # noqa: S603, S607 -- fixed argv, no shell, private tempdir
        [GIT, "-C", str(repo), "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout.strip()


def _git_missing_case(script: Path, cwd: Path, empty_bin: Path) -> None:
    """With no `git` on PATH the guard must red, not shrug."""
    env = {**os.environ, "PATH": str(empty_bin)}
    proc = subprocess.run(  # noqa: S603 -- fixed argv, no shell
        [sys.executable, str(script)],
        capture_output=True,
        text=True,
        check=False,
        cwd=str(cwd),
        env=env,
    )
    if proc.returncode == 0:
        print(
            "self-test [red-git-unavailable]: expected red with no git on PATH,"
            f" was green: {proc.stdout.strip()}",
            file=sys.stderr,
        )
        sys.exit(1)
    if "could not be run" not in proc.stderr:
        print(
            "self-test [red-git-unavailable]: verdict text missing 'could not be run':"
            f" {proc.stderr.strip()}",
            file=sys.stderr,
        )
        sys.exit(1)


def main(argv: Iterable[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        description="assert the checkout a build runs in is a working tree, not a bare repo"
    )
    parser.add_argument(
        "--path",
        type=Path,
        default=None,
        help="repo to check (default: the current directory, the tree the build runs in)",
    )
    parser.add_argument(
        "--assert",
        dest="only",
        choices=("inside-work-tree", "core-bare"),
        default=None,
        help="run one assertion alone (used by the self-test to bind each red to its symptom)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="re-prove every red and green direction against throwaway git init fixtures and exit",
    )
    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.self_test:
        self_test()
        return

    cwd = (args.path if args.path is not None else Path.cwd()).resolve()
    if args.only == "inside-work-tree":
        inside_work_tree(cwd)
        print(f"worktree guard ok: {cwd} is inside a working tree")
        return
    if args.only == "core-bare":
        assert_not_bare(cwd)
        print(f"worktree guard ok: {cwd} does not set core.bare = {BARE_TRUE!r}")
        return
    check(cwd)


if __name__ == "__main__":
    main()
