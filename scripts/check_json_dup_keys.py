#!/usr/bin/env python3
"""Fail on duplicate keys in tracked JSON.

Every JSON parser in this repo's stack — V8's JSON.parse, serde_json,
Python's json — silently keeps the LAST duplicate and discards the rest, so a
manifest can misstate its own dependency policy and every tool agrees with
the lie (gh#49 shipped four dead exact pins this way). This check is exact,
not heuristic: stdlib's object_pairs_hook sees every key pair before the dict
collapses them, so any repetition inside one object fails the gate. A second,
position-tracking scan (only on the failure path) reports the file, the key
and the line number of every occurrence.

Strict parsing also means comments or trailing commas fail loudly. All
tracked JSON here is strict; if a file ever genuinely needs JSONC, name it
.jsonc and extend the discovery pathspec deliberately — do not loosen the
parser.

Usage:
    check_json_dup_keys.py             # every tracked *.json (git ls-files)
    check_json_dup_keys.py --staged    # only JSON staged for commit (hook)
    check_json_dup_keys.py FILE...     # explicit files
    check_json_dup_keys.py --selftest  # fixture assertions, scripts/testdata/

Zero-dependency: Python 3 stdlib only, nothing pinned.
"""

import json
import subprocess
import sys
from pathlib import Path


class DuplicateKey(Exception):
    def __init__(self, key):
        self.key = key
        super().__init__(key)


def no_duplicate_pairs(pairs):
    """object_pairs_hook: sees every pair before the dict collapses them."""
    seen = set()
    for key, _ in pairs:
        if key in seen:
            raise DuplicateKey(key)
        seen.add(key)
    return dict(pairs)


def locate_duplicates(text):
    """Find every key repeated within a single object in *valid* JSON text.

    Returns a list of (object_label, key, line_numbers), innermost-first,
    with the line number of every occurrence of each repeated key. The strict
    parse in check_file establishes validity before this runs, so strings are
    terminated and braces balanced by construction.
    """
    dups = []
    stack = [{"kind": "obj", "keys": {}, "label": "<root>"}]
    last_key = None
    i, n, line = 0, len(text), 1

    def close_top():
        top = stack.pop()
        for key, lines in top["keys"].items():
            if len(lines) > 1:
                dups.append((top["label"], key, lines))

    while i < n:
        c = text[i]
        if c in " \t\r\n":
            if c == "\n":
                line += 1
            i += 1
        elif c == "{":
            if last_key is not None:
                label = last_key
            elif stack[-1]["kind"] == "arr":
                label = stack[-1]["label"] + "[]"
            else:
                label = "<root>"
            last_key = None
            stack.append({"kind": "obj", "keys": {}, "label": label})
            i += 1
        elif c == "[":
            label = last_key if last_key is not None else "<array>"
            last_key = None
            stack.append({"kind": "arr", "keys": {}, "label": label})
            i += 1
        elif c in "}]":
            last_key = None
            close_top()
            i += 1
        elif c == ":":
            i += 1
        elif c == '"':
            start_line = line
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                elif text[j] == '"':
                    break
                else:
                    j += 1
            raw = text[i + 1 : j]
            line += raw.count("\n")
            value = json.loads('"' + raw + '"')
            k = j + 1
            while k < n and text[k] in " \t\r\n":
                if text[k] == "\n":
                    line += 1
                k += 1
            if k < n and text[k] == ":":
                stack[-1]["keys"].setdefault(value, []).append(start_line)
                last_key = value
                i = k + 1
            else:
                last_key = None
                i = j + 1
        else:
            # number, true, false, null — contiguous until a delimiter
            j = i
            while j < n and text[j] not in " \t\r\n,]}":
                j += 1
            i = j

    while stack:
        close_top()
    dups.sort(key=lambda d: d[2][0])
    return dups


def check_file(path):
    """Return the failure lines for one JSON file; empty list means clean."""
    text = Path(path).read_text(encoding="utf-8-sig")
    try:
        json.loads(text)
    except ValueError as e:
        return [f"{path}: invalid JSON: {e}"]
    try:
        json.loads(text, object_pairs_hook=no_duplicate_pairs)
    except DuplicateKey as e:
        found = locate_duplicates(text)
        hits = [
            (label, key, lines) for label, key, lines in found if key == e.key
        ]
        out = []
        for label, _key, lines in hits:
            where = "" if label == "<root>" else f' in object "{label}"'
            occurrences = ", ".join(f"line {n}" for n in lines)
            out.append(
                f"{path}: duplicate key \"{e.key}\"{where}: "
                f"{occurrences} ({len(lines)} occurrences; "
                f"JSON silently kept the last one)"
            )
        if not out:
            out.append(f'{path}: duplicate key "{e.key}" (lines unavailable)')
        return out
    return []


def git_files(staged):
    if staged:
        out = subprocess.run(
            [
                "git",
                "diff",
                "--cached",
                "--name-only",
                "--diff-filter=ACM",
                "-z",
                "--",
                "*.json",
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        return [f for f in out.stdout.split("\0") if f]
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.json"],
        capture_output=True,
        text=True,
        check=True,
    )
    return [f for f in out.stdout.split("\0") if f]


def selftest():
    """Assert the fixtures behave: duplicates fail with file/key/lines,
    clean passes, invalid JSON fails."""
    here = Path(__file__).resolve().parent / "testdata"
    expectations = [
        ("clean.json.fixture", "clean", []),
        (
            "dup-keys.json.fixture",
            "dup",
            ['"vitest"', "line 6", "line 9", '"devDependencies"'],
        ),
        ("dup-top.json.fixture", "dup", ['"name"', "line 2", "line 5"]),
        (
            "dup-nested.json.fixture",
            "dup",
            ['"id"', "line 4", "line 4", '"packs"'],
        ),
        ("invalid.json.fixture", "invalid", ["invalid JSON"]),
    ]
    failures = 0
    for name, kind, must_contain in expectations:
        path = here / name
        got = check_file(path)
        if kind == "clean":
            if got:
                print(f"FAIL {name}: expected clean, got: {got}")
                failures += 1
            else:
                print(f"ok   {name}: clean passes")
            continue
        if not got:
            print(f"FAIL {name}: expected failure ({kind}), got none")
            failures += 1
            continue
        blob = "\n".join(got)
        missing = [s for s in must_contain if s not in blob]
        if missing:
            print(f"FAIL {name}: message missing {missing}: {got}")
            failures += 1
        else:
            print(f"ok   {name}: {got[0]}")
    if failures:
        print(f"selftest: {failures} failure(s)")
        return 1
    print("selftest: all fixture expectations hold")
    return 0


def main(argv):
    if "--selftest" in argv:
        return selftest()
    if len(argv) > 1 and argv[1] == "--staged":
        files = git_files(staged=True)
    elif len(argv) > 1:
        files = argv[1:]
    else:
        files = git_files(staged=False)
    failures = []
    for f in files:
        try:
            failures.extend(check_file(f))
        except OSError as e:
            failures.append(f"{f}: unreadable: {e}")
    if failures:
        for line in failures:
            print(line)
        print(f"FAILED: {len(failures)} problem(s) in {len(files)} JSON file(s)")
        return 1
    print(f"checked {len(files)} JSON file(s), no duplicate keys")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
