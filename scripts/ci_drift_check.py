#!/usr/bin/env python3
"""ci-drift: the guardrail guarding the guardrails (gh#151).

Four directions, all failing closed:

A. every ci-local leg has a CI home — an image mirror declared in
   scripts/gate_passes.tsv, or a guard step in the gate workflow
   (repo_side, derived from the workflow's own text; MOR-142/MOR-130:
   never a hand-kept literal);
B. every declared image pass is mirrored by a leg ci-local actually
   runs, or carries its written exemption (an empty reason does not
   even parse);
C. (--image-log, CI-side) the image's own stdout and the declaration
   agree in both directions (gh#163 F1): the image names nothing the
   declaration does not know, and it names every mirrored pass the
   declaration does know — a digest bump cannot silently drop a pass
   any more than silently add one. Exempt rows are not required in the
   image's output (a skipped provisioning or scan pass is not drift).
   A log with no pass tokens is a failure, not a pass.
D. the gate workflow's own text still invokes this script with
   --image-log (gh#163 F3) — the image-side step invokes the script
   directly, so repo_side cannot see its deletion; this closes it the
   same text-derived way.

A verdict line whose pass name is off-shape is a failure naming the
line (gh#163 F2): a filter that silently drops what it cannot parse
is not fail-closed.

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
from collections.abc import Callable
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
# A guard step body that runs exactly one leg on one line: `just <leg>`,
# optionally followed by flag-shaped arguments only (gh#320 runs `just
# check-vendored --allow-unvendored`). Anything beyond a bare flag tail — a
# shell chain `just a && just b`, a command separator `just a; true`, a
# redirect `just a > /dev/null`, a pipe — reads as ungated and fails closed:
# the CI home is the step, and a step that can never red (or whose diagnostic
# is suppressed) is not a home. `[\w./-]+` keeps flag values to path/word
# shapes; `--flag value` (two words) is deliberately not matched, so a
# multi-word tail stays ungated rather than being guessed.
GUARD_STEP_RE = r"^ *run: just ([\w-]+)(?: +--?[\w-]+(?:=[\w./-]+)?)* *$"
# A declaration row is pass, mirror-or-'-', reason — no more, no less.
DECLARATION_FIELDS = 3
# The image prints one verdict line per check: "[PASS] node:tsc".
# Match the verdict BROADLY (gh#163 F2): CHECK_NAME_RE stays out of the
# match and is enforced with re.fullmatch on each captured name, so an
# off-shape name reds with a diagnostic instead of being dropped from
# the found set and invisible to direction C.
IMAGE_LINE_RE = re.compile(r"\[(PASS|FAIL|SKIP)\] +(\S+)")
# The gate workflow's image-side drift step (gh#163 F3): this script
# invoked with --image-log. Text-derived, same discipline as repo_side.
IMAGE_STEP_RE = re.compile(r"^ *run: .*\bci_drift_check\.py +--image-log\b", re.MULTILINE)

SELF_TEST_TREE_JUSTFILE = "ci-local: alpha\n\nalpha:\n\ttrue\n"
SELF_TEST_TREE_WORKFLOW = (
    "jobs:\n  gate:\n    steps:\n      - name: guard\n        run: just alpha\n"
    "      - name: drift image side\n"
    "        run: python3 scripts/ci_drift_check.py --image-log image.log\n"
)
# The same guard step, with an argument appended. The CI home of a leg is the
# step; what the step passes to it is not a new leg and must not read as one.
SELF_TEST_TREE_WORKFLOW_ARGED = (
    "jobs:\n  gate:\n    steps:"
    "\n      - name: guard\n        run: just alpha --allow-unvendored\n"
    "      - name: drift image side\n"
    "        run: python3 scripts/ci_drift_check.py --image-log image.log\n"
)
# A multi-line `run: |` block whose first line invokes the leg. A YAML literal
# block is a different shape of `run:` value and this check does not read it,
# so the leg must still read as ungated (fail closed) rather than be guessed.
SELF_TEST_TREE_WORKFLOW_MULTILINE = (
    "jobs:\n  gate:\n    steps:"
    "\n      - name: guard\n        run: |\n          just alpha\n"
    "      - name: drift image side\n"
    "        run: python3 scripts/ci_drift_check.py --image-log image.log\n"
)
SELF_TEST_TREE_WORKFLOW_NO_IMAGE_STEP = (
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
    coverage over a hole. Matches only a single-line `run: just <leg>`
    step, optionally followed by flag-shaped arguments only (`just
    check-vendored --allow-unvendored`, gh#320): the CI home is the step,
    and what it passes to the leg is not a second leg. Everything else
    reads as ungated and fails closed — a multi-line `run: |` block (its
    body lines lack the `run: ` prefix), a chained `just a && just b`, a
    command separator `just a; true`, a redirect `just a > /dev/null`, and
    a piped `just a | tee`.
    This is a stdlib-only text match: `scripts/pyproject.toml` declares the
    scripts zero-dependency, so no YAML parser is pulled in here.

    What the derivation still cannot see: textual presence is not execution
    (Thrane, MOR-130) — a guard step with `if: false` stays in this set
    while no longer running on PRs. No parse closes that hole; the
    deliberate-break run is the cover.
    """
    if not WORKFLOW.is_file():
        fail(f"{WORKFLOW} not found — run from the repo root")
    text = WORKFLOW.read_text(encoding="utf-8")
    return set(re.findall(GUARD_STEP_RE, text, re.MULTILINE))


def require_image_side_step() -> None:
    """Direction C must be wired: the workflow's text invokes --image-log.

    gh#163 F3. The bench-side step protects itself — repo_side matches
    `run: just ci-drift` from this same text, so neutering that step
    reds direction A. The image-side step invokes this script directly,
    so no leg list notices its deletion: remove it and bench and CI both
    stay green with direction C gone. Close it the same text-derived way
    repo_side closes the bench side — the invocation must appear in the
    workflow's own text. Like repo_side, matches only a single-line
    `run: … ci_drift_check.py --image-log …` step, so any other spelling
    reads as unwired and fails closed. Same caveat as repo_side: textual
    presence is not execution; the deliberate-break run is the cover.
    """
    if not WORKFLOW.is_file():
        fail(f"{WORKFLOW} not found — run from the repo root")
    if not IMAGE_STEP_RE.search(WORKFLOW.read_text(encoding="utf-8")):
        fail(
            f"no ci_drift_check.py --image-log invocation in {WORKFLOW} —"
            " direction C (the image-side drift check) is not wired;"
            " restore the step in the gate workflow"
        )


def check_image_log(
    image_log: Path, passes: dict[str, tuple[str, str]], mirrors: set[str]
) -> None:
    """Direction C: the image's stdout and the declaration agree, two-way.

    Forward (gh#151): a pass the image runs must be declared. Reverse
    (gh#163 F1): a MIRRORED pass the image's output stops naming reds —
    direction A would otherwise keep certifying its bench leg through a
    mirror row nothing backs. Exempt rows are excluded from the reverse
    comparison: they describe provisioning/scan passes the image may
    legitimately skip, and requiring them would false-red. Fail-closed
    on shape (gh#163 F2): a verdict line whose name does not fit
    CHECK_NAME_RE reds naming the line — a filter that silently drops
    what it cannot parse is not fail-closed — and a log with no pass
    tokens is a failure, not a pass.
    """
    if not image_log.is_file():
        fail(
            f"image log {image_log} not found — cannot certify the"
            " declaration against the image"
        )
    names: list[str] = []
    for lineno, line in enumerate(
        image_log.read_text(encoding="utf-8", errors="replace").splitlines(), 1
    ):
        for _verdict, name in IMAGE_LINE_RE.findall(line):
            # gh#163 F2: a name that does not fit the check-name shape
            # is a pass the declaration cannot be compared against —
            # red naming the line, never silently dropped.
            if not re.fullmatch(CHECK_NAME_RE, name):
                fail(
                    f"{image_log}:{lineno}: verdict names '{name}',"
                    f" which is not an image check name ({CHECK_NAME_RE})"
                    f" — an unparsable pass is an undeclared pass, not a"
                    f" skippable line: {line.strip()!r}"
                )
            names.append(name)
    if not names:
        fail(
            f"no [PASS]/[FAIL] pass tokens found in {image_log} — the"
            " image's output is empty or unreadable; failing closed rather"
            " than certifying an unseen run"
        )
    names_set = set(names)
    undeclared = [name for name in sorted(names_set) if name not in passes]
    if undeclared:
        fail(
            "the image ran pass(es) the declaration does not know: "
            + ", ".join(undeclared)
            + f" — a digest bump moved the pass list; re-read and extend"
              f" {DECLARATION} in the bump's own PR"
        )
    # gh#163 F1: the reverse direction. A mirrored pass the image's
    # output stops naming means direction A has been certifying its
    # bench leg through a mirror row nothing backs. Mirrored rows
    # only — the exempt rows describe provisioning/scan passes the
    # image may legitimately skip, and requiring them would false-red.
    dropped = sorted(
        name
        for name, (mirror, _) in passes.items()
        if mirror != "-" and name not in names_set
    )
    if dropped:
        fail(
            "the image's output omits mirrored pass(es) the declaration"
            " expects: "
            + ", ".join(dropped)
            + " — a digest bump dropped or renamed them; the mirror rows"
              " no longer back their bench legs. Re-read and fix"
            + f" {DECLARATION} in the bump's own PR"
        )
    print(
        f"drift check ok: image output names {len(names_set)} passes,"
        f" all declared; all {len(mirrors)} mirrored passes present"
    )


def main() -> None:
    """Check A, B and D against the repo; C additionally with --image-log."""
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
    require_image_side_step()

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

    # C. The image's own stdout and the declaration agree in both directions.
    if args.image_log is not None:
        check_image_log(args.image_log, passes, mirrors)


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
    """Re-prove all four red directions plus fail-closed on synthetic trees."""
    script = Path(__file__).resolve()
    ran: list[str] = []

    def run_case(tree: Path, name: str, args: list[str], *, expect_red: bool) -> None:
        ran.append(name)
        _self_test_case(script, tree, name, args, expect_red=expect_red)

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

        _run_self_test_cases(run_case, make_tree)

    print(f"ci-drift self-test: {len(ran)} cases, all pass")


def _run_self_test_cases(
    run_case: Callable[..., None],
    make_tree: Callable[..., Path],
) -> None:
    """Build and run every synthetic-tree case (kept out of self_test for PLR0915)."""
    # Green: alpha mirrored, alpha guarded by the workflow step, both
    # declared checks covered, image output all declared.
    green = make_tree(
        "one:alpha\talpha\tmirrors alpha\ntwo:beta\t-\texempt: provisioning only\n",
        "[PASS] one:alpha\n[PASS] two:beta\n",
    )
    run_case(green, "green", [], expect_red=False)
    run_case(green, "green-image", ["--image-log", "image.log"], expect_red=False)

    # A-red: a ci-local leg with no CI home.
    tree = make_tree("one:alpha\talpha\tmirrors alpha\n", None)
    (tree / "justfile").write_text(
        "ci-local: alpha charlie\n\nalpha:\n\ttrue\ncharlie:\n\ttrue\n", encoding="utf-8"
    )
    run_case(tree, "a-ungated-leg", [], expect_red=True)

    # B-red: a mirror naming a leg ci-local does not run.
    tree = make_tree("one:alpha\tnonexistent\tmirrors nothing real\n", None)
    run_case(tree, "b-phantom-mirror", [], expect_red=True)

    # B-red: a declared pass with no mirror and no written reason.
    tree = make_tree("two:beta\t-\t\n", None)
    run_case(tree, "b-empty-reason", [], expect_red=True)

    # C-red: the image's output names a check the declaration does not know.
    tree = make_tree(
        "one:alpha\talpha\tmirrors alpha\n", "[PASS] one:alpha\n[PASS] node:zork\n"
    )
    run_case(tree, "c-undeclared-pass", ["--image-log", "image.log"], expect_red=True)

    # C fail-closed: an empty log is a failure, not a pass.
    tree = make_tree("one:alpha\talpha\tmirrors alpha\n", "")
    run_case(tree, "c-empty-log", ["--image-log", "image.log"], expect_red=True)

    # C-red (gh#163 F1): the image's output omits a MIRRORED declared
    # pass — the reverse of c-undeclared-pass. one:alpha is mirrored;
    # its absence must red even though two:beta (exempt) is named.
    tree = make_tree(
        "one:alpha\talpha\tmirrors alpha\ntwo:beta\t-\texempt: provisioning only\n",
        "[PASS] two:beta\n",
    )
    run_case(
        tree, "c-mirrored-pass-dropped", ["--image-log", "image.log"], expect_red=True
    )

    # C-green (gh#163 F1, the false-red guard): an EXEMPT pass absent
    # from the image's output is not drift — only mirrored rows are
    # required to appear.
    tree = make_tree(
        "one:alpha\talpha\tmirrors alpha\ntwo:beta\t-\texempt: provisioning only\n",
        "[PASS] one:alpha\n",
    )
    run_case(
        tree, "c-exempt-pass-skipped", ["--image-log", "image.log"], expect_red=False
    )

    # C-red (gh#163 F2): a verdict line whose name is off-shape reds
    # naming the line, instead of being dropped from the found set.
    tree = make_tree(
        "one:alpha\talpha\tmirrors alpha\n",
        "[PASS] one:alpha\n[PASS] node:ts_check\n",
    )
    run_case(
        tree, "c-offshape-pass-name", ["--image-log", "image.log"], expect_red=True
    )

    # D-red (gh#163 F3): the workflow no longer invokes this script
    # with --image-log. Direction A and B still hold; the missing
    # step alone must red, bench-side, without --image-log.
    tree = make_tree("one:alpha\talpha\tmirrors alpha\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        SELF_TEST_TREE_WORKFLOW_NO_IMAGE_STEP, encoding="utf-8"
    )
    run_case(tree, "d-image-step-deleted", [], expect_red=True)

    # A-green: the guard step passes an argument to the leg. The leg is
    # still gated; the argument is not a new leg (gh#320: CI runs
    # `just check-vendored --allow-unvendored`). Alpha is unmirrored so
    # the derived repo_side step is its only CI home — the arged form
    # must still derive it, or direction A reds.
    tree = make_tree("one:alpha\t-\texempt: gated by the guard step\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        SELF_TEST_TREE_WORKFLOW_ARGED, encoding="utf-8"
    )
    run_case(tree, "a-guard-step-arged", [], expect_red=False)

    # A-red: the same leg invoked from a multi-line `run: |` block, a
    # shape this derivation does not read. Fails closed: textual presence
    # in an unread shape is not a derived CI home, so the unmirrored leg
    # stays ungated and direction A reds.
    tree = make_tree("one:alpha\t-\texempt: gated by the guard step\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        SELF_TEST_TREE_WORKFLOW_MULTILINE, encoding="utf-8"
    )
    run_case(tree, "a-guard-step-multiline", [], expect_red=True)

    # A-red (gh#320 finding 5): a chained `just alpha && just beta` must
    # read as ungated — the leg name is not the whole line, and deriving
    # alpha would certify a step whose tail is a second leg.
    tree = make_tree("one:alpha\t-\texempt: gated by the guard step\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        "jobs:\n  gate:\n    steps:"
        "\n      - name: guard\n        run: just alpha && just beta\n"
        "      - name: drift image side\n"
        "        run: python3 scripts/ci_drift_check.py --image-log image.log\n",
        encoding="utf-8",
    )
    run_case(tree, "a-guard-step-chained", [], expect_red=True)

    # A-red (gh#320 finding 5): `just alpha; true` derives alpha from a step
    # that can never red — a fail-open hole in the checker whose whole job is
    # proving every leg has a CI home that can fail.
    tree = make_tree("one:alpha\t-\texempt: gated by the guard step\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        "jobs:\n  gate:\n    steps:"
        "\n      - name: guard\n        run: just alpha; true\n"
        "      - name: drift image side\n"
        "        run: python3 scripts/ci_drift_check.py --image-log image.log\n",
        encoding="utf-8",
    )
    run_case(tree, "a-guard-step-separator", [], expect_red=True)

    # A-red (gh#320 finding 5): `just alpha > /dev/null` suppresses the
    # diagnostic — a step that cannot be seen to red is not a CI home.
    tree = make_tree("one:alpha\t-\texempt: gated by the guard step\n", None)
    (tree / ".github" / "workflows" / "gate.yml").write_text(
        "jobs:\n  gate:\n    steps:"
        "\n      - name: guard\n        run: just alpha > /dev/null\n"
        "      - name: drift image side\n"
        "        run: python3 scripts/ci_drift_check.py --image-log image.log\n",
        encoding="utf-8",
    )
    run_case(tree, "a-guard-step-redirect", [], expect_red=True)


if __name__ == "__main__":
    main()
