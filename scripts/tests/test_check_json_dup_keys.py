# Copyright (c) 2026 Joshua Flinn.
"""Gate-side pytest suite for the duplicate-key checker (gh#49).

Every test drives the checker's CLI as a subprocess — the same entrypoint
`just ci-local` and the pre-commit hook use — so these tests exercise the
production path, not library internals. The in-script `--self-test` suite
covers the full fixture matrix on the dev-machine gate (`just ci-local`);
this file proves detection independently inside the pinned grizzly-gate
image, which runs pytest from its own venv.
"""

import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "check_json_dup_keys.py"


def _run_cli(args: list[str]) -> subprocess.CompletedProcess[str]:
    """Run the checker's CLI. check=False: the tests assert exit codes."""
    return subprocess.run(  # noqa: S603 -- sys.executable + our own script path
        [sys.executable, str(SCRIPT), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def test_self_test_suite_passes() -> None:
    """The in-script fixture suite (7 fixtures + line assertions) is green."""
    proc = _run_cli(["--self-test"])
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "all pass" in proc.stdout


def test_clean_file_passes(tmp_path: Path) -> None:
    """A valid nested document exits 0 and reports a clean file."""
    path = tmp_path / "clean.json"
    path.write_text('{"a": {"b": [1, 2]}, "c": "d"}\n', encoding="utf-8")
    proc = _run_cli([str(path)])
    assert proc.returncode == 0, proc.stderr
    assert "clean" in proc.stdout


def test_duplicate_fails_naming_file_key_and_lines(tmp_path: Path) -> None:
    """The gh#49 shape: message carries the file, the key and both lines."""
    path = tmp_path / "dup.json"
    path.write_text(
        '{\n  "name": "x",\n  "version": "1.0.0",\n  "name": "y"\n}\n',
        encoding="utf-8",
    )
    proc = _run_cli([str(path)])
    assert proc.returncode == 1
    err = proc.stderr
    assert str(path) in err
    assert "'name'" in err
    assert "line 2" in err
    assert "line 4" in err


def test_escaped_key_duplicate_is_caught(tmp_path: Path) -> None:
    r"""An escaped-key dup ("\u0061" vs "a") fails — raw scanners miss it."""
    path = tmp_path / "escaped.json"
    path.write_text('{"a": 1, "\\u0061": 2}\n', encoding="utf-8")
    proc = _run_cli([str(path)])
    assert proc.returncode == 1, proc.stderr
    assert "duplicate key" in proc.stderr


def test_tracked_tree_is_clean() -> None:
    """No-arg mode scans the whole tracked JSON set and finds zero dups.

    Runs the real gate on this checkout's tree; path resolution is against
    the repo root, so cwd does not matter.
    """
    proc = _run_cli([])
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "clean" in proc.stdout
