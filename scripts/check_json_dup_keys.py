#!/usr/bin/env python3
# Copyright (c) 2026 Joshua Flinn.
# ruff: noqa: T201 -- a CLI guardrail: printing its verdict to stdout/stderr is the product.
"""Gate guardrail: fail on duplicate keys in any tracked JSON file (gh#49).

JSON is last-wins: JSON.parse, serde_json and Python's json module all
silently keep the *last* duplicate key, so a manifest can ship dead keys
and no parser warns. gh#49: web/package.json carried four exact pins that
had been replaced, unnoticed, by caret ranges repeated under the same key.

Detection is exact, not heuristic: stdlib json with an object_pairs_hook,
which sees every key pair before the dict collapses them (so escaped-key
duplicates like ``{"a": 1, "\u0061": 2}`` are caught too). The stdlib parse
is the pass/fail authority. Only on failure does a line-locating walk over
the raw text run, to name the file, the key and both line numbers; if that
walk cannot locate the pair the verdict stands and the message degrades.

Zero dependencies. Wired into ``just ci-local`` and .githooks/pre-commit.
Fails loudly if git or python3 is missing — a guardrail that skips is not
a guardrail.

Usage::

    check_json_dup_keys.py               # check the whole tracked JSON set
    check_json_dup_keys.py FILE [FILE…]  # check explicit files
    check_json_dup_keys.py --self-test   # run the fixture suite, exit 0/1
"""

import json
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Iterator, Sequence
from pathlib import Path
from typing import cast

# Resolved once at import; main() fails loudly when it is None (fail-closed).
GIT = shutil.which("git")


class DuplicateKeyError(ValueError):
    """Raised by the pairs hook when an object repeats a key."""

    def __init__(self, key: str) -> None:
        """Carry the duplicated key so the failure message can name it."""
        super().__init__(f"duplicate key {key!r}")
        self.key = key


class GitMissingError(RuntimeError):
    """Raised when git is unavailable — the tracked set cannot be enumerated."""

    def __init__(self) -> None:
        """Set the fixed message: the cause is always the same missing binary."""
        super().__init__("git not found on PATH — cannot enumerate tracked JSON")


def strict_load(text: str) -> object:
    """Exact pass/fail authority. Return parsed data, raise DuplicateKeyError."""

    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        seen: set[str] = set()
        for key, _ in pairs:
            if key in seen:
                raise DuplicateKeyError(key)
            seen.add(key)
        return dict(pairs)

    # The hook sees every key pair before the dict collapses them — exact
    # even for escaped-key duplicates ("a" vs "\u0061").
    data: object = json.loads(text, object_pairs_hook=reject_duplicates)
    return data


def find_duplicate_lines(text: str, key: str) -> tuple[int, int] | None:
    r"""Return (first_line, dup_line) for the first textual repeat of ``key``.

    Only ever called after strict_load raised for ``key``, so the text is
    well-formed JSON. Strings decode through stdlib json, so escapes and
    ``\uXXXX`` sequences compare exactly as the parser compared them.
    Returns None if the walk cannot find the pair (the verdict was already
    decided by strict_load; only this message degrades).
    """
    locator = _DupLocator(text, key)
    return locator.find()


class _DupLocator:
    """Line-locating walk over raw JSON text for one duplicated key."""

    def __init__(self, text: str, key: str) -> None:
        """Start the walk at line 1, index 0, with no scopes open."""
        self.text = text
        self.key = key
        self.line = 1
        self.i = 0
        self.scopes: list[str] = []  # 'o' object / 'a' array, innermost last
        self.seen: list[dict[str, int] | None] = []  # per-object key -> first line

    def find(self) -> tuple[int, int] | None:
        """Walk the text; return the first repeat pair of the key, or None."""
        while self.i < len(self.text):
            c = self.text[self.i]
            if c == "\n":
                self.line += 1
                self.i += 1
            elif c == "{":
                self.scopes.append("o")
                self.seen.append({})
                self.i += 1
            elif c == "[":
                self.scopes.append("a")
                self.seen.append(None)
                self.i += 1
            elif c in "}]":
                self.scopes.pop()
                self.seen.pop()
                self.i += 1
            elif c == '"':
                dup = self._consume_string()
                if dup is not None:
                    return dup
            else:
                self.i += 1
        return None

    def _consume_string(self) -> tuple[int, int] | None:
        """Decode the string token at the cursor; record it if it is a key."""
        value, self.i, self.line = self._scan_string(self.i)
        return self._record_key(value)

    def _scan_string(self, start: int) -> tuple[str, int, int]:
        r"""Decode the string token whose opening quote is at ``start``.

        Return ``(value, index_after_token, line_after_token)``. The token's
        raw span is found by an escape-aware quote scan; the content is then
        decoded by stdlib json, so escapes and ``\uXXXX`` sequences compare
        exactly as the parser compared them.
        """
        end = self._token_end(start)
        decoded: object = json.loads(self.text[start:end])
        value = cast("str", decoded)
        return value, end, self.line + self.text.count("\n", start, end)

    def _token_end(self, start: int) -> int:
        """Return the index just past the closing quote of the token."""
        i = start + 1
        n = len(self.text)
        while i < n:
            c = self.text[i]
            if c == "\\":
                i += 2  # the escaped char — including \" — cannot close the token
            elif c == '"':
                return i + 1
            else:
                i += 1
        unterminated = "unterminated string token"
        raise ValueError(unterminated)  # unreachable: strict_load validated the text

    def _record_key(self, value: str) -> tuple[int, int] | None:
        """Record ``value`` as a key of the innermost object, if it is one.

        Return ``(first_line, dup_line)`` when ``value`` repeats the target
        key, else None. Deciding key-vs-value by lookahead is exact in
        well-formed JSON: a string value is always followed by ',' '}' or
        ']', never by ':'.
        """
        if not self.scopes or self.scopes[-1] != "o" or self._peek_colon() < 0:
            return None
        scope = self.seen[-1]
        if scope is None:  # unreachable by construction ('o' scopes push a dict)
            return None
        first = scope.get(value)
        if value == self.key and first is not None:
            return (first, self.line)
        if first is None:
            scope[value] = self.line
        return None

    def _peek_colon(self) -> int:
        """Return the cursor if ':' follows whitespace, else -1."""
        j = self.i
        n = len(self.text)
        while j < n and self.text[j] in " \t\r\n":
            j += 1
        if j < n and self.text[j] == ":":
            return j
        return -1


def iter_tracked_json(repo_root: Path) -> Iterator[str]:
    """Yield every tracked *.json path (repo-relative), via git ls-files."""
    if GIT is None:
        raise GitMissingError
    # Audited for semgrep (dangerous-subprocess-use-audit): list argv, no
    # shell; args are the which(1)-resolved git path, fixed flags, and paths
    # produced by git itself — nothing attacker-controlled reaches argv.
    out = subprocess.run(  # noqa: S603 -- fixed argv, repo-controlled enumeration
        [GIT, "ls-files", "-z", "--", "*.json"],
        cwd=repo_root,
        check=True,
        capture_output=True,
    )
    for raw in out.stdout.split(b"\0"):
        if raw:
            yield raw.decode("utf-8", "surrogateescape")


def check_files(paths: Sequence[str]) -> int:
    """Check explicit paths (relative to cwd). Return process exit code."""
    failures = 0
    for path in paths:
        try:
            text = Path(path).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as err:
            print(f"ERROR: {path}: cannot read ({err})", file=sys.stderr)
            failures += 1
            continue
        try:
            strict_load(text)
        except DuplicateKeyError as err:
            failures += _report_duplicate(path, text, err.key)
        except ValueError as err:
            print(f"ERROR: {path}: not valid JSON ({err})", file=sys.stderr)
            failures += 1
    if failures:
        print(
            f"json duplicate-key check: {failures} file(s) with duplicate keys",
            file=sys.stderr,
        )
        return 1
    print(f"json duplicate-key check: {len(paths)} file(s), clean")
    return 0


def _report_duplicate(path: str, text: str, key: str) -> int:
    """Print the failure line for one duplicate key. Return 1 (one failure)."""
    located = find_duplicate_lines(text, key)
    if located is None:
        print(
            f"ERROR: {path}: duplicate key {key!r} "
            "(line numbers could not be located — fix the duplicate)",
            file=sys.stderr,
        )
        return 1
    first, dup = located
    print(
        f"ERROR: {path}: duplicate key {key!r} "
        f"(first at line {first}, duplicated at line {dup})",
        file=sys.stderr,
    )
    return 1


def self_test() -> int:
    """Run the fixture suite against this script's real CLI.

    Every case runs as a subprocess — the same entrypoint ``just ci-local``
    and the pre-commit hook use — so the tests exercise the production
    path, not library internals.
    """
    # (name, text, expect_clean). Clean fixtures pass; dup fixtures fail.
    cases: list[tuple[str, str, bool]] = [
        (
            "clean_nested",
            '{\n  "a": {"b": [1, 2, {"c": "d"}]},\n  "\\u0065": "escaped, once"\n}\n',
            True,
        ),
        # The gh#49 shape: dup key with the repeats on known lines.
        (
            "dup_top",
            '{\n  "name": "hireling",\n  "version": "1.0.0",\n  "name": "other"\n}\n',
            False,
        ),  # key "name": lines 2 and 4
        # package.json-shaped: dup inside a nested devDependencies block.
        # One list element per line so the asserted numbers (4 and 6) stay visible.
        (
            "dup_nested",
            _text(
                "{",
                '  "name": "x",',
                '  "devDependencies": {',
                '    "vitest": "1.0.0",',
                '    "jsdom": "2.0.0",',
                '    "vitest": "^1.0.0"',
                "  }",
                "}",
            ),
            False,
        ),  # key "vitest": lines 4 and 6
        # Escaped-key dup a raw-text scanner would miss.
        ("dup_escaped", '{"a": 1, "\\u0061": 2}\n', False),
        # Same key in *different* objects is legal.
        (
            "legal_repeats_across_objects",
            '{\n  "outer": {"k": 1},\n  "inner": {"k": 2}\n}\n',
            True,
        ),
        # Dup keys separated by a multi-line value: line numbers must be real.
        ("dup_far_apart", '{\n  "k": [\n    1,\n    2\n  ],\n  "k": 3\n}\n', False),
        # A string *value* that looks like "k": must not fool the locator.
        (
            "dup_after_lookalike_value",
            '{\n  "msg": "he said \\"k\\": no",\n  "k": 1,\n  "k": 2\n}\n',
            False,
        ),  # key "k": lines 3 and 4 — never line 2
    ]
    return _run_self_test_cases(cases)


def _text(*lines: str) -> str:
    """Join fixture lines into a newline-terminated JSON text."""
    return "\n".join(lines) + "\n"


def _run_self_test_cases(cases: list[tuple[str, str, bool]]) -> int:
    """Write each fixture to a tempdir, run the CLI on it, assert the verdict."""
    failures = 0
    with tempfile.TemporaryDirectory(prefix="json-dup-keys-test-") as tmp:
        for case in cases:
            failures += _run_one_case(Path(tmp), case)
        failures += _assert_reported_lines(Path(tmp))
    if failures:
        print(f"self-test: {failures} failure(s)", file=sys.stderr)
        return 1
    print(f"self-test: {len(cases)} fixtures + line assertions, all pass")
    return 0


def _script_path() -> Path:
    """Absolute path to this script — the entrypoint the gates invoke."""
    return Path(__file__).resolve()


def _run_cli(args: list[str]) -> subprocess.CompletedProcess[str]:
    """Run this script's CLI as the hook and the recipe do. Never raises."""
    # Audited for semgrep (dangerous-subprocess-use-audit): list argv, no
    # shell; args are sys.executable, this script's own path, and fixture
    # paths from our own tempdir — nothing attacker-controlled reaches argv.
    return subprocess.run(  # noqa: S603 -- sys.executable + our own script path
        [sys.executable, str(_script_path()), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def _run_one_case(tmp: Path, case: tuple[str, str, bool]) -> int:
    """Run one (name, text, expect_clean) case; return 1 on failure, else 0."""
    name, fixture_text, expect_clean = case
    path = tmp / f"{name}.json"
    path.write_text(fixture_text, encoding="utf-8")
    proc = _run_cli([str(path)])
    combined = proc.stdout + proc.stderr
    if expect_clean and proc.returncode != 0:
        print(f"FAIL {name}: expected clean, exit {proc.returncode}\n{combined}")
        return 1
    if not expect_clean and proc.returncode == 0:
        print(f"FAIL {name}: expected failure, exited 0\n{combined}")
        return 1
    return 0


# name -> (first line, dup line, key) expected in the failure output.
EXPECTED_LINES: dict[str, tuple[str, str, str]] = {
    "dup_top": ("2", "4", "name"),
    "dup_nested": ("4", "6", "vitest"),
    "dup_far_apart": ("2", "6", "k"),
    "dup_after_lookalike_value": ("3", "4", "k"),
}


def _assert_reported_lines(tmp: Path) -> int:
    """Assert the failure output names the key and both real line numbers."""
    failures = 0
    for name, (first, dup, key) in EXPECTED_LINES.items():
        path = tmp / f"{name}.json"
        proc = _run_cli([str(path)])
        combined = proc.stdout + proc.stderr
        for token in (f"line {first}", f"line {dup}", f"{key!r}"):
            if token not in combined:
                failures += 1
                print(f"FAIL {name}: output missing {token}\n{combined}")
    return failures


def main(argv: Sequence[str]) -> int:
    """Entry point: --self-test, explicit files, or the whole tracked set."""
    if GIT is None:
        print(
            "ERROR: git not found on PATH — the duplicate-key check cannot run.",
            file=sys.stderr,
        )
        return 1
    if "--self-test" in argv:
        return self_test()
    paths = [a for a in argv if not a.startswith("-")]
    if not paths:
        repo_root = Path(__file__).parent.parent
        # Resolve against the repo root, not cwd: the hook and the recipe run
        # from the root, but the gate's pytest may invoke from anywhere.
        paths = [str(repo_root / rel) for rel in iter_tracked_json(repo_root=repo_root)]
    if not paths:
        print("json duplicate-key check: no JSON files found, clean")
        return 0
    return check_files(paths)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
