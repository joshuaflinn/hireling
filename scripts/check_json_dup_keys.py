#!/usr/bin/env python3
"""Gate guardrail: fail on duplicate keys in any tracked JSON file (gh#49).

JSON is last-wins: JSON.parse, serde_json and Python's json module all
silently keep the *last* duplicate key, so a manifest can ship dead keys
and no parser warns. gh#49: web/package.json carried four exact pins that
had been replaced, unnoticed, by caret ranges repeated under the same key.

Detection is exact, not heuristic: stdlib json with an object_pairs_hook,
which sees every key pair before the dict collapses them (so escaped-key
duplicates like {"a": 1, "\\u0061": 2} are caught too). The stdlib parse is
the pass/fail authority. Only on failure does a line-locating walk over the
raw text run, to name the file, the key and both line numbers; if that walk
cannot locate the pair the verdict stands and the message degrades.

Zero dependencies. Wired into `just ci-local` and .githooks/pre-commit.
Fails loudly if git or python3 is missing — a guardrail that skips is not
a guardrail.

Usage:
    check_json_dup_keys.py               # check the whole tracked JSON set
    check_json_dup_keys.py FILE [FILE…]  # check explicit files
    check_json_dup_keys.py --self-test   # run the fixture suite, exit 0/1
"""

import json
import subprocess
import sys
import tempfile
from json.decoder import scanstring  # stdlib's exact JSON string decoder
from pathlib import Path


class DuplicateKeyError(ValueError):
    """Raised by the pairs hook when an object repeats a key."""

    def __init__(self, key):
        super().__init__(f"duplicate key {key!r}")
        self.key = key


def strict_load(text):
    """Exact pass/fail authority. Return parsed data, raise DuplicateKeyError."""

    def reject_duplicates(pairs):
        seen = set()
        for key, _ in pairs:
            if key in seen:
                raise DuplicateKeyError(key)
            seen.add(key)
        return dict(pairs)

    # The hook sees every key pair before the dict collapses them — exact
    # even for escaped-key duplicates ("a" vs "\u0061").
    return json.loads(text, object_pairs_hook=reject_duplicates)


def find_duplicate_lines(text, key):
    """Return (first_line, dup_line) for the first textual repeat of `key`.

    Only ever called after strict_load raised for `key`, so the text is
    well-formed JSON. Strings decode through stdlib's own scanstring, so
    escapes and \\uXXXX sequences compare exactly as the parser compared
    them. Returns None if the walk cannot find the pair (the verdict was
    already decided by strict_load; only this message degrades).
    """
    line = 1
    i, n = 0, len(text)
    scopes = []  # 'o' object / 'a' array, innermost last
    seen = []  # parallel: per-object dict of key -> first line
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif c == '"':
            value, end = scanstring(text, i + 1)
            i = end
            # A key is a string followed, after whitespace, by ':'. In
            # well-formed JSON that test is exact — string values are
            # always followed by ',' '}' or ']'.
            j = i
            while j < n and text[j] in " \t\r\n":
                j += 1
            if j < n and text[j] == ":" and scopes and scopes[-1] == "o":
                first = seen[-1].get(value)
                if value == key and first is not None:
                    return (first, line)
                if first is None:
                    seen[-1][value] = line
        elif c == "{":
            scopes.append("o")
            seen.append({})
            i += 1
        elif c == "[":
            scopes.append("a")
            seen.append(None)
            i += 1
        elif c in "}]":
            scopes.pop()
            seen.pop()
            i += 1
        else:
            i += 1
    return None


def iter_tracked_json(repo_root):
    """Yield every tracked *.json path (repo-relative), via git ls-files."""
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.json"],
        cwd=repo_root, check=True, capture_output=True,
    )
    for raw in out.stdout.split(b"\0"):
        if raw:
            yield raw.decode("utf-8", "surrogateescape")


def check_files(paths):
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
            located = find_duplicate_lines(text, err.key)
            if located is None:
                print(
                    f"ERROR: {path}: duplicate key {err.key!r} "
                    "(line numbers could not be located — fix the duplicate)",
                    file=sys.stderr,
                )
            else:
                first, dup = located
                print(
                    f"ERROR: {path}: duplicate key {err.key!r} "
                    f"(first at line {first}, duplicated at line {dup})",
                    file=sys.stderr,
                )
            failures += 1
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


def self_test():
    """Fixture suite. Runs this script as a subprocess — the same entrypoint
    `just ci-local` and the pre-commit hook use — so the tests exercise the
    production path, not library internals."""
    # (name, text, expect_clean). Every fixture asserts against the real CLI.
    cases = [
        (
            "clean_nested",
            '{\n  "a": {"b": [1, 2, {"c": "d"}]},\n  "\\u0065": "escaped key, only once"\n}\n',
            True,
        ),
        # The gh#49 shape: dup key with the repeats on known lines.
        (
            "dup_top",
            '{\n  "name": "hireling",\n  "version": "1.0.0",\n  "name": "other"\n}\n',
            False,
        ),  # key "name": lines 2 and 4
        # package.json-shaped: dup inside a nested devDependencies block.
        (
            "dup_nested",
            '{\n  "name": "x",\n  "devDependencies": {\n    "vitest": "1.0.0",\n    "jsdom": "2.0.0",\n    "vitest": "^1.0.0"\n  }\n}\n',
            False,
        ),  # key "vitest": lines 4 and 7
        # Escaped-key dup a raw-text scanner would miss.
        (
            "dup_escaped",
            '{"a": 1, "\\u0061": 2}\n',
            False,
        ),
        # Same key in *different* objects is legal.
        (
            "legal_repeats_across_objects",
            '{\n  "outer": {"k": 1},\n  "inner": {"k": 2}\n}\n',
            True,
        ),
        # Dup keys separated by a multi-line value: line numbers must be real.
        (
            "dup_far_apart",
            '{\n  "k": [\n    1,\n    2\n  ],\n  "k": 3\n}\n',
            False,
        ),  # key "k": lines 2 and 6
        # A string *value* that looks like "k": must not fool the locator.
        (
            "dup_after_lookalike_value",
            '{\n  "msg": "he said \\"k\\": no",\n  "k": 1,\n  "k": 2\n}\n',
            False,
        ),  # key "k": lines 3 and 4 — never line 2
    ]
    failures = 0
    with tempfile.TemporaryDirectory(prefix="json-dup-keys-test-") as tmp:
        for name, text, expect_clean in cases:
            path = Path(tmp) / f"{name}.json"
            path.write_text(text, encoding="utf-8")
            proc = subprocess.run(
                [sys.executable, str(Path(__file__).resolve()), str(path)],
                capture_output=True, text=True,
            )
            combined = proc.stdout + proc.stderr
            if expect_clean and proc.returncode != 0:
                failures += 1
                print(f"FAIL {name}: expected clean, exit {proc.returncode}\n{combined}")
            elif not expect_clean and proc.returncode == 0:
                failures += 1
                print(f"FAIL {name}: expected failure, exited 0\n{combined}")
        # Line-number assertions on the failure cases with known positions.
        expected_lines = {
            "dup_top": ("2", "4", "name"),
            "dup_nested": ("4", "6", "vitest"),
            "dup_far_apart": ("2", "6", "k"),
            "dup_after_lookalike_value": ("3", "4", "k"),
        }
        for name, (first, dup, key) in expected_lines.items():
            path = Path(tmp) / f"{name}.json"
            proc = subprocess.run(
                [sys.executable, str(Path(__file__).resolve()), str(path)],
                capture_output=True, text=True,
            )
            combined = proc.stdout + proc.stderr
            for token in (f"line {first}", f"line {dup}", f"{key!r}"):
                if token not in combined:
                    failures += 1
                    print(f"FAIL {name}: output missing {token}\n{combined}")
    if failures:
        print(f"self-test: {failures} failure(s)", file=sys.stderr)
        return 1
    print(f"self-test: {len(cases)} fixtures + line assertions, all pass")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    paths = [a for a in argv if not a.startswith("-")]
    if not paths:
        paths = list(iter_tracked_json(repo_root=Path(__file__).parent.parent))
    if not paths:
        print("json duplicate-key check: no JSON files found, clean")
        return 0
    return check_files(paths)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
