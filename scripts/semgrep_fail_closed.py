#!/usr/bin/env python3
"""semgrep-fail-closed: the scan:semgrep JSON carries no blind spots (gh#332).

semgrep --json records a rule that could not parse its target as an
`errors[]` entry at `level: warn` with `type: ["PartialParsing", …]` — a skip
that reads as a clean pass because the command exits 0 with `"results": []`.
A rule that could not read its target looks exactly like one that read it and
found nothing. This guard reads the same JSON and fails closed on any
warn/PartialParsing entry, so a skipped target reds instead of passing.

`level: info` entries (the image's 19 `Missing plugin` Apex/Elixir pro-engine
notices) are tolerated: they do not mark a target the rules skipped.

Usage:
    semgrep scan --json --error … | python3 scripts/semgrep_fail_closed.py
    python3 scripts/semgrep_fail_closed.py semgrep.json   # file form
    python3 scripts/semgrep_fail_closed.py --self-test
"""

# ruff: noqa: T201, S603 -- a CLI guardrail: printing the verdict is the
# product, and the self-test spawns this very script (fixed argv, no shell).

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, NoReturn


def fail(message: str) -> NoReturn:
    """Print the verdict to stderr and exit 1 — the only red this guard knows."""
    print(f"semgrep fail-closed: {message}", file=sys.stderr)
    sys.exit(1)


def parse_doc(data: bytes) -> dict[str, Any]:
    """Parse semgrep's JSON output, failing closed on anything unreadable."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        fail(f"semgrep JSON is not UTF-8: {exc}")
    try:
        doc = json.loads(text)
    except json.JSONDecodeError as exc:
        fail(f"semgrep JSON is not parseable ({exc}) — refusing to pass")
    if not isinstance(doc, dict):
        fail("semgrep JSON top-level is not an object — refusing to pass")
    for key in ("results", "errors"):
        if not isinstance(doc.get(key), list):
            fail(f"semgrep JSON lacks a {key!r} list — refusing to pass")
    return doc


def blind_spot_entries(errors: list[Any]) -> list[Any]:
    """Return the errors[] entries this guard reds on (warn / PartialParsing)."""
    out = []
    for entry in errors:
        if not isinstance(entry, dict):
            out.append(entry)  # a malformed entry is itself a blind spot
            continue
        serialized = json.dumps(entry)
        if entry.get("level") == "warn" or "PartialParsing" in serialized:
            out.append(entry)
    return out


def check(data: bytes) -> None:
    """Run the assertion over semgrep's JSON; print findings and a summary."""
    doc = parse_doc(data)
    results = doc["results"]
    errors = doc["errors"]
    paths = doc.get("paths")
    scanned = paths.get("scanned", []) if isinstance(paths, dict) else []
    blind = blind_spot_entries(errors)
    for entry in blind:
        # The whole entry: the rule, the path/line, and the parse error.
        print(f"semgrep fail-closed: blind-spot entry: {json.dumps(entry)}")
    if blind:
        fail(
            f"{len(blind)} error(s) mark a target semgrep could not parse — a"
            " skipped target reads as a clean pass; fix the target or the rule"
            " before this passes"
        )
    info = [e for e in errors if isinstance(e, dict) and e.get("level") == "info"]
    print(
        f"semgrep fail-closed ok: {len(results)} finding(s),"
        f" {len(scanned)} path(s) scanned, {len(info)} info notice(s) tolerated"
    )


def main() -> None:
    """Parse arguments and run the fail-closed assertion over the JSON."""
    parser = argparse.ArgumentParser(
        description="fail closed on semgrep --json blind spots"
    )
    parser.add_argument(
        "json_file",
        nargs="?",
        help="path to semgrep --json output (default: read stdin)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="re-prove every red direction against synthetic JSON and exit",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    data = Path(args.json_file).read_bytes() if args.json_file else sys.stdin.buffer.read()
    check(data)


# A guard that has never been seen to fail is not evidence. The self-test
# re-proves every red direction on every ci-local run through the same CLI
# the justfile invokes — subprocess with cwd set, exactly how production
# calls it.


def _self_test_case(
    script: Path, name: str, doc: bytes, *, expect_red: bool
) -> None:
    with tempfile.TemporaryDirectory(prefix="semgrep-failclosed-") as tmp:
        tree = Path(tmp)
        (tree / "semgrep.json").write_bytes(doc)
        proc = subprocess.run(  # nosemgrep -- audited: dangerous-subprocess-use-audit
            [sys.executable, str(script), "semgrep.json"],
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


def _doc(results: list[Any], errors: list[Any]) -> bytes:
    """Encode a minimal semgrep-shaped JSON document."""
    return json.dumps(
        {"results": results, "errors": errors, "paths": {"scanned": ["a.py"]}}
    ).encode("utf-8")


def self_test() -> None:
    """Re-prove every red direction against synthetic JSON through the CLI."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    def case(name: str, doc: bytes, *, expect_red: bool) -> None:
        ran.append(name)
        _self_test_case(script, name, doc, expect_red=expect_red)

    # Green: no findings, one tolerated info notice (the Missing-plugin shape).
    case(
        "clean-with-info",
        _doc([], [{"level": "info", "type": "Missing plugin", "message": "Apex"}]),
        expect_red=False,
    )
    # Green: findings present are semgrep's own rc, not this guard's red.
    case(
        "findings-pass-through",
        _doc([{"check_id": "x", "path": "a.py"}], []),
        expect_red=False,
    )
    # Red: a warn entry (the PartialParsing shape, verbatim).
    case(
        "warn-entry",
        _doc(
            [],
            [{"code": 3, "level": "warn", "type": ["PartialParsing", []]}],
        ),
        expect_red=True,
    )
    # Red: the PartialParsing marker hides under an unexpected level.
    case(
        "partialparsing-under-info",
        _doc([], [{"level": "info", "type": "Missing plugin", "message": "PartialParsing"}]),
        expect_red=True,
    )
    # Red: a malformed (non-object) errors entry.
    case("non-object-entry", _doc([], ["not a dict"]), expect_red=True)
    # Red: unparseable JSON.
    case("unparseable", b"{not json", expect_red=True)
    # Red: top-level is not an object.
    case("top-level-not-object", b"[]", expect_red=True)
    # Red: the errors list is missing (semgrep always emits it; absence is a fault).
    case("missing-errors", b'{"results": []}', expect_red=True)
    # Red: the results list is missing.
    case("missing-results", b'{"errors": []}', expect_red=True)

    print(f"semgrep fail-closed self-test: {len(ran)} cases, all pass")


if __name__ == "__main__":
    main()
