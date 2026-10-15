#!/usr/bin/env python3
"""fetch-gate-file: extract a named path from the pinned gate image.

The mechanism behind the vendored gate config (gh#314). The gate image is
pinned by digest at `.github/workflows/gate.yml` (`env.GATE_IMAGE`); this
script reads that digest — never a second copy — and pulls the named file
out of the image's layers over the registry HTTP API, so a vendored copy is
produced by the same authority the gate actually runs, instead of by hand.

Stdlib only (urllib, tarfile, hashlib): same discipline as
scripts/check_json_dup_keys.py and scripts/ci_drift_check.py. Fails closed —
a missing digest, an unparseable manifest, a path the image does not carry,
or any network/parse failure exits non-zero with a diagnostic, never a
silent skip.

The primary target today is the node eslint flat config:

    etc/grizzly-gate/config/languages/node/eslint.config.mjs.tmpl

The image ships it as a `.tmpl` (not a live `.mjs`); the harness copies it
verbatim to `eslint.config.mjs` beside its own node_modules at run time (see
the file's own header and ADR-033). This script vendors that byte stream
into `web/gate-eslint.config.mjs.tmpl` — the same `.tmpl` shape, so neither
the gate's honest-map walk nor `node:eslint` sees a live `.mjs` — with a
short provenance header above a marker line; scripts/check_gate_vendored.py
hashes everything below the marker and reds on drift, and the justfile
`web-eslint` leg materializes it into `web/node_modules/` at run time
(auto-ignored by ESLint 9) and removes it after. The mechanism generalizes
to any path the image carries — gh#295 (the semgrep rule directory) is the
next consumer, same call shape with a different IMAGE_PATH and --output.

Usage:
    python3 scripts/fetch_gate_file.py IMAGE_PATH \
        --output web/gate-eslint.config.mjs.tmpl \
        --manifest scripts/gate_vendored.tsv

    python3 scripts/fetch_gate_file.py IMAGE_PATH --hash
    python3 scripts/fetch_gate_file.py --self-test

IMAGE_PATH is the path inside the image, without a leading slash (the layer
tar stores `etc/...`, not `/etc/...`).
"""

# ruff: noqa: S310, T201 -- a vendoring tool: urllib over fixed https
# endpoints (digest read from the repo's own workflow, no user input), and
# printing its verdict/hash is the product.

import argparse
import contextlib
import hashlib
import io
import json
import re
import sys
import tarfile
import urllib.error
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any, NoReturn, cast

GATE_WORKFLOW = Path(".github/workflows/gate.yml")
DEFAULT_DIGEST_FILE = GATE_WORKFLOW

# A digest is sha256:<64 lowercase hex chars> — checked, never trusted.
SHA256_HEX_LEN = 64

# The registry the digest names when the ref has no host (Docker Hub).
DOCKER_HUB_REGISTRY = "registry-1.docker.io"
DOCKER_HUB_AUTH = "https://auth.docker.io/token"

# The marker separating the provenance header from the vendored bytes.
# scripts/check_gate_vendored.py hashes everything below this line; the two
# copies must stay byte-identical or the check fails closed on a marker it
# cannot find.
VENDOR_MARKER = "// === vendored-source-begin ==="

# Media types for the two manifest shapes the registry can return.
INDEX_TYPES = (
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
)
MANIFEST_TYPES = (
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
)
ACCEPT = ", ".join((*INDEX_TYPES, *MANIFEST_TYPES))


@dataclass(frozen=True)
class ImageRef:
    """A parsed image reference: where and what to pull."""

    registry: str
    repository: str
    digest: str


def fail(message: str) -> NoReturn:
    """Print the diagnostic and exit non-zero — the only red this tool knows."""
    print(f"fetch-gate-file: {message}", file=sys.stderr)
    sys.exit(1)


def read_gate_image_ref(text: str) -> str | None:
    """Return the GATE_IMAGE value (name@sha256:…) from workflow text, or None.

    The digest is named once, in the workflow; this is the single reader of
    it, so a bump edits one line and every consumer follows.
    """
    match = re.search(r"^\s*GATE_IMAGE:\s*(\S+)$", text, re.MULTILINE)
    return match.group(1) if match else None


def parse_image_ref(ref: str) -> ImageRef:
    """Parse a `name[@digest]` image ref into registry/repository/digest.

    Supports `<repo>@sha256:…`, `<host>/<repo>@sha256:…` and the Docker Hub
    short forms (`ubuntu` -> `library/ubuntu`). A tag (`:latest`) is refused:
    a vendored copy of a moving tag certifies nothing, so only digests are
    acceptable here.
    """
    name, at, digest = ref.rpartition("@")
    if not at or not digest.startswith("sha256:"):
        fail(
            f"image ref '{ref}' is not digest-pinned — the vendoring source"
            " must be a digest (name@sha256:…), never a tag"
        )
    hexdigest = digest[len("sha256:") :]
    if len(hexdigest) != SHA256_HEX_LEN or any(
        c not in "0123456789abcdef" for c in hexdigest
    ):
        fail(
            f"image ref '{ref}' has a malformed digest — expected"
            " sha256:<64 hex chars>, never a tag or a shortened hash"
        )
    first, slash, rest = name.partition("/")
    if "." in first or ":" in first or first == "localhost":
        # A host was named (ghcr.io/…, localhost:5000/…).
        if not rest:
            fail(f"image ref '{ref}' names a host but no repository")
        return ImageRef(registry=first, repository=rest, digest=digest)
    if not slash:
        return ImageRef(
            registry=DOCKER_HUB_REGISTRY, repository=f"library/{name}", digest=digest
        )
    return ImageRef(registry=DOCKER_HUB_REGISTRY, repository=name, digest=digest)


def _http_get(url: str, token: str | None = None) -> bytes:
    """GET `url` (bearer token optional), following redirects safely.

    The token rides as an unredirected header: urllib sends it on the first
    request only and its redirect handler drops it on any redirect, so a
    blob GET that 307s to a CDN never forwards the bearer token cross-host.
    """
    request = urllib.request.Request(url, headers={"Accept": "*/*"})
    if token:
        request.add_unredirected_header("Authorization", f"Bearer {token}")
    try:
        # Audited: urllib over fixed https endpoints — the digest is read from
        # the repo's own workflow, never user input; same as the module header.
        with urllib.request.urlopen(request, timeout=120) as response:  # nosemgrep
            return cast("bytes", response.read())
    except urllib.error.HTTPError as exc:
        fail(f"GET {url} -> HTTP {exc.code}")
    except urllib.error.URLError as exc:
        fail(f"GET {url} failed: {exc}")


def registry_token(ref: ImageRef) -> str:
    """Fetch an anonymous pull token for the named repository."""
    if ref.registry == DOCKER_HUB_REGISTRY:
        auth_url = (
            f"{DOCKER_HUB_AUTH}?service=registry.docker.io"
            f"&scope=repository:{ref.repository}:pull"
        )
    else:
        auth_url = f"https://{ref.registry}/token?scope=repository:{ref.repository}:pull"
    body = _fetch_json(auth_url)
    token = body.get("token") or body.get("access_token")
    if not isinstance(token, str):
        fail(f"no token returned by {auth_url}")
    return token


def _fetch_json(url: str, token: str | None = None) -> dict[str, Any]:
    """GET `url` (bearer-authenticated) and return the parsed JSON object."""
    return cast("dict[str, Any]", json.loads(_http_get(url, token).decode("utf-8")))


def _manifest_blob_url(ref: ImageRef) -> str:
    return f"https://{ref.registry}/v2/{ref.repository}/manifests/{ref.digest}"


def resolve_manifest(ref: ImageRef, token: str) -> dict[str, Any]:
    """Fetch the manifest for `ref.digest`, descending an index to amd64/linux.

    The digest may name an OCI image index (multi-arch); the layers live in
    a per-platform manifest, so pick linux/amd64 (what the gate workflow's
    `ubuntu-latest` runner and the bench run) and fall back to the first
    entry otherwise.
    """
    index = _fetch_json(_manifest_blob_url(ref), token)
    if index.get("mediaType") in INDEX_TYPES:
        entries = index.get("manifests", [])
        if not entries:
            fail(f"image index {ref.digest} has no manifests")
        chosen = next(
            (
                entry
                for entry in entries
                if entry.get("platform", {}).get("os") == "linux"
                and entry.get("platform", {}).get("architecture") == "amd64"
            ),
            entries[0],
        )
        child_digest = chosen.get("digest")
        if not child_digest:
            fail(f"image index entry has no digest: {chosen!r}")
        child = ImageRef(
            registry=ref.registry, repository=ref.repository, digest=child_digest
        )
        return resolve_manifest(child, token)
    if index.get("mediaType") not in MANIFEST_TYPES:
        fail(
            f"manifest {ref.digest} has unexpected mediaType {index.get('mediaType')!r}"
        )
    return index


def search_tar_bytes(blob: bytes, target: str) -> bytes | None:
    """Return the content of `target` inside a gzipped layer tar, or None.

    Normalizes member names (leading `./` and `/` both occur) and refuses
    directory/symlink members — the vendored copy must be a regular file's
    bytes, not a link to be resolved against a tree that does not exist.
    """
    normalized_target = target.removeprefix("./").removeprefix("/")
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
        for member in archive.getmembers():
            name = member.name.removeprefix("./").removeprefix("/")
            if name == normalized_target:
                if not member.isfile():
                    fail(f"path '{target}' in the image is not a regular file")
                extracted = archive.extractfile(member)
                if extracted is None:
                    fail(f"path '{target}' could not be read")
                return extracted.read()
    return None


def search_layers(
    digest: str,
    image_path: str,
    layers: list[dict[str, Any]],
    blob_getter: Callable[[str], bytes],
) -> bytes:
    """Return `image_path`'s bytes, searching `layers` topmost-first.

    Layers are read in reverse (topmost first) so a file overwritten by a
    later layer is found in its final state; the first hit wins. The blob
    fetch is injected so the overlay order is provable without a registry.
    """
    if not layers:
        fail(f"manifest {digest} declares no layers")
    for layer in reversed(layers):
        layer_digest = layer.get("digest")
        if not layer_digest:
            fail(f"layer without digest: {layer!r}")
        content = search_tar_bytes(blob_getter(layer_digest), image_path)
        if content is not None:
            return content
    fail(f"path '{image_path}' not found in image {digest}")


def extract_from_image(ref: ImageRef, image_path: str) -> bytes:
    """Pull `image_path` out of the pinned image's layers over the registry."""
    token = registry_token(ref)
    manifest = resolve_manifest(ref, token)
    return search_layers(
        ref.digest,
        image_path,
        manifest.get("layers", []),
        lambda layer_digest: _http_get(
            f"https://{ref.registry}/v2/{ref.repository}/blobs/{layer_digest}",
            token,
        ),
    )


def vendoring_header(image_path: str, digest: str) -> str:
    """Return the provenance header written above the marker in the vendored file."""
    return (
        "// Vendored from the pinned gate image by scripts/fetch_gate_file.py — do not\n"
        "// hand-edit. scripts/check_gate_vendored.py hashes everything below the marker\n"
        "// line and `just check-vendored` reds on drift.\n"
        "//\n"
        f"// Source: {image_path} in GATE_IMAGE ({digest[:16]}…), read from\n"
        "// .github/workflows/gate.yml — the digest is named once, there.\n"
        "// Rendering contract: the image ships this file as a `.tmpl`, NOT a live\n"
        "// `.mjs` — the harness copies it verbatim to `eslint.config.mjs` beside the\n"
        "// gate's own node_modules at run time (see the image's header below, ADR-033).\n"
        "// The `{config}` / `{tsconfig}` / `{skip_dirs}` placeholders live in the\n"
        "// adapter's manifest.toml, not here; this file reads its two runtime knobs\n"
        "// from process.env — GATE_TSCONFIG and GATE_SKIP_DIRS — which the justfile\n"
        "// `web-eslint` leg supplies to reproduce the image's adapter contract.\n"
        "//\n"
        "// Regenerate after a digest bump:\n"
        f"//   python3 scripts/fetch_gate_file.py {image_path} \\\n"
        "//     --output web/gate-eslint.config.mjs.tmpl --manifest scripts/gate_vendored.tsv\n"
        f"{VENDOR_MARKER}\n"
    )


def compose_vendored_file(image_path: str, digest: str, source: bytes) -> bytes:
    """Header + marker + raw source bytes — the exact vendored file content."""
    return vendoring_header(image_path, digest).encode("utf-8") + source


# --- manifest handling ------------------------------------------------------

MANIFEST_FIELDS = 3


def _split_header_and_rows(text: str) -> tuple[str, list[list[str]]]:
    """Split a manifest into its leading comment block and its data rows."""
    lines = text.splitlines()
    header_lines: list[str] = []
    rows: list[list[str]] = []
    in_data = False
    for line in lines:
        stripped = line.strip()
        if not in_data and (not stripped or stripped.startswith("#")):
            header_lines.append(line)
            continue
        in_data = True
        if not stripped or stripped.startswith("#"):
            continue  # a trailing/interstitial comment is dropped on rewrite
        fields = line.split("\t")
        if len(fields) != MANIFEST_FIELDS:
            fail(
                f"manifest row has {len(fields)} fields, expected"
                f" {MANIFEST_FIELDS}: {line!r}"
            )
        rows.append(fields)
    return "\n".join(header_lines), rows


def upsert_manifest(
    manifest_path: Path, image_path: str, vendored_path: str, sha256: str
) -> None:
    """Record (or refresh) the row for `image_path` in the vendored manifest."""
    if manifest_path.is_file():
        header, rows = _split_header_and_rows(
            manifest_path.read_text(encoding="utf-8")
        )
    else:
        header = (
            "# Vendored gate-file manifest. One row per vendored file, three\n"
            "# tab-separated fields: <image-path> <vendored-path> <sha256>\n"
            "# (sha256 of the image's raw bytes, i.e. everything below the\n"
            "# marker line in the vendored file). Written by\n"
            "# scripts/fetch_gate_file.py; read by scripts/check_gate_vendored.py."
        )
        rows = []
    rows = [row for row in rows if row[0] != image_path]
    rows.append([image_path, vendored_path, sha256])
    rows.sort(key=lambda row: row[0])
    body = "\n".join("\t".join(row) for row in rows)
    manifest_path.write_text(
        (header + "\n" + body + "\n" if header else body + "\n"),
        encoding="utf-8",
    )


def _make_layer(entries: dict[str, bytes]) -> bytes:
    """Build a gzip tar with the given name->bytes entries (self-test fixture)."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as archive:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def _check(ran: list[str], name: str, *, condition: bool) -> None:
    """Record one self-test check; exit 1 on failure."""
    ran.append(name)
    if not condition:
        print(f"fetch-gate-file self-test [{name}]: FAILED", file=sys.stderr)
        sys.exit(1)


def _self_test_ref_cases(ran: list[str], digest64: str) -> None:
    # Digest is read from the workflow's own text, and a tag is refused.
    _check(
        ran,
        "read-ref",
        condition=read_gate_image_ref("env:\n  GATE_IMAGE: a/b@sha256:abc\n")
        == "a/b@sha256:abc",
    )
    _check(ran, "read-ref-missing", condition=read_gate_image_ref("env: {}\n") is None)
    hub = parse_image_ref(f"bearflinn/grizzly-gate@{digest64}")
    _check(
        ran,
        "ref-hub",
        condition=hub.registry == DOCKER_HUB_REGISTRY
        and hub.repository == "bearflinn/grizzly-gate"
        and hub.digest == digest64,
    )
    short = parse_image_ref(f"ubuntu@{digest64}")
    _check(ran, "ref-short", condition=short.repository == "library/ubuntu")
    hosted = parse_image_ref(f"ghcr.io/owner/repo@{digest64}")
    _check(
        ran,
        "ref-hosted",
        condition=hosted.registry == "ghcr.io" and hosted.repository == "owner/repo",
    )

    def refuses(ref: str) -> bool:
        with contextlib.redirect_stderr(io.StringIO()):
            try:
                parse_image_ref(ref)
            except SystemExit:
                return True
        return False

    _check(ran, "ref-tag-refused", condition=refuses("a/b:latest"))
    _check(ran, "ref-malformed-digest-refused", condition=refuses(f"a/b@sha256:{'a' * 7}"))


def _self_test_tar_cases(ran: list[str]) -> None:
    # Tar search: found, name-normalized, missing, and non-regular-file.
    layer = _make_layer({"etc/gate/x.tmpl": b"hello"})
    _check(ran, "tar-found", condition=search_tar_bytes(layer, "etc/gate/x.tmpl") == b"hello")
    _check(
        ran,
        "tar-dot-normalized",
        condition=search_tar_bytes(layer, "./etc/gate/x.tmpl") == b"hello",
    )
    _check(
        ran,
        "tar-slash-normalized",
        condition=search_tar_bytes(layer, "/etc/gate/x.tmpl") == b"hello",
    )
    _check(ran, "tar-missing", condition=search_tar_bytes(layer, "etc/gate/nope") is None)

    def refuse_non_regular() -> bool:
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w:gz") as archive:
            info = tarfile.TarInfo("etc/gate/x.tmpl")
            info.type = tarfile.DIRTYPE
            archive.addfile(info)
        with contextlib.redirect_stderr(io.StringIO()):
            try:
                search_tar_bytes(buf.getvalue(), "etc/gate/x.tmpl")
            except SystemExit:
                return True
        return False

    _check(ran, "tar-non-regular-refused", condition=refuse_non_regular())


def _self_test_overlay_case(ran: list[str], digest64: str) -> None:
    # Overlay: a top layer's copy wins over a lower layer's — through the
    # production search_layers path (reversed(layers), first hit wins).
    bottom = _make_layer({"etc/gate/x.tmpl": b"old"})
    top = _make_layer({"etc/gate/x.tmpl": b"new"})
    blobs = {"sha256:bottom": bottom, "sha256:top": top}
    found = search_layers(
        digest64,
        "etc/gate/x.tmpl",
        [{"digest": "sha256:bottom"}, {"digest": "sha256:top"}],
        lambda d: blobs[d],
    )
    _check(ran, "overlay-top-wins", condition=found == b"new")


def _self_test_compose_cases(ran: list[str], digest64: str) -> None:
    # Vendored file composition: header + marker + exact source bytes.
    composed = compose_vendored_file("etc/gate/x.tmpl", digest64, b"body")
    _check(ran, "compose-marker", condition=VENDOR_MARKER.encode("utf-8") + b"\n" in composed)
    _check(ran, "compose-body", condition=composed.endswith(b"body"))
    body_after_marker = composed.split(VENDOR_MARKER.encode("utf-8") + b"\n", 1)[1]
    _check(
        ran,
        "compose-hash-matches-source",
        condition=hashlib.sha256(b"body").hexdigest()
        == hashlib.sha256(body_after_marker).hexdigest(),
    )


def _self_test() -> None:
    """Re-prove the pure paths: ref parsing, tar search, overlay, manifest.

    No network — the registry round-trip is exercised by the real run, and
    the pure pieces it depends on are re-proven here.
    """
    ran: list[str] = []
    digest64 = "sha256:" + "a" * 64
    _self_test_ref_cases(ran, digest64)
    _self_test_tar_cases(ran)
    _self_test_overlay_case(ran, digest64)
    _self_test_compose_cases(ran, digest64)
    print(f"fetch-gate-file self-test: {len(ran)} checks, all pass")


def main() -> None:
    """Parse arguments and vend the named path out of the pinned image."""
    parser = argparse.ArgumentParser(
        description="extract a named path from the pinned gate image"
    )
    parser.add_argument("image_path", nargs="?", help="path inside the image, no leading slash")
    parser.add_argument("--output", type=Path, help="write the vendored file here")
    parser.add_argument(
        "--manifest",
        type=Path,
        help="record the vendored file's hash here (requires --output)",
    )
    parser.add_argument(
        "--hash", action="store_true", help="print the sha256 of the extracted bytes and exit"
    )
    parser.add_argument(
        "--digest-file",
        type=Path,
        default=DEFAULT_DIGEST_FILE,
        help="workflow file to read GATE_IMAGE from (default: .github/workflows/gate.yml)",
    )
    parser.add_argument("--self-test", action="store_true", help="re-prove the pure paths and exit")
    args = parser.parse_args()

    if args.self_test:
        _self_test()
        return

    if not args.image_path:
        fail("IMAGE_PATH is required (or pass --self-test)")
    if args.manifest and not args.output:
        fail("--manifest requires --output (the vendored path the hash belongs to)")

    if not args.digest_file.is_file():
        fail(f"digest file {args.digest_file} not found — run from the repo root")
    ref_text = read_gate_image_ref(args.digest_file.read_text(encoding="utf-8"))
    if not ref_text:
        fail(f"no GATE_IMAGE line in {args.digest_file}")
    ref = parse_image_ref(ref_text)

    source = extract_from_image(ref, args.image_path)
    digest = hashlib.sha256(source).hexdigest()

    if args.hash:
        print(digest)
        return
    if args.output:
        args.output.write_bytes(compose_vendored_file(args.image_path, ref.digest, source))
        if args.manifest:
            upsert_manifest(
                args.manifest,
                args.image_path,
                str(args.output),
                digest,
            )
        print(
            f"vendored {args.image_path} -> {args.output}"
            f" (sha256 {digest[:16]}…, {len(source)} bytes)"
        )
        return
    # No --output/--hash: emit the raw bytes, the same shape the original
    # extraction used for inspection.
    sys.stdout.buffer.write(source)


if __name__ == "__main__":
    main()
