#!/usr/bin/env python3
"""check-gate-vendored: the vendored gate config has not drifted (gh#314).

The class fault this closes: a hand-copied mirror of the gate image's
eslint config drifts the moment the image digest moves, and nothing reds
because the copy is just text no check re-reads against the pinned digest.
Two directions, both failing closed:

1. Offline (default): the vendored file named by scripts/gate_vendored.tsv
   still hashes to the sha256 that manifest records. The hash covers the
   bytes below the `// === vendored-source-begin ===` marker — the image's
   own source, verbatim — so a hand edit above the marker (a provenance
   comment) is fine and a hand edit below it reds.

2. --image-source KEY=PATH (CI-side): the image itself still carries those
   bytes. The gate workflow extracts the file from the pinned digest (the
   digest is named once, at .github/workflows/gate.yml) and passes it here;
   a digest bump that moved the config without re-vendoring reds this
   direction instead of silently stranding the vendored copy.
   `--require-image-sources` (also CI-side) reds when a manifest row is not
   covered by an `--image-source`, so the image-side extraction cannot
   silently skip a row.

The manifest is written by scripts/fetch_gate_file.py (the producer); this
is the verifier. One row per vendored file, three tab-separated fields:
<image-path> <vendored-path> <sha256>.

Zero dependencies, stdlib only — same discipline as the guardrails it sits
beside. A guardrail that skips is not a guardrail: a missing manifest, a
missing vendored file, a missing marker, or a malformed row all fail loudly.
"""

# ruff: noqa: T201, S603 -- a CLI guardrail: printing its verdict is the
# product, and the self-test spawns this very script (fixed argv, no shell).

import argparse
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NoReturn

MANIFEST = Path("scripts/gate_vendored.tsv")
MANIFEST_FIELDS = 3
SHA256_HEX_LEN = 64

# Must stay byte-identical to VENDOR_MARKER in scripts/fetch_gate_file.py:
# that script writes the line, this one hashes everything below it. A
# mismatch is a marker the check cannot find — which fails closed, but the
# fix is to re-align the two literals, not to patch around the red.
MARKER = b"// === vendored-source-begin ===\n"


def fail(message: str) -> NoReturn:
    """Print the verdict to stderr and exit 1 — the only red this guard knows."""
    print(f"vendored-drift check: {message}", file=sys.stderr)
    sys.exit(1)


def load_manifest() -> list[tuple[str, str, str]]:
    """Parse (image-path, vendored-path, sha256) rows. Malformed rows red."""
    if not MANIFEST.is_file():
        fail(
            f"{MANIFEST} not found — the manifest is the single authority for"
            " what is vendored; refusing to pass without it"
        )
    rows: list[tuple[str, str, str]] = []
    seen: set[str] = set()
    for lineno, line in enumerate(
        MANIFEST.read_text(encoding="utf-8").splitlines(), 1
    ):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != MANIFEST_FIELDS:
            fail(
                f"{MANIFEST}:{lineno}: expected {MANIFEST_FIELDS} tab-separated"
                f" fields (image-path, vendored-path, sha256), got {len(fields)}"
            )
        image_path, vendored_path, sha256 = (field.strip() for field in fields)
        if len(sha256) != SHA256_HEX_LEN or any(c not in "0123456789abcdef" for c in sha256):
            fail(f"{MANIFEST}:{lineno}: '{sha256}' is not a 64-char hex sha256")
        if image_path in seen:
            fail(f"{MANIFEST}:{lineno}: image path '{image_path}' declared twice")
        seen.add(image_path)
        rows.append((image_path, vendored_path, sha256))
    if not rows:
        fail(
            f"{MANIFEST} parsed to zero rows — printing coverage over a hole is"
            " the fault this guards"
        )
    return rows


def sha256_bytes(data: bytes) -> str:
    """Return the lowercase hex sha256 of `data`."""
    return hashlib.sha256(data).hexdigest()


def vendored_body(path: Path) -> bytes:
    """Return the bytes below the marker in a vendored file, failing closed.

    A vendored file is `provenance header + MARKER + image source`. The
    provenance header may be edited (it documents the file); the source
    bytes may not. A missing marker is a file this check cannot interpret —
    fail, never skip.
    """
    if not path.is_file():
        fail(f"vendored file {path} not found — refusing to pass")
    data = path.read_bytes()
    index = data.find(MARKER)
    if index == -1:
        fail(
            f"vendored file {path} has no marker line ({MARKER.decode().strip()!r})"
            " — it is not the file scripts/fetch_gate_file.py writes"
        )
    return data[index + len(MARKER) :]


def check_offline(rows: list[tuple[str, str, str]]) -> None:
    """Direction 1: every vendored file hashes to its recorded sha256."""
    for image_path, vendored_path, sha256 in rows:
        actual = sha256_bytes(vendored_body(Path(vendored_path)))
        if actual != sha256:
            fail(
                f"{vendored_path} has sha256 {actual}, manifest records {sha256}"
                f" for {image_path} — the vendored copy drifted from the pinned"
                " image; re-vendor with scripts/fetch_gate_file.py (or fix the"
                " hand edit below the marker)"
            )


def check_image_sources(
    rows: list[tuple[str, str, str]], sources: dict[str, Path]
) -> None:
    """Direction 2: the image itself still carries the recorded bytes."""
    for key, path in sources.items():
        if not path.is_file():
            fail(f"image source {path} for '{key}' not found — refusing to pass")
        row = next((r for r in rows if r[0] == key), None)
        if row is None:
            fail(
                f"image source key '{key}' matches no row in {MANIFEST} — the"
                " manifest and the CI extraction disagree"
            )
        actual = sha256_bytes(path.read_bytes())
        if actual != row[2]:
            fail(
                f"image carries sha256 {actual} for {key}, manifest records"
                f" {row[2]} — the pinned digest's config moved; re-vendor with"
                " scripts/fetch_gate_file.py in the bump's own PR"
            )
    # A manifest row without an --image-source is not an error: it is still
    # covered offline (direction 1), and --image-source is CI-side by design.


def main() -> None:
    """Parse arguments, run the offline check, then the image-source check."""
    parser = argparse.ArgumentParser(
        description="check the vendored gate config against its recorded hash"
    )
    parser.add_argument(
        "--image-source",
        action="append",
        default=[],
        metavar="KEY=PATH",
        help="image path key and the file extracted from the pinned image (repeatable)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="re-prove every red direction against synthetic trees and exit",
    )
    parser.add_argument(
        "--require-image-sources",
        action="store_true",
        help="red if any manifest row lacks an --image-source (CI-side)",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return

    rows = load_manifest()
    check_offline(rows)

    sources: dict[str, Path] = {}
    for item in args.image_source:
        if "=" not in item:
            fail(f"--image-source '{item}' is not KEY=PATH")
        key, _, path = item.partition("=")
        if not key or not path:
            fail(f"--image-source '{item}' has an empty key or path")
        sources[key] = Path(path)
    if sources:
        check_image_sources(rows, sources)
    if args.require_image_sources:
        missing = [image_path for image_path, _, _ in rows if image_path not in sources]
        if missing:
            fail(
                f"--require-image-sources: {len(missing)} manifest row(s) not"
                f" covered by --image-source: {', '.join(missing)}"
            )

    direction = f"{len(rows)} vendored file(s)"
    if sources:
        direction += f" + {len(sources)} image source(s)"
    print(f"vendored-drift check ok: {direction} match the manifest")


# A guard that has never been seen to fail is not evidence. The self-test
# re-proves every red direction on every ci-local run, against synthetic
# trees, through the same CLI the justfile and the workflow invoke —
# subprocess with cwd set, exactly how production calls it.

SELF_TEST_HEADER = (
    "// Vendored from the pinned gate image.\n"
    "//\n"
    "// === vendored-source-begin ===\n"
)


def _self_test_case(
    script: Path, tree: Path, name: str, args: list[str], *, expect_red: bool
) -> None:
    proc = subprocess.run(  # nosemgrep -- audited: dangerous-subprocess-use-audit
        [sys.executable, str(script), *args],
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
    """Re-prove every red direction against synthetic trees through the CLI."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    def run_case(tree: Path, name: str, args: list[str], *, expect_red: bool) -> None:
        ran.append(name)
        _self_test_case(script, tree, name, args, expect_red=expect_red)

    with tempfile.TemporaryDirectory(prefix="vendored-selftest-") as tmp:
        root = Path(tmp)
        counter = [0]

        def make_tree(body: bytes, *, row_sha: str | None = None) -> Path:
            counter[0] += 1
            tree = root / f"tree-{counter[0]}"
            (tree / "scripts").mkdir(parents=True)
            (tree / "web").mkdir()
            (tree / "web" / "gate-eslint.config.mjs").write_bytes(
                SELF_TEST_HEADER.encode("utf-8") + body
            )
            digest = row_sha if row_sha is not None else sha256_bytes(body)
            (tree / "scripts" / "gate_vendored.tsv").write_text(
                "etc/gate/x.tmpl\tweb/gate-eslint.config.mjs\t" + digest + "\n",
                encoding="utf-8",
            )
            return tree

        # Green: vendored file matches the recorded hash.
        green = make_tree(b"body\n")
        run_case(green, "green", [], expect_red=False)

        # Red: the vendored file is missing.
        tree = make_tree(b"body\n")
        (tree / "web" / "gate-eslint.config.mjs").unlink()
        run_case(tree, "missing-vendored-file", [], expect_red=True)

        # Red: the marker is missing (not the file fetch_gate_file writes).
        tree = make_tree(b"body\n")
        (tree / "web" / "gate-eslint.config.mjs").write_bytes(b"// no marker\nbody\n")
        run_case(tree, "missing-marker", [], expect_red=True)

        # Red: bytes below the marker were edited.
        tree = make_tree(b"body\n")
        (tree / "web" / "gate-eslint.config.mjs").write_bytes(
            SELF_TEST_HEADER.encode("utf-8") + b"edited\n"
        )
        run_case(tree, "body-drift", [], expect_red=True)

        # Green: a provenance-comment edit ABOVE the marker is not drift.
        tree = make_tree(b"body\n")
        (tree / "web" / "gate-eslint.config.mjs").write_bytes(
            b"// Vendored (edited provenance only).\n"
            b"// === vendored-source-begin ===\n"
            b"body\n"
        )
        run_case(tree, "provenance-edit-ok", [], expect_red=False)

        # Red: manifest is missing.
        tree = make_tree(b"body\n")
        (tree / "scripts" / "gate_vendored.tsv").unlink()
        run_case(tree, "missing-manifest", [], expect_red=True)

        # Red: a malformed row (2 fields).
        tree = make_tree(b"body\n")
        (tree / "scripts" / "gate_vendored.tsv").write_text(
            "etc/gate/x.tmpl\tweb/gate-eslint.config.mjs\n", encoding="utf-8"
        )
        run_case(tree, "malformed-row", [], expect_red=True)

        # Red: a row whose sha256 is not hex.
        tree = make_tree(b"body\n", row_sha="z" * 64)
        run_case(tree, "bad-sha", [], expect_red=True)

        # Red: the same image path is declared twice.
        tree = make_tree(b"body\n")
        (tree / "scripts" / "gate_vendored.tsv").write_text(
            "etc/gate/x.tmpl\tweb/gate-eslint.config.mjs\t" + sha256_bytes(b"body\n")
            + "\netc/gate/x.tmpl\tweb/other.tmpl\t" + sha256_bytes(b"body\n") + "\n",
            encoding="utf-8",
        )
        run_case(tree, "duplicate-image-path", [], expect_red=True)

        # Red: a manifest with only comments parses to zero rows.
        tree = make_tree(b"body\n")
        (tree / "scripts" / "gate_vendored.tsv").write_text(
            "# only a comment, no data rows\n", encoding="utf-8"
        )
        run_case(tree, "zero-rows", [], expect_red=True)

        # Image-source direction. The extracted image file is the raw source
        # (no header/marker), so its full hash equals the recorded one.
        img = root / "image.tmpl"
        img.write_bytes(b"body\n")
        run_case(
            green,
            "image-source-green",
            ["--image-source", f"etc/gate/x.tmpl={img}"],
            expect_red=False,
        )
        run_case(
            green,
            "require-image-sources-green",
            ["--require-image-sources", "--image-source", f"etc/gate/x.tmpl={img}"],
            expect_red=False,
        )
        img.write_bytes(b"other\n")
        run_case(
            green,
            "image-source-drift",
            ["--image-source", f"etc/gate/x.tmpl={img}"],
            expect_red=True,
        )
        # The extracted image file must exist (a matching key, a missing file).
        run_case(
            green,
            "image-source-missing-file",
            ["--image-source", "etc/gate/x.tmpl=/nonexistent"],
            expect_red=True,
        )
        # A key with an existing file but no matching manifest row.
        run_case(
            green,
            "image-source-unknown-key",
            ["--image-source", f"etc/gate/nope={img}"],
            expect_red=True,
        )
        # --image-source that is not KEY=PATH.
        run_case(
            green,
            "image-source-not-key-path",
            ["--image-source", "notkeypath"],
            expect_red=True,
        )
        # --image-source with an empty key or path.
        run_case(
            green,
            "image-source-empty-key",
            ["--image-source", "=path"],
            expect_red=True,
        )
        # --require-image-sources reds on an uncovered row.
        run_case(
            green,
            "require-image-sources-missing",
            ["--require-image-sources"],
            expect_red=True,
        )

    print(f"vendored-drift self-test: {len(ran)} cases, all pass")


if __name__ == "__main__":
    main()
