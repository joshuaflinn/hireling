# Plan: vendor the image's semgrep rule directory, scan repo root (gh#320)

**Task**: MOR-679 (re-scoped from MOR-678 by Korrin's MOR-692 decision, Route 1)
· **Issue**: gh#320 (plus the gh#332 `errors[]` fail-closed assertion it forces)
· **Branch**: `ci/320-vendor-semgrep-rules`, cut fresh from `origin/main` and
fast-forwarded past PR #319 to `7b1c14f`.
**Spec**: gh#320's body — **this file is the plan the PR opens against.**

Korrin's MOR-692 decision picked **Route 1** over two declined alternatives:
vendor the image's semgrep rule directory into the repo and flip
`gate_passes.tsv`'s `scan:semgrep` row from a false mirror to a **real**
mirror — not a one-line CI-only exemption (Route B), and not a narrower
bench leg (Route C). The two MOR-692 follow-ups (CONTRIBUTOR.md prose and
justfile drift-check prose) are conditional on Route A and **do not apply**:
`scan-semgrep` stays a ci-local leg with a CI home, so both texts remain
accurate.

One scope addition fell out of implementing Route 1: the gh#332 `errors[]`
fail-closed assertion reds the tree as it stands — the image's `curl-eval` /
`gha-curl-pipe-shell` rules hand `gate.yml`'s two `docker run … "${{ env.GATE_IMAGE }}"`
`run:` bodies to a Bash parser that cannot read GitHub expressions, and emit
four `PartialParsing` warns. `nosemgrep` cannot suppress `PartialParsing`, so
the two sites are moved to step-level `env: GATE_IMAGE_REF` mappings (the
expression stays in YAML, the `run:` body reads `"$GATE_IMAGE_REF"`). That fix
is part of this PR, not a separate one.

---

## What was measured first (before any design choice)

All from the pinned digest at `.github/workflows/gate.yml`:
`bearflinn/grizzly-gate@sha256:25c6193d…42bc4e`.

| Measurement | Method | Result |
|---|---|---|
| Files under `etc/grizzly-gate/config/util/semgrep/rules` | registry layer walk, topmost-layer-wins | **4178** regular files |
| Their bytes | same walk | **6 857 653 B** raw; **~21 MiB** on disk (4 KiB block rounding) |
| Languages covered | top-level dirs of that tree | 30+ trees: terraform 730, python 717, generic 510, javascript 358, java 251, ai 227, ruby 189, go 169, yaml 132, php 128, csharp 105, solidity 100, dockerfile 78, typescript 72, … plus `.github`, `Makefile`, `LICENSE` at the directory root |
| Image's own `scan:semgrep` leg | gh#320's cited run `38740809126` | **74.1 s** |
| Vendoring the directory | `scripts/fetch_gate_file.py <rules>/ --output gate-semgrep-rules --manifest scripts/gate_vendored.tsv` | **~35 s**, 4178 files, digest `bbb457ad…` |
| Full vendored set, repo root, image's excludes + `--exclude-rule` | `just scan-semgrep` | **~30 s warm / ~54 s cold, exit 0**; 0 findings, 19 `Missing plugin` info notices tolerated |
| **Fixture A** — `src/main.rs` with ` // nosemgrep` stripped | same command | **exit 1**, `gate-semgrep-rules.rust.lang.security.args` (CWE-807) |
| **Fixture B** — `scripts/fetch_gate_file.py` with `  # nosemgrep` stripped | same command | **exit 1**, `gate-semgrep-rules.python.lang.security.audit.dynamic-urllib-use-detected` |

Both of gh#320's acceptance instances reproduce through the image's rule set
at repo root, and the clean tree goes green. Two fixtures that differ, two
results — the leg the issue asks for exists and is provably load-bearing.

The image's exact scan contract is read off the image's own manifest
(`etc/grizzly-gate/config/util/semgrep/manifest.toml`, fetched for this plan):

```
semgrep scan --json --error --quiet \
  --exclude=dist --exclude=build --exclude=.svelte-kit \
  --exclude-rule=etc.grizzly-gate.config.util.semgrep.rules.python.lang.correctness.common-mistakes.string-concat-in-list \
  --config {config}/rules {source}
```

The bench recipe reproduces it verbatim, with two justified deltas: the
check_id prefixes `gate-semgrep-rules` (the vendored `--config` path) in place
of the image's install path, and one extra `--exclude=gate-semgrep-rules` so
the vendored rules are not scanned by the vendored rules (the image's rule
tree lives outside `{source}`; ours lives inside the repo root). Both are
named in the justfile comment.

---

## Decisions

### D1 — the vendoring unit is the **directory**, not the file

`fetch_gate_file.py` gains a directory mode signalled by a **trailing slash on
`IMAGE_PATH`** (the same signal the manifest row carries): `--output
gate-semgrep-rules` writes the whole tree, and `--manifest` upserts **one**
row for it. `check_gate_vendored.py` gains the directory-row form and
`--image-source` accepts a directory.

*Why.* The unit of the drift question is the unit the image enforces at, and
the image enforces at directory granularity: it scans `--config …/semgrep/rules`,
the whole tree. A manifest with 4178 rows records the same fact 4178 times,
and its drift check can only ever report the trivial "1 file changed of 4178" —
the aggregate digest answers the actual question ("did the image's rule set
move?") in one hash and one comparison.

*Rejected: one row per rule file.* 4178 rows added to a manifest the CI
`check-vendored` step loops over with a **`docker run` per row** — 4178
container starts to re-extract one directory. Same mechanism, 4178× the
ceremony, same answer.

*Rejected: language-scoped subsets.* It reintroduces exactly the drift gh#320
exists to remove, just with a nicer rationale. The whole-directory rule set is
mirrored, or the row is an exemption.

**Aggregate hash contract.** A directory row's third field is `sha256` over a
sorted digest list: for every regular file under the directory, sorted by
relpath, one `relpath<TAB>sha256(content)\n` line each, the joined stream
hashed. A file added, removed, or edited anywhere in the tree moves the
digest. `fetch_gate_file.py` (producer) and `check_gate_vendored.py`
(verifier) compute it byte-identically, and both fail closed on an empty tree,
a symlink, or any non-regular file.

**Directory rows are not self-describing.** The row's image path ends in `/`
(`etc/grizzly-gate/config/util/semgrep/rules/`) and the vendored path too
(`gate-semgrep-rules/`); the trailing slash is what `check_gate_vendored.py`
reads to pick the directory form, and the CI loop reads it to pick `tar -c`
over `cat`. That is the whole signal, and it is parse-guarded: `load_manifest`
reds on a row whose two fields disagree (one trailing slash, the other not),
and a directory row whose materialized dir is missing is tolerated bench-side
(with a notice) while `just scan-semgrep` fails loudly until `just
vendor-semgrep` runs.

### D2 — the materialized tree is git-ignored and vendored on demand

`gate_vendored.tsv`'s second field gains the directory value
`gate-semgrep-rules/` (repo root). The tree is the image's bytes verbatim — no
per-file provenance headers (YAML could carry `#`, JSON and Python cannot, and
stamping 4178 files is noise). Provenance and regeneration live in the
justfile `vendor-semgrep` recipe and the manifest header instead.

Two hygiene exclusions are required by the vendoring itself:

- **`.gitignore`**: `gate-semgrep-rules/` is 4178 files / ~21 MiB of generated
  content, and the tree carries semgrep's own `# ruleid:` test fixtures — 10
  of which are invalid `.json`, which the pre-commit `check_json_dup_keys.py`
  guardrail refuses to commit. So it is git-ignored and vendored on demand,
  the way `web/node_modules` is. `just check-vendored` reds if it is absent
  from a bench tree only via a notice; `just scan-semgrep` fails loudly.
- **scan exclusion**: the vendored tree must not be scanned by the vendored
  rules, so `scan-semgrep` passes `--exclude=gate-semgrep-rules` (see the
  contract above). No `.semgrepignore` is used — the `.gitignore` entry plus
  the explicit `--exclude` is the single mechanism, named in the justfile
  comment.

### D3 — the rule set and the file set are both the image's; that is the mirror

Reproduced exactly from the image's manifest: `--json --error --quiet`, the
three `--exclude` dirs, the one `--exclude-rule`
(`gate-semgrep-rules.python.lang.correctness.common-mistakes.string-concat-in-list`
— the vendored check_id form of the image's path-derived id), and the **repo
root** as target. Nothing dropped, nothing narrowed. Cost ~30–54 s against the
image's 74.1 s, so the bench leg is cheaper than the leg it mirrors and the
issue's cost worry does not fire.

The `--exclude-rule` is empirically verified in both directions: with it, a
scratch file carrying implicit adjacent-literal concatenation reports **0**
results; without it, **1** result under exactly the excluded id. The image's
own manifest comment (ADR-040) documents *why* the rule is excluded — implicit
concatenation is the single form ruff permits and semgrep's alternative
`string-concat-in-list` cannot distinguish from a lost comma — so it is
reproduced, not invented.

### D4 — the gh#332 fail-closed assertion, and the gate.yml fix it forces

`scripts/semgrep_fail_closed.py` (new) reads the same `--json` semgrep emits
and reds on any `errors[]` entry at `level: warn` or carrying `PartialParsing`:
a rule that could not parse its target looks exactly like one that read it and
found nothing, and semgrep exits 0 either way. `level: info` entries (the
image's 19 `Missing plugin` Apex/Elixir notices) are tolerated. Unparseable
JSON, a non-object top level, and a missing `results`/`errors` list all red.

This assertion reds the tree as it stands: the image's `curl-eval` and
`gha-curl-pipe-shell` rules emit four `PartialParsing` warns at the two
`docker run … "${{ env.GATE_IMAGE }}"` lines (gh#332 names both). The fix is
in scope: each `docker run` step gains a step-level `env: GATE_IMAGE_REF: ${{ env.GATE_IMAGE }}`
mapping and the `run:` body references `"$GATE_IMAGE_REF"`. The expression
stays in YAML where the workflow parser evaluates it; the Bash parser only ever
sees `$GATE_IMAGE_REF`. The gate.yml header comment ("named once") is updated
to count the four readers: cache key, first restore-keys line, and the two
`GATE_IMAGE_REF` env lines. After the fix, the full scan reports **0** warn
entries.

### D5 — `just ci-local` must not require a 21 MiB download on every run

- **`just vendor-semgrep`**: runs `scripts/fetch_gate_file.py <rules-dir>/
  --output gate-semgrep-rules --manifest scripts/gate_vendored.tsv`. Explicit,
  never a ci-local dependency — a network pull folded into `ci-local` is a leg
  nobody on a train runs, and the leg that silently skips is the leg that lies.
- **`just check-vendored`** (already a ci-local leg, unchanged name) hashes the
  directory row; absent → notice, present + drifted → red.
- **`scan-semgrep`** keeps its fail-loud contract: missing CLI → red; missing
  vendored rules → red with the remedy named (`just vendor-semgrep`); semgrep
  exit ≥ 2 → red (a run failure, not findings); then the fail-closed assertion
  over the JSON. Findings (exit 1) are semgrep's verdict and are passed through.
- **CI**: the gate workflow's existing image-side loop gains the directory
  branch (`docker run --entrypoint tar … -c -C <dir> . | tar -x`), so a digest
  bump that moves the rules reds CI without any hand-kept list. CI's copy comes
  from `--image-source`; the bench-side tree is git-ignored and CI never needs
  it.

### D6 — the `nosemgrep` suppressions stay; the fixtures prove the leg reds

Fixture A and B are about **proving the leg reds**, not findings to fix. Both
are transient edits, reverted, recorded. Neither suppression is removable:
`src/main.rs`'s argv read selects a subcommand and makes no security decision
(the security-relevant inputs are each validated downstream), and
`fetch_gate_file.py`'s `urllib.request.urlopen` hits fixed `https` endpoints
with the digest read from the repo's own workflow, never user input.

---

## Files this PR touches

| Path | Change |
|---|---|
| `scripts/fetch_gate_file.py` | directory mode (trailing-slash `IMAGE_PATH`, `--output`), `directory_digest` / `search_directory` / `extract_directory` / `write_tree`, `--self-test` cases (22 checks) |
| `scripts/check_gate_vendored.py` | directory-row form (trailing-slash signal on both fields), `directory_digest`, `check_offline` absent→notice / present→compare, `check_image_sources` dir branch, `--self-test` cases (28) |
| `scripts/gate_vendored.tsv` | +1 directory row: `etc/grizzly-gate/config/util/semgrep/rules/`, digest `bbb457ad…`; header documents both row forms |
| `scripts/semgrep_fail_closed.py` | new: the gh#332 `errors[]` fail-closed assertion, findings/summary, `--self-test` (9 cases) |
| `.gitignore` | ignore `gate-semgrep-rules/` |
| `justfile` | new `vendor-semgrep` recipe; `scan-semgrep` rewritten to the image's contract (repo root, vendored rules, its excludes, its `--exclude-rule`, fail-closed JSON assertion); comments naming every reproduced knob |
| `scripts/gate_passes.tsv` | `scan:semgrep` row: real mirror claim linking gh#320 + gh#332 + `check-vendored` |
| `.github/workflows/gate.yml` | image-side loop handles the directory row (`tar` branch); both `docker run` sites move `${{ env.GATE_IMAGE }}` to step-level `env: GATE_IMAGE_REF` (gh#332); header comment re-counts the readers |

**Test-first**: the `check_gate_vendored.py --self-test` and
`fetch_gate_file.py --self-test` cases for the directory form — missing
directory, drifted file inside it, a symlink, a mixed trailing-slash row, an
empty tree — are written before the directory mode runs and each confirmed red
before it is made green. The two gh#320 fixtures are driven as transient tree
edits with the red rule id captured, and reverted.

## Open questions handed to the review round

1. Is the aggregate-digest-per-directory contract in D1 the right drift unit,
   or should the row carry per-language sub-digests so a drift diagnostic names
   the subtree that moved?
2. D5 keeps the ~21 MiB tree out of git and out of ci-local's dependency
   graph, gating it on `vendor-semgrep` + `check-vendored`. Is a bench leg that
   reds on an un-vendored tree acceptable on a fresh clone, or does the repo's
   convention demand the leg provision itself?
3. The bench CLI is semgrep 1.180.0; the image's semgrep version is not in the
   manifest. Is a version-parity knob in scope for this PR or a gh#322-style
   follow-up?
4. The `tar` branch in gate.yml's image-side loop (`docker run --entrypoint tar`)
   assumes the gate image ships a `tar` binary — the same image already serves
   `--entrypoint cat` for the file row. This is CI-verified (no docker on the
   bench); flagging it as the one assumption the PR gate, not the bench, closes.

## Not in scope

No rule authoring, no rule subset, no new bench leg for any other scan
(gitleaks, osv-scanner, trivy keep their rows), no change to how the image
runs beyond the two `GATE_IMAGE_REF` moves, and no `#[allow]`/`nosemgrep` added
to this tree's own source.
