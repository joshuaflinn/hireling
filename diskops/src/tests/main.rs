//! Tests for [`super`] — the `reap` shell.
//!
//! Loaded by `src/main.rs` via `#[path = "tests/main.rs"] mod tests;`, which
//! makes this a *child* of that module, so `use super::*` reaches the private
//! `reap_into` and the rest of the shell exactly as an inline `mod tests`
//! would. No visibility inflation.
//!
//! These are integration tests in substance: they build a throwaway repo with
//! a real worktree, run the real `reap` path over it, and assert the plan text
//! and the filesystem state. Nothing here reads or sweeps the bench's own
//! sibling set — every call passes `--parent` into a temp dir.
//!
//! No `#[expect(clippy::unwrap_used, ...)]` header: the root `clippy.toml`
//! already exempts unwrap/expect/panic/dbg in tests.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use super::reap_into;

/// A throwaway parent dir holding a bare `origin`, a main worktree, and one
/// hand-cut `hireling-*` worktree whose branch was squash-merged onto `main`
/// — so the branch tip is *not* an ancestor of `origin/main`, which is the
/// exact shape `main`'s squash-merge policy puts every merged sibling in
/// (gh#372's evidence: 8 CLOSED/MERGED siblings, every one `is_ancestor`
/// -failing). The root lives under `$PAPERCLIP_RUN_SCRATCH_DIR`, which the
/// harness deletes when the run ends. There is deliberately no `Drop` here:
/// an `rm -rf` racing a `git` process still writing inside the same tree is a
/// flake, not a cleanup, and the two tests in this module run in parallel.
struct SquashMergedSibling {
    root: PathBuf,
}

/// Run `git` for the fixture, in `cwd`, with the ambient git environment
/// stripped.
///
/// The strip is not hygiene, it is correctness. The fixture's directories are
/// not repositories yet, and `git` locates the repo for `init`/`clone` by
/// walking up from the cwd. Under the pre-commit hook the test process
/// inherits the invoking `git`'s environment (`GIT_PREFIX`, and `GIT_DIR`
/// whenever the commit ran from a subdirectory), which gives that walk a
/// result: `rev-parse --show-toplevel` answers the *harness* worktree instead
/// of failing, the `--show-cdup` walk runs from inside it, and it does not stop
/// at `--show-ceiling` because `/home/flinn/.tmp` is not inside
/// `.../aa215649-.../hireling`. `git init --bare` then targets the shared
/// house repo and fails on its locked `config`:
///
/// > could not lock config file .../hireling/.git/config: File exists
///
/// Measured: 11 consecutive `cargo test` runs pass, while the commit hook fails
/// all three every time — so this is environment-dependent, not a flake. The
/// suite is not allowed to inherit that state: a test that can write to the
/// house repo is wrong whatever its exit code. `env_clear` keeps only what a
/// `git` subprocess needs to run.
///
/// Identity is set with `git config --file` on the fixture's own config files,
/// never through the environment. The bench's `git` is a wrapper
/// (`paperclip-github-runtime/<run>/git`) that deletes every inherited
/// `GIT_AUTHOR_*`/`GIT_COMMITTER_*`/`GIT_CONFIG_*` and then sets the identity
/// vars to the empty string and `GIT_CONFIG_GLOBAL`/`_SYSTEM` to `/dev/null`
/// — after `Command::env` has run, so no `.env()` call survives it. Measured:
/// `.env("GIT_AUTHOR_NAME", "Reap Test")` through the wrapper still dies with
/// `fatal: empty ident name (for <>) not allowed`.
fn git(args: &[&str], cwd: &Path) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap_or_else(|error| panic!("cannot run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| "<non-utf8>".to_owned())
        .trim()
        .to_owned()
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
}

impl SquashMergedSibling {
    /// Build the fixture for test `id`, whose sibling worktree is either
    /// clean or holds an uncommitted change.
    fn new(id: &str, sibling_clean: bool) -> Self {
        // Under the run scratch dir when the harness names one, so the tree is
        // outside every worktree and is cleaned with the run. The name is
        // pid-and-kind unique: these tests run in parallel and each needs its
        // own repo.
        //
        // `unwrap_or_else` is not a fallback-only branch: the harness deletes
        // the run scratch dir when the run ends, so any later `cargo test` in
        // this worktree names a directory that no longer exists. Create it if
        // missing rather than failing the suite on a stale variable.
        let scratch = std::env::var_os("PAPERCLIP_RUN_SCRATCH_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let root = scratch.join(format!("diskops-reap-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("cannot create {}: {error}", root.display()));

        // origin: an empty repo standing in for the remote `main`.
        let origin = root.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&["init", "--bare"], &origin);
        git(&["symbolic-ref", "HEAD", "refs/heads/main"], &origin);
        // Identity lives in the repo, not the environment — see `git` above.
        // `clone` propagates these two keys to `work`, so one write here
        // covers both repos.
        git(&["config", "--file", "config", "user.email", "reap@example.invalid"], &origin);
        git(&["config", "--file", "config", "user.name", "Reap Test"], &origin);

        // work: the checkout the worktrees are cut from.
        let work = root.join("work");
        git(&["clone", origin.to_str().unwrap(), "work"], &root);
        git(&["config", "user.email", "reap@example.invalid"], &work);
        git(&["config", "user.name", "Reap Test"], &work);
        write_file(&work.join("tracked.txt"), "one\n");
        git(&["add", "tracked.txt"], &work);
        git(&["commit", "-m", "base"], &work);
        git(&["push", "origin", "main"], &work);

        // The sibling branch: one commit of its own, squash-merged onto main.
        git(&["checkout", "-b", "sibling"], &work);
        write_file(&work.join("sibling.txt"), "shipped\n");
        git(&["add", "sibling.txt"], &work);
        git(&["commit", "-m", "sibling work"], &work);
        git(&["checkout", "main"], &work);
        // The squash commit: same tree as the branch, different commit object.
        git(&["merge", "--squash", "sibling"], &work);
        git(&["commit", "-m", "sibling work (#1)"], &work);
        git(&["push", "origin", "main"], &work);
        // `work` returns to a clean main; the sibling keeps the branch tip.
        git(
            &["worktree", "add", "../hireling-999-squashed", "sibling"],
            &work,
        );

        let sibling = root.join("hireling-999-squashed");
        assert!(sibling.is_dir(), "fixture: worktree was not created");
        // The fixture must reproduce the failure gh#372 measured: a merged,
        // clean sibling whose HEAD is not an ancestor of origin/main.
        git(&["fetch", "origin", "main"], &sibling);
        let ancestor = Command::new("git")
            .args(["merge-base", "--is-ancestor", "HEAD", "origin/main"])
            .current_dir(&sibling)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("cannot run git merge-base");
        assert!(
            !ancestor.status.success(),
            "fixture is wrong: the squash-merged sibling IS an ancestor of origin/main, so this test would pass against the old ancestry gate too"
        );

        if !sibling_clean {
            write_file(&sibling.join("uncommitted.txt"), "the owner's work\n");
        }
        Self { root }
    }

    fn sibling_path(&self) -> PathBuf {
        // `worktree add ../sibling-wt` is relative to `work`, i.e. `root`.
        self.root.join("hireling-999-squashed")
    }
}

/// Run the real `reap` path over the fixture's parent and return its output.
fn reap_parent_of(root: &Path, prune: bool) -> (String, ExitCode) {
    let parent = std::fs::canonicalize(root).expect("cannot resolve the fixture parent");
    let mut sink: Vec<u8> = Vec::new();
    let args: Vec<String> = if prune {
        vec![
            "--prune".to_owned(),
            "--parent".to_owned(),
            parent.display().to_string(),
        ]
    } else {
        vec!["--parent".to_owned(), parent.display().to_string()]
    };
    let code = reap_into(&args, &mut sink).expect("reap_into returned an error");
    (
        String::from_utf8(sink).expect("reap output is not utf8"),
        code,
    )
}

#[test]
fn reap_removes_a_clean_squash_merged_sibling() {
    let fixture = SquashMergedSibling::new("prune", true);
    let (output, _code) = reap_parent_of(&fixture.root, true);

    assert!(
        output.contains("removed hireling-999-squashed"),
        "the reaper must remove a clean sibling whose branch was squash-merged and so is not an \
         ancestor of origin/main — that is every merged sibling on this bench (gh#372). Output:\n{output}"
    );
    assert!(
        !output.contains("skip hireling-999-squashed"),
        "the retired ancestry criterion must not appear as a skip. Output:\n{output}"
    );
    assert!(
        !fixture.sibling_path().exists(),
        "the worktree directory must actually be gone after --prune"
    );
}

#[test]
fn reap_skips_a_dirty_sibling_whatever_its_merge_state() {
    let fixture = SquashMergedSibling::new("dirty", false);
    let (output, _code) = reap_parent_of(&fixture.root, true);

    assert!(
        output.contains("skip hireling-999-squashed (dirty worktree)"),
        "a dirty sibling is skipped: the clean-tree test is the whole test and it is fail-closed. \
         Output:\n{output}"
    );
    assert!(
        fixture.sibling_path().exists(),
        "a dirty tree belongs to its owner — the directory must still be there"
    );
}

#[test]
fn reap_list_reports_the_clean_sibling_without_removing_it() {
    let fixture = SquashMergedSibling::new("dry-run", true);
    let (output, _code) = reap_parent_of(&fixture.root, false);

    assert!(
        output.contains("would remove hireling-999-squashed"),
        "the dry run must name the clean sibling as reapable. Output:\n{output}"
    );
    assert!(
        fixture.sibling_path().exists(),
        "the default dry run removes nothing"
    );
}
