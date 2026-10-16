#!/usr/bin/env python3
"""check-runner-label: the gate's runner label and the cache key agree (gh#331).

The class fault this closes: after PR #319 the gate workflow carries four
`ubuntu-24.04` literals that must agree by hand — `runs-on:` and the label
embedded in the cache `key:` and both `restore-keys:` lines. A mismatch is
silent: change `runs-on:` and leave the key, and the cache degrades to a
miss rather than a red gate — `runner.os` is "Linux" on every Ubuntu, so the
key cannot catch the drift either. This check reads the workflow's own text
and reds when the labels disagree.

Failing closed: a missing `runs-on:`, a missing cache `key:` line, a missing
`restore-keys:` block, a cache line whose embedded label is off-shape, or
any two labels that disagree all red — a guardrail that skips is not a
guardrail.

The runner label itself contains a hyphen (`ubuntu-24.04`), so the boundary
after the label is structural, never a bare `-`: the key and the first
restore-keys entry end the label at `-${{ env.GATE_IMAGE }}`; the last
restore-keys entry ends it at the trailing `-`.

Zero dependencies, stdlib only — same discipline as
scripts/check_gate_vendored.py and scripts/ci_drift_check.py.
"""

# ruff: noqa: T201, S603 -- a CLI guardrail: printing its verdict is the
# product, and the self-test spawns this very script (fixed argv, no shell).

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NoReturn

WORKFLOW = Path(".github/workflows/gate.yml")

# The cache key family shares one prefix; the label rides immediately after
# it. IMAGE_MARK is the structural boundary after the label in the key and
# the first restore-keys entry (`grizzly-gate-<label>-${{ env.GATE_IMAGE }}`).
CACHE_PREFIX = "grizzly-gate-"
IMAGE_MARK = "-${{ env.GATE_IMAGE }}"

# `runs-on: <label>` — the job's runner, the authoritative label.
RUNS_ON_RE = re.compile(r"^ *runs-on: *(\S+)$", re.MULTILINE)
# The cargo cache key line: `key: grizzly-gate-<label>-${{ env.GATE_IMAGE }}-…`.
# Scoped to the cache-key family so an unrelated `key:` (e.g. npm) never
# matches, and a cache key that stops carrying the prefix reads as missing.
KEY_LINE_RE = re.compile(r"^ *key: *(grizzly-gate-\S.*)$", re.MULTILINE)
# The restore-keys block opener: `restore-keys: |`, a YAML block scalar.
RESTORE_KEYS_RE = re.compile(r"^ *restore-keys: *\| *$")


def fail(message: str) -> NoReturn:
    """Print the verdict to stderr and exit 1 — the only red this guard knows."""
    print(f"runner-label check: {message}", file=sys.stderr)
    sys.exit(1)


def embedded_label(line: str) -> str | None:
    """Return the label embedded after CACHE_PREFIX in `line`, or None.

    The label runs from after `grizzly-gate-` to the first of the two
    structural boundaries: `-${{ env.GATE_IMAGE }}` (key and the first
    restore-keys entry) or the trailing `-` (the last restore-keys entry).
    Any other shape — no prefix, no boundary — is off-shape and returns
    None so the caller fails closed rather than guessing.
    """
    index = line.find(CACHE_PREFIX)
    if index == -1:
        return None
    rest = line[index + len(CACHE_PREFIX):]
    if IMAGE_MARK in rest:
        return rest[: rest.index(IMAGE_MARK)]
    stripped = rest.rstrip()
    if stripped.endswith("-"):
        return stripped[:-1]
    return None


def collect_restore_entries(lines: list[str]) -> list[tuple[int, str]]:
    """Return (line_no, text) for every entry in the restore-keys block.

    The block is the indented literal lines after `restore-keys: |`. A line
    stops belonging to the block when its indentation stops exceeding the
    opener's. Blank lines are skipped.
    """
    blocks: list[list[tuple[int, str]]] = []
    for index, line in enumerate(lines):
        if not RESTORE_KEYS_RE.match(line):
            continue
        indent = len(line) - len(line.lstrip())
        entries: list[tuple[int, str]] = []
        for lineno in range(index + 1, len(lines)):
            following = lines[lineno]
            if not following.strip():
                continue
            if (len(following) - len(following.lstrip())) <= indent:
                break
            entries.append((lineno + 1, following))
        blocks.append(entries)
    if not blocks:
        fail(
            f"{WORKFLOW}: no `restore-keys: |` block found — the cache"
            " restore-keys line is part of the reviewed one-line PR this"
            " guard protects"
        )
    if len(blocks) > 1:
        fail(f"{WORKFLOW}: {len(blocks)} `restore-keys:` blocks — expected exactly one")
    return blocks[0]


def _single_runs_on(lines: list[str]) -> str:
    """Return the job's runs-on label, failing closed on none or several."""
    runs_on_lines = [line for line in lines if RUNS_ON_RE.match(line)]
    if not runs_on_lines:
        fail(f"{WORKFLOW}: no `runs-on:` line found")
    if len(runs_on_lines) > 1:
        fail(
            f"{WORKFLOW}: {len(runs_on_lines)} `runs-on:` lines — expected"
            " exactly one"
        )
    match = RUNS_ON_RE.match(runs_on_lines[0])
    if match is None:  # unreachable: the line matched to get here
        fail(f"{WORKFLOW}: runs-on line failed to re-match — shape changed mid-parse")
    return match.group(1)


def _single_key_line(lines: list[str]) -> tuple[int, str]:
    """Return (lineno, value) of the one cache key line, failing closed."""
    key_hits: list[tuple[int, str]] = []
    for lineno, line in enumerate(lines, 1):
        match = KEY_LINE_RE.match(line)
        if match:
            key_hits.append((lineno, match.group(1)))
    if not key_hits:
        fail(
            f"{WORKFLOW}: no cache `key:` line carrying the"
            f" `{CACHE_PREFIX}<label>` shape — refusing to pass over a hole"
        )
    if len(key_hits) > 1:
        fail(f"{WORKFLOW}: {len(key_hits)} cache `key:` lines — expected exactly one")
    return key_hits[0]


def check(text: str) -> None:
    """Parse the workflow text and require every label to agree."""
    lines = text.splitlines()
    runner_label = _single_runs_on(lines)
    key_lineno, key_value = _single_key_line(lines)

    entries = collect_restore_entries(lines)
    if not entries:
        fail(f"{WORKFLOW}: the `restore-keys:` block parsed to zero entries")

    # Every cache label — key and each restore-keys entry — must equal the
    # runs-on label. Off-shape labels are a red, never a skip.
    problems: list[str] = []
    key_label = embedded_label(key_value)
    if key_label is None:
        fail(
            f"{WORKFLOW}:{key_lineno}: cache key label is off-shape —"
            f" expected `{CACHE_PREFIX}<label>`, refusing to guess"
        )
    if key_label != runner_label:
        problems.append(f"key line {key_lineno} embeds '{key_label}'")

    entry_labels: list[str] = []
    for lineno, entry in entries:
        label = embedded_label(entry)
        if label is None:
            fail(
                f"{WORKFLOW}:{lineno}: restore-keys entry label is off-shape —"
                f" expected `{CACHE_PREFIX}<label>`, refusing to guess"
            )
        entry_labels.append(label)
        if label != runner_label:
            problems.append(f"restore-keys line {lineno} embeds '{label}'")

    if problems:
        fail(
            f"runs-on label '{runner_label}' disagrees with the cache label(s): "
            + "; ".join(problems)
            + " — the label must move in the same reviewed one-line PR"
        )
    print(
        f"runner-label check ok: runs-on '{runner_label}' matches the cache"
        f" key and {len(entry_labels)} restore-keys entr"
        f"{'y' if len(entry_labels) == 1 else 'ies'}"
    )


def main() -> None:
    """Run the check against the workflow; --self-test re-proves the reds."""
    parser = argparse.ArgumentParser(
        description="check the gate's runs-on label against the cache key/restore-keys"
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="re-prove every red direction against synthetic trees and exit",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return

    if not WORKFLOW.is_file():
        fail(f"{WORKFLOW} not found — run from the repo root")
    check(WORKFLOW.read_text(encoding="utf-8"))


# A guard that has never been seen to fail is not evidence. The self-test
# re-proves every red direction on every ci-local run, against synthetic
# trees, through the same CLI the justfile and the workflow invoke —
# subprocess with cwd set, exactly how production calls it.

SELF_TEST_KEY = (
    "          key: grizzly-gate-{label}-${{{{ env.GATE_IMAGE }}}}-"
    "${{{{ hashFiles('Cargo.lock') }}}}"
)
SELF_TEST_RESTORE_1 = "            grizzly-gate-{label}-${{{{ env.GATE_IMAGE }}}}-"
SELF_TEST_RESTORE_2 = "            grizzly-gate-{label}-"


def _synthetic_workflow(
    runs_on: str, key_label: str, restore_1: str, restore_2: str
) -> str:
    """Build a minimal gate workflow with the four labels in their places."""
    return (
        "jobs:\n"
        "  gate:\n"
        f"    runs-on: {runs_on}\n"
        "    steps:\n"
        "      - name: Restore gate cargo cache\n"
        "        uses: actions/cache@v4\n"
        "        with:\n"
        "          path: /home/runner/.cache/grizzly-gate\n"
        + SELF_TEST_KEY.format(label=key_label)
        + "\n"
        + "          restore-keys: |\n"
        + SELF_TEST_RESTORE_1.format(label=restore_1)
        + "\n"
        + SELF_TEST_RESTORE_2.format(label=restore_2)
        + "\n"
    )


def _self_test_case(
    script: Path, tree: Path, name: str, *, expect_red: bool
) -> None:
    """Run the CLI in a synthetic tree and require the expected verdict."""
    proc = subprocess.run(  # nosemgrep -- audited: dangerous-subprocess-use-audit
        # Fixed argv (this script + its own flags), no shell, cwd is a
        # private tempdir the test just built. Nothing externally controllable.
        [sys.executable, str(script)],
        capture_output=True,
        text=True,
        check=False,
        cwd=tree,
    )
    was_red = proc.returncode != 0
    if was_red != expect_red:
        detail = proc.stderr.strip() or "no diagnostic output"
        print(
            f"self-test [{name}]: expected {'red' if expect_red else 'green'},"
            f" was {'red' if was_red else 'green'}: {detail}",
            file=sys.stderr,
        )
        sys.exit(1)


def self_test() -> None:
    """Re-prove every red direction plus the green, on synthetic trees."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    with tempfile.TemporaryDirectory(prefix="runner-label-selftest-") as tmp:
        root = Path(tmp)

        def run_case(name: str, text: str, *, expect_red: bool) -> None:
            """Write a synthetic workflow tree and run one CLI case."""
            tree = root / name
            (tree / ".github" / "workflows").mkdir(parents=True)
            (tree / ".github" / "workflows" / "gate.yml").write_text(
                text, encoding="utf-8"
            )
            ran.append(name)
            _self_test_case(script, tree, name, expect_red=expect_red)

        # Green: all four labels agree.
        run_case(
            "green",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04"),
            expect_red=False,
        )

        # Red: the key label disagrees with runs-on — the mismatch PR #319
        # round-2's four hand-maintained literals made possible. Inverted from
        # the historical direction on purpose: runs-on stays and the key
        # drifts, so only the key comparison fires. Moving runs-on instead
        # would fire all three comparisons and the case would red through the
        # restore-keys path with the key comparison deleted.
        run_case(
            "red-key-label",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-22.04", "ubuntu-24.04", "ubuntu-24.04"),
            expect_red=True,
        )

        # Red: the first restore-keys label disagrees.
        run_case(
            "red-restore-1",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-22.04", "ubuntu-24.04"),
            expect_red=True,
        )

        # Red: the last restore-keys label disagrees.
        run_case(
            "red-restore-2",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-22.04"),
            expect_red=True,
        )

        # Red: no runs-on line at all.
        run_case(
            "red-missing-runs-on",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04")
            .replace("    runs-on: ubuntu-24.04\n", ""),
            expect_red=True,
        )

        # Red: no cache key line (the prefix is gone).
        run_case(
            "red-missing-key",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04")
            .replace(SELF_TEST_KEY.format(label="ubuntu-24.04") + "\n", ""),
            expect_red=True,
        )

        # Red: the restore-keys block is gone.
        run_case(
            "red-missing-restore-keys",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04")
            .replace(
                "          restore-keys: |\n"
                + SELF_TEST_RESTORE_1.format(label="ubuntu-24.04")
                + "\n"
                + SELF_TEST_RESTORE_2.format(label="ubuntu-24.04")
                + "\n",
                "",
            ),
            expect_red=True,
        )

        # Red: the key line loses the grizzly-gate- prefix (off-shape).
        run_case(
            "red-key-offshape",
            _synthetic_workflow("ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04")
            .replace(
                SELF_TEST_KEY.format(label="ubuntu-24.04"),
                "          key: ubuntu-24.04-${{ env.GATE_IMAGE }}-${{ hashFiles('Cargo.lock') }}",
            ),
            expect_red=True,
        )

    print(f"runner-label self-test: {len(ran)} cases, all pass")


if __name__ == "__main__":
    main()
