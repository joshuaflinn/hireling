#!/usr/bin/env python3
"""check-gate-vendored: the vendored gate config has not drifted (gh#314).

The class fault this closes: a hand-copied mirror of the gate image's
eslint config drifts the moment the image digest moves, and nothing reds
because the copy is just text no check re-reads against the pinned digest.
Two directions, both failing closed:

1. Offline (default): each vendored path named by scripts/gate_vendored.tsv
   still hashes to what the manifest records. File rows hash the bytes below
   the `// === vendored-source-begin ===` marker — the image's own source,
   verbatim — so a hand edit above the marker (a provenance comment) is fine
   and a hand edit below it reds. Directory rows (both fields end in `/`)
   hash the canonical tree digest of the vendored directory. An absent
   directory reds: it is git-ignored, so the justfile materializes it from
   the pinned digest (`check-vendored` and `scan-semgrep` self-provision;
   `vendor-semgrep` re-vendors and records the digest), and the tree is the
   provenance `just scan-semgrep` depends on
   — a hand-made copy left stale reads green here while the scan runs
   against the wrong rules. `--allow-unvendored` is the only opt-out, and it
   says so in the output; it exists for a machine that cannot reach the
   registry (CONTRIBUTOR.md), because the vendor step and the drift check are
   the same fetch. A present-but-wrong tree reds either way.

2. --image-source KEY=PATH (CI-side): the image itself still carries those
   bytes. The gate workflow extracts the file (or, for a directory row, the
   whole subtree) from the pinned digest (the digest is named once, at
   .github/workflows/gate.yml) and passes it here;
   a digest bump that moved the config without re-vendoring reds this
   direction instead of silently stranding the vendored copy.
   `--require-image-sources` (also CI-side) reds when a manifest row is not
   covered by an `--image-source`, so the image-side extraction cannot
   silently skip a row.

The manifest is written by scripts/fetch_gate_file.py (the producer); this
is the verifier. One row per vendored file or directory, three tab-separated
fields: <image-path> <vendored-path> <sha256>. A directory row is signalled
by a trailing slash on BOTH fields; a file row by neither.

Zero dependencies, stdlib only — same discipline as the guardrails it sits
beside. A guardrail that skips is not a guardrail: a missing manifest, a
missing vendored file, a missing marker, a drifted directory, or a malformed
row all fail loudly.
"""

# ruff: noqa: T201, S603 -- a CLI guardrail: printing its verdict is the
# product, and the self-test spawns this very script (fixed argv, no shell).

import argparse
import hashlib
import shutil
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
        if image_path.endswith("/") != vendored_path.endswith("/"):
            fail(
                f"{MANIFEST}:{lineno}: a directory row must carry a trailing"
                f" slash on BOTH fields and a file row on NEITHER — got"
                f" image_path={image_path!r} vendored_path={vendored_path!r}"
            )
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


def is_directory_row(image_path: str, vendored_path: str) -> bool:
    """Return True when both fields end in a slash (a directory row)."""
    return image_path.endswith("/") and vendored_path.endswith("/")


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


def directory_digest(path: Path) -> str:
    """Return the canonical tree digest over `path`.

    Byte-identical to scripts/fetch_gate_file.py's directory_digest over
    {relpath: bytes}: sorted relpaths, one `relpath<TAB>sha256(content)` line
    each, the joined stream hashed. Fails closed on a missing/empty directory,
    a symlink, or any non-regular file — a tree the image did not ship as such.
    """
    if not path.is_dir():
        fail(f"vendored directory {path} not found or not a directory")
    entries: dict[str, bytes] = {}
    for file in path.rglob("*"):
        if file.is_symlink():
            fail(f"vendored directory {path} contains a symlink ({file}) — refusing to pass")
        if file.is_file():
            entries[file.relative_to(path).as_posix()] = file.read_bytes()
        elif not file.is_dir():
            fail(f"vendored directory {path} contains a non-regular file ({file})")
    if not entries:
        fail(f"vendored directory {path} is empty — refusing to pass")
    lines = "".join(
        f"{relpath}\t{sha256_bytes(entries[relpath])}\n" for relpath in sorted(entries)
    )
    return sha256_bytes(lines.encode("utf-8"))


def check_offline(
    rows: list[tuple[str, str, str]], *, allow_unvendored: bool
) -> None:
    """Direction 1: every vendored path hashes to its recorded sha256."""
    for image_path, vendored_path, sha256 in rows:
        if is_directory_row(image_path, vendored_path):
            vendored_dir = Path(vendored_path)
            if not vendored_dir.exists():
                if allow_unvendored:
                    print(
                        f"vendored-drift check: note: {vendored_path} not vendored"
                        " — tolerated because --allow-unvendored says the registry"
                        " is unreachable here; 'just scan-semgrep' still self-"
                        " vendors from the pinned image and reds without the"
                        " registry, so the scan cannot skip"
                    )
                    continue
                fail(
                    f"vendored directory {vendored_path} is absent — run 'just"
                    " vendor-semgrep'. The tree is git-ignored, so an absent one"
                    " means it was never fetched on this machine, and a hand-made"
                    " copy would read green here while 'just scan-semgrep'"
                    " scanned the wrong rules. --allow-unvendored opts out (a"
                    " machine that cannot reach the registry; CONTRIBUTOR.md)."
                )
            actual = directory_digest(vendored_dir)
        else:
            actual = sha256_bytes(vendored_body(Path(vendored_path)))
        if actual != sha256:
            if is_directory_row(image_path, vendored_path):
                hint = "re-vendor with scripts/fetch_gate_file.py"
            else:
                hint = (
                    "re-vendor with scripts/fetch_gate_file.py (or fix the"
                    " hand edit below the marker)"
                )
            fail(
                f"{vendored_path} has sha256 {actual}, manifest records {sha256}"
                f" for {image_path} — the vendored copy drifted from the pinned"
                f" image; {hint}"
            )


def check_image_sources(
    rows: list[tuple[str, str, str]], sources: dict[str, Path]
) -> None:
    """Direction 2: the image itself still carries the recorded bytes."""
    for key, path in sources.items():
        row = next((r for r in rows if r[0] == key), None)
        if row is None:
            fail(
                f"image source key '{key}' matches no row in {MANIFEST} — the"
                " manifest and the CI extraction disagree"
            )
        if is_directory_row(row[0], row[1]):
            if not path.is_dir():
                fail(
                    f"image source {path} for '{key}' is not a directory —"
                    " refusing to pass"
                )
            actual = directory_digest(path)
        else:
            if not path.is_file():
                fail(f"image source {path} for '{key}' not found — refusing to pass")
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
        "--allow-unvendored",
        action="store_true",
        help=(
            "tolerate an absent vendored directory (registry unreachable;"
            " scan-semgrep still fails)"
        ),
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
    check_offline(rows, allow_unvendored=args.allow_unvendored)

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

    direction = f"{len(rows)} vendored path(s)"
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


def _run_self_test_case(
    script: Path, ran: list[str], tree: Path, args: list[str], *, expect_red: bool
) -> None:
    """Run one CLI case through _self_test_case and record it in `ran`."""
    ran.append(tree.name)
    _self_test_case(script, tree, tree.name, args, expect_red=expect_red)


def _make_self_test_tree(
    root: Path, name: str, body: bytes, *, row_sha: str | None = None
) -> Path:
    """Build a synthetic tree with one vendored file and a matching manifest row."""
    tree = root / name
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


def _offline_self_test(script: Path, ran: list[str], root: Path) -> Path:
    """Run the offline direction's cases; return the shared green tree."""
    # Green: vendored file matches the recorded hash.
    green = _make_self_test_tree(root, "green", b"body\n")
    _run_self_test_case(script, ran, green, [], expect_red=False)

    # Red: the vendored file is missing.
    tree = _make_self_test_tree(root, "missing-vendored-file", b"body\n")
    (tree / "web" / "gate-eslint.config.mjs").unlink()
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: the marker is missing (not the file fetch_gate_file writes).
    tree = _make_self_test_tree(root, "missing-marker", b"body\n")
    (tree / "web" / "gate-eslint.config.mjs").write_bytes(b"// no marker\nbody\n")
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: bytes below the marker were edited.
    tree = _make_self_test_tree(root, "body-drift", b"body\n")
    (tree / "web" / "gate-eslint.config.mjs").write_bytes(
        SELF_TEST_HEADER.encode("utf-8") + b"edited\n"
    )
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Green: a provenance-comment edit ABOVE the marker is not drift.
    tree = _make_self_test_tree(root, "provenance-edit-ok", b"body\n")
    (tree / "web" / "gate-eslint.config.mjs").write_bytes(
        b"// Vendored (edited provenance only).\n"
        b"// === vendored-source-begin ===\n"
        b"body\n"
    )
    _run_self_test_case(script, ran, tree, [], expect_red=False)

    # Red: manifest is missing.
    tree = _make_self_test_tree(root, "missing-manifest", b"body\n")
    (tree / "scripts" / "gate_vendored.tsv").unlink()
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: a malformed row (2 fields).
    tree = _make_self_test_tree(root, "malformed-row", b"body\n")
    (tree / "scripts" / "gate_vendored.tsv").write_text(
        "etc/gate/x.tmpl\tweb/gate-eslint.config.mjs\n", encoding="utf-8"
    )
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: a row whose sha256 is not hex.
    tree = _make_self_test_tree(root, "bad-sha", b"body\n", row_sha="z" * 64)
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: the same image path is declared twice.
    tree = _make_self_test_tree(root, "duplicate-image-path", b"body\n")
    (tree / "scripts" / "gate_vendored.tsv").write_text(
        "etc/gate/x.tmpl\tweb/gate-eslint.config.mjs\t" + sha256_bytes(b"body\n")
        + "\netc/gate/x.tmpl\tweb/other.tmpl\t" + sha256_bytes(b"body\n") + "\n",
        encoding="utf-8",
    )
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: a manifest with only comments parses to zero rows.
    tree = _make_self_test_tree(root, "zero-rows", b"body\n")
    (tree / "scripts" / "gate_vendored.tsv").write_text(
        "# only a comment, no data rows\n", encoding="utf-8"
    )
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    return green


def _image_source_self_test(
    script: Path, ran: list[str], root: Path, green: Path
) -> None:
    """Run the --image-source direction's cases."""
    # The extracted image file is the raw source (no header/marker), so its
    # full hash equals the recorded one.
    img = root / "image.tmpl"
    img.write_bytes(b"body\n")
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/x.tmpl={img}"],
        expect_red=False,
    )
    _run_self_test_case(
        script,
        ran,
        green,
        ["--require-image-sources", "--image-source", f"etc/gate/x.tmpl={img}"],
        expect_red=False,
    )
    img.write_bytes(b"other\n")
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/x.tmpl={img}"],
        expect_red=True,
    )
    # The extracted image file must exist (a matching key, a missing file).
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", "etc/gate/x.tmpl=/nonexistent"],
        expect_red=True,
    )
    # A key with an existing file but no matching manifest row.
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/nope={img}"],
        expect_red=True,
    )
    # --image-source that is not KEY=PATH.
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", "notkeypath"],
        expect_red=True,
    )
    # --image-source with an empty key or path.
    _run_self_test_case(
        script, ran, green, ["--image-source", "=path"],
        expect_red=True,
    )
    # --require-image-sources reds on an uncovered row.
    _run_self_test_case(
        script, ran, green, ["--require-image-sources"],
        expect_red=True,
    )


def _tree_digest(files: dict[str, bytes]) -> str:
    """Compute the canonical directory digest the manifest records (test fixture)."""
    lines = "".join(f"{rel}\t{sha256_bytes(files[rel])}\n" for rel in sorted(files))
    return sha256_bytes(lines.encode("utf-8"))


def _make_directory_self_test_tree(
    root: Path, name: str, files: dict[str, bytes], *, row_sha: str | None = None
) -> Path:
    """Build a synthetic tree with one vendored directory and a matching row."""
    tree = root / name
    (tree / "scripts").mkdir(parents=True)
    rules = tree / "gate-semgrep-rules"
    rules.mkdir(parents=True)
    for rel, data in files.items():
        target = rules / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    digest = row_sha if row_sha is not None else _tree_digest(files)
    (tree / "scripts" / "gate_vendored.tsv").write_text(
        "etc/gate/rules/\tgate-semgrep-rules/\t" + digest + "\n",
        encoding="utf-8",
    )
    return tree


def _directory_offline_self_test(script: Path, ran: list[str], root: Path) -> Path:
    """Run the directory-row offline cases; return the shared green tree."""
    green = _make_directory_self_test_tree(
        root, "dir-green", {"a.yaml": b"one\n", "sub/b.yaml": b"two\n"}
    )
    _run_self_test_case(script, ran, green, [], expect_red=False)

    # Red: the directory is absent (never vendored) — the tree is provenance,
    # so absence is a hole, not a notice. Green only with the explicit opt-out.
    tree = _make_directory_self_test_tree(root, "dir-absent-red", {"a.yaml": b"one\n"})
    shutil.rmtree(tree / "gate-semgrep-rules")
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Green: absent, but the caller declared the registry unreachable.
    tree = _make_directory_self_test_tree(root, "dir-absent-opt-out", {"a.yaml": b"one\n"})
    shutil.rmtree(tree / "gate-semgrep-rules")
    _run_self_test_case(script, ran, tree, ["--allow-unvendored"], expect_red=False)

    # Red: a file inside the directory drifted.
    tree = _make_directory_self_test_tree(root, "dir-body-drift", {"a.yaml": b"one\n"})
    (tree / "gate-semgrep-rules" / "a.yaml").write_bytes(b"edited\n")
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: a symlink inside the directory (a tree the image did not ship).
    tree = _make_directory_self_test_tree(root, "dir-symlink", {"a.yaml": b"one\n"})
    (tree / "gate-semgrep-rules" / "link").symlink_to("a.yaml")
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    # Red: a mixed row — one field carries the trailing slash, the other not.
    tree = _make_directory_self_test_tree(root, "dir-mixed-row", {"a.yaml": b"one\n"})
    (tree / "scripts" / "gate_vendored.tsv").write_text(
        "etc/gate/rules/\tgate-semgrep-rules\t" + _tree_digest({"a.yaml": b"one\n"}) + "\n",
        encoding="utf-8",
    )
    _run_self_test_case(script, ran, tree, [], expect_red=True)

    return green


def _directory_image_source_self_test(
    script: Path, ran: list[str], root: Path, green: Path
) -> None:
    """Run the directory-row --image-source cases."""
    src = root / "image-rules"
    src.mkdir(parents=True)
    (src / "a.yaml").write_bytes(b"one\n")
    (src / "sub").mkdir()
    (src / "sub" / "b.yaml").write_bytes(b"two\n")
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/rules/={src}"],
        expect_red=False,
    )
    _run_self_test_case(
        script,
        ran,
        green,
        ["--require-image-sources", "--image-source", f"etc/gate/rules/={src}"],
        expect_red=False,
    )
    # Drift: a file in the image-source directory changed.
    (src / "a.yaml").write_bytes(b"other\n")
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/rules/={src}"],
        expect_red=True,
    )
    # The image-source key resolves to a file, not a directory.
    file_src = root / "image-file"
    file_src.write_bytes(b"not a tree\n")
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/rules/={file_src}"],
        expect_red=True,
    )
    # A directory image source for a key with no matching row.
    _run_self_test_case(
        script,
        ran,
        green,
        ["--image-source", f"etc/gate/nope/={src}"],
        expect_red=True,
    )


def self_test() -> None:
    """Re-prove every red direction against synthetic trees through the CLI."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    with tempfile.TemporaryDirectory(prefix="vendored-selftest-") as tmp:
        root = Path(tmp)
        green = _offline_self_test(script, ran, root)
        _image_source_self_test(script, ran, root, green)
        dir_green = _directory_offline_self_test(script, ran, root)
        _directory_image_source_self_test(script, ran, root, dir_green)

    print(f"vendored-drift self-test: {len(ran)} cases, all pass")


if __name__ == "__main__":
    main()
