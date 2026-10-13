#!/usr/bin/env python3
"""ci-drift: the guardrail guarding the guardrails (gh#151).

Three directions, all failing closed:

A. every ci-local leg has a CI home — an image mirror declared in
   scripts/gate_passes.tsv, or a guard step in the gate workflow
   (repo_side, derived from the workflow's own text; MOR-142/MOR-130:
   never a hand-kept literal);
B. every declared image pass is mirrored by a leg ci-local actually
   runs, or carries its written exemption (an empty reason does not
   even parse);
C. (--image-log, CI-side) the image's own stdout names nothing the
   declaration does not know — this is what makes the declaration
   survive a digest bump. A log with no pass tokens is a failure,
   not a pass.

Zero dependencies, stdlib only — same discipline as
scripts/check_json_dup_keys.py. A guardrail that skips is not a
guardrail: missing files and malformed rows fail loudly.
"""

# ruff: noqa: T201, S603 -- a CLI guardrail: printing its verdict is the
# product, and the self-test spawns this very script (fixed argv, no shell).

import argparse
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path
from typing import NoReturn

DECLARATION = Path("scripts/gate_passes.tsv")
JUSTFILE = Path("justfile")
WORKFLOW = Path(".github/workflows/gate.yml")

# Image check names look like node:tsc; just leg names like web-tsc.
# (Named *_CHECK_*, not *_PASS_*: bandit's S105 matches the variable NAME,
# and a regex constant is no more a password than a verdict line is.)
CHECK_NAME_RE = r"[a-z]+:[a-z0-9-]+"
LEG_NAME_RE = r"[\w-]+"
# A declaration row is pass, mirror-or-'-', reason — no more, no less.
DECLARATION_FIELDS = 3
# The image prints one verdict line per check: "[PASS] node:tsc".
IMAGE_LINE_RE = re.compile(r"\[(PASS|FAIL|SKIP)\] (" + CHECK_NAME_RE + r")")

SELF_TEST_TREE_JUSTFILE = "ci-local: alpha\n\nalpha:\n\ttrue\n"
SELF_TEST_TREE_WORKFLOW = (
    "jobs:\n  gate:\n    steps:\n      - name: guard\n        run: just alpha\n"
)



def fail(message: str) -> NoReturn:
    """Print the verdict to stderr and exit 1 — the only red this guard knows."""
    print(f"drift check: {message}", file=sys.stderr)
    sys.exit(1)


def load_declaration() -> dict[str, tuple[str, str]]:
    """Parse pass -> (mirror leg or '-', reason). Malformed rows are red."""
    if not DECLARATION.is_file():
        fail(
            f"{DECLARATION} not found — the declaration is the single authority"
            " for the image's pass list; refusing to pass without it"
        )
    passes: dict[str, tuple[str, str]] = {}
    for lineno, line in enumerate(DECLARATION.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != DECLARATION_FIELDS:
            fail(
                f"{DECLARATION}:{lineno}: expected {DECLARATION_FIELDS}"
                " tab-separated fields (pass, mirror-or-'-', reason),"
                f" got {len(fields)}"
            )
        name, mirror, reason = (field.strip() for field in fields)
        if not re.fullmatch(CHECK_NAME_RE, name):
            fail(f"{DECLARATION}:{lineno}: '{name}' is not an image check name ({CHECK_NAME_RE})")
        if name in passes:
            fail(f"{DECLARATION}:{lineno}: '{name}' is declared twice")
        if mirror != "-" and not re.fullmatch(LEG_NAME_RE, mirror):
            fail(
                f"{DECLARATION}:{lineno}: mirror '{mirror}' is not a just leg name ({LEG_NAME_RE})"
            )
        if not reason:
            fail(
                f"{DECLARATION}:{lineno}: empty reason — every row carries a"
                " written reason (a mirror note, or the exemption's why)"
            )
        passes[name] = (mirror, reason)
    if not passes:
        fail(
            f"{DECLARATION} parsed to zero passes — printing coverage over a hole"
            " is the fault this guards"
        )
    return passes


def load_ci_local_legs() -> list[str]:
    """Return the ci-local dependency line — the bench gate's own leg list."""
    if not JUSTFILE.is_file():
        fail(f"{JUSTFILE} not found — run from the repo root")
    match = re.search(r"^ci-local: *(.+)$", JUSTFILE.read_text(encoding="utf-8"), re.MULTILINE)
    if not match:
        fail("no ci-local recipe found in justfile")
    return match.group(1).split()


def derive_repo_side() -> set[str]:
    """Legs the gate workflow runs as guard steps, from the workflow text.

    A hand-kept literal certifies steps it never reads (MOR-142,
    MOR-130's fault class one storey up): delete a guard step and its leg
    drops out of this set, turning direction A red instead of printing
    coverage over a hole. Matches only single-line `run: just <leg>`
    steps, so any other spelling reads as ungated and fails closed. What
    the derivation still cannot see: textual presence is not execution
    (Thrane, MOR-130) — a guard step with `if: false` stays in this set
    while no longer running on PRs. No parse closes that hole; the
    deliberate-break run is the cover.
    """
    if not WORKFLOW.is_file():
        fail(f"{WORKFLOW} not found — run from the repo root")
    text = WORKFLOW.read_text(encoding="utf-8")
    return set(re.findall(r"^ *run: just ([\w-]+) *$", text, re.MULTILINE))


def main() -> None:
    """Check A and B against the repo; C additionally when given an image log."""
    parser = argparse.ArgumentParser(
        description="ci drift check: legs, declaration, and the image's own output agree"
    )
    parser.add_argument(
        "--image-log",
        type=Path,
        default=None,
        help="path to the gate image's captured stdout; enables direction C (CI-side)",
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

    passes = load_declaration()
    legs = load_ci_local_legs()
    repo_side = derive_repo_side()

    mirrors = {mirror for mirror, _ in passes.values() if mirror != "-"}

    # One bench leg claiming two image checks is a fat-fingered row, not a
    # mapping — fail and make the author write two legs or two reasons.
    mirror_counts = Counter(mirror for mirror, _ in passes.values() if mirror != "-")
    contested = sorted(mirror for mirror, count in mirror_counts.items() if count > 1)
    if contested:
        fail(
            "declaration mirror(s) claimed by more than one pass: "
            + ", ".join(contested)
            + " — one mirror leg per image pass"
        )

    # A. Every ci-local leg has a CI home.
    ungated = sorted(set(legs) - mirrors - repo_side)
    if ungated:
        fail(
            "ci-local leg(s) gated by no CI job: "
            + ", ".join(ungated)
            + " — add them to the guard steps in the gate workflow, or declare"
              f" the image pass's mirror in {DECLARATION}"
        )

    # B. Every declared pass is mirrored by a leg ci-local actually runs.
    phantom = sorted(
        f"{name} -> {mirror}"
        for name, (mirror, _) in passes.items()
        if mirror != "-" and mirror not in legs
    )
    if phantom:
        fail(
            "declaration mirror(s) naming legs ci-local does not run: "
            + ", ".join(phantom)
            + " — add the leg to ci-local or fix the mirror"
        )

    mirrored_count = len(mirrors)
    print(
        f"drift check ok: all {len(legs)} ci-local legs are gated;"
        f" {len(passes)} image passes declared"
        f" ({mirrored_count} mirrored, {len(passes) - mirrored_count} exempt)"
    )

    # C. The image's own stdout names nothing the declaration does not know.
    if args.image_log is not None:
        if not args.image_log.is_file():
            fail(
                f"image log {args.image_log} not found — cannot certify the"
                " declaration against the image"
            )
        found = IMAGE_LINE_RE.findall(args.image_log.read_text(encoding="utf-8", errors="replace"))
        names = sorted({name for _, name in found})
        if not names:
            fail(
                f"no [PASS]/[FAIL] pass tokens found in {args.image_log} — the"
                " image's output is empty or unreadable; failing closed rather"
                " than certifying an unseen run"
            )
        undeclared = [name for name in names if name not in passes]
        if undeclared:
            fail(
                "the image ran pass(es) the declaration does not know: "
                + ", ".join(undeclared)
                + f" — a digest bump moved the pass list; re-read and extend"
                  f" {DECLARATION} in the bump's own PR"
            )
        print(f"drift check ok: image output names {len(names)} passes, all declared")


# A guard that has never been seen to fail is not evidence (the
# deliberate-break precedent, run 38088014628 for web-test). The self-test
# re-proves every red direction on every ci-local run, against synthetic
# trees, through the same CLI the justfile and the workflow invoke —
# subprocess with cwd set, exactly how production calls it.


def _self_test_case(
    script: Path, tree: Path, name: str, args: list[str], *, expect_red: bool
) -> None:
    """Run the CLI in a synthetic tree and require the expected verdict."""
    proc = subprocess.run(  # nosemgrep -- audited: dangerous-subprocess-use-audit
        # Fixed argv (this script + its own flags), no shell, cwd is a
        # private tempdir the test just built. Nothing externally controllable.
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
    """Re-prove all three red directions plus fail-closed on synthetic trees."""
    script = Path(__file__).resolve()
    with tempfile.TemporaryDirectory(prefix="ci-drift-selftest-") as tmp:
        root = Path(tmp)
        case_counter = [0]

        def make_tree(declaration: str, image_log: str | None) -> Path:
            case_counter[0] += 1
            tree = root / f"tree-{case_counter[0]}"
            tree.mkdir()
            (tree / "justfile").write_text(SELF_TEST_TREE_JUSTFILE, encoding="utf-8")
            (tree / ".github" / "workflows").mkdir(parents=True)
            wf = tree / ".github" / "workflows" / "gate.yml"
            wf.write_text(SELF_TEST_TREE_WORKFLOW, encoding="utf-8")
            (tree / "scripts").mkdir()
            (tree / "scripts" / "gate_passes.tsv").write_text(declaration, encoding="utf-8")
            if image_log is not None:
                (tree / "image.log").write_text(image_log, encoding="utf-8")
            return tree

        # Green: alpha mirrored, alpha guarded by the workflow step, both
        # declared checks covered, image output all declared.
        green = make_tree(
            "one:alpha\talpha\tmirrors alpha\ntwo:beta\t-\texempt: provisioning only\n",
            "[PASS] one:alpha\n[PASS] two:beta\n",
        )
        _self_test_case(script, green, "green", [], expect_red=False)
        _self_test_case(
            script, green, "green-image", ["--image-log", "image.log"], expect_red=False
        )

        # A-red: a ci-local leg with no CI home.
        tree = make_tree("one:alpha\talpha\tmirrors alpha\n", None)
        (tree / "justfile").write_text(
            "ci-local: alpha charlie\n\nalpha:\n\ttrue\ncharlie:\n\ttrue\n", encoding="utf-8"
        )
        _self_test_case(script, tree, "a-ungated-leg", [], expect_red=True)

        # B-red: a mirror naming a leg ci-local does not run.
        tree = make_tree("one:alpha\tnonexistent\tmirrors nothing real\n", None)
        _self_test_case(script, tree, "b-phantom-mirror", [], expect_red=True)

        # B-red: a declared pass with no mirror and no written reason.
        tree = make_tree("two:beta\t-\t\n", None)
        _self_test_case(script, tree, "b-empty-reason", [], expect_red=True)

        # C-red: the image's output names a check the declaration does not know.
        tree = make_tree(
            "one:alpha\talpha\tmirrors alpha\n", "[PASS] one:alpha\n[PASS] node:zork\n"
        )
        _self_test_case(
            script, tree, "c-undeclared-pass", ["--image-log", "image.log"], expect_red=True
        )

        # C fail-closed: an empty log is a failure, not a pass.
        tree = make_tree("one:alpha\talpha\tmirrors alpha\n", "")
        _self_test_case(script, tree, "c-empty-log", ["--image-log", "image.log"], expect_red=True)

    print("ci-drift self-test: 8 cases, all pass")


if __name__ == "__main__":
    main()
