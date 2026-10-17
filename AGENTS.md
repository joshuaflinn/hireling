# AGENTS.md — Hireling

Instructions for any AI coding agent working in this repo. Read `CONSTITUTION.md`
first — it is law. Then `docs/PRD.md` — it is the spec. Rust engineering
conventions (lint policy, test layout, error handling, git workflow) live in
`docs/toolkit-conventions.md` — vendored from Bear's rust-toolkit, enforced by
the pre-commit hooks and CI. They are not advisory. Where this file and
`docs/toolkit-conventions.md` disagree, this file wins: the vendored conventions
govern Rust engineering practice — lint policy, test layout, error handling —
not this repo's branch, review, and merge workflow, which Hard rule 3 and
*Workflow* below own outright.

## What this is

Hireling: a party-linked Pathfinder 2e character tracker. PWA. Pathbuilder JSON
imports, real-time party sync, a modifier engine that does PF2e buff/condition math
with per-number provenance, shared party inventory (P1). Friends-and-family POC for
six users. Full spec: `docs/PRD.md`. UX baseline: `docs/reference/lorum_ipsum_dashboard.html`.

## Stack (settled — do not re-litigate)

- **Backend:** Rust, axum, single binary. WebSocket for party sync; REST for auth/import/bootstrap.
- **Database:** Postgres — a `hireling` hall on the house shared instance (Asgard).
  Migrations via `sqlx migrate`, checked in. Local dev: throwaway postgres in compose.
- **Frontend:** Svelte (Vite), hand-rolled CSS, PWA (service worker + manifest).
  No component library. No SvelteKit.
- **Auth:** Authentik OIDC. No password code in this repo, ever.

## Hard rules

1. **Scope discipline.** The PRD's non-goals are binding: no character builder, no
   combat/round/initiative tracking, no GM tooling beyond read-only, no dice roller,
   no auto-expiry or aura/positioning automation. If a task smells like these, stop
   and flag it — do not build it.
2. **The modifier engine** is pure, isolated, and test-first. Base stats + active
   effects in; derived stats + provenance out. No I/O, no framework imports.
3. **Branches and PRs.** Inside a harness-provisioned workspace, the
   provisioned branch (`MOR-<n>`) is the PR branch — it is tied to its task
   1:1, so do not cut a prefixed branch over it. One exception: the branch
   is not yours — it is another open PR's head, or it carries another
   task's commits — see the collision rule in *Workflow*. The
   conventional prefixes
   (`feat/`, `fix/`, `refactor/`, `docs/`, `ci/`, `chore/`), referencing the
   issue where one exists (`feat/12-buff-engine`), govern branches cut by
   hand outside a provisioned workspace — a human in a clone, a hotfix off
   the shared checkout. Never commit to `main`.
   Never force-push shared branches — a branch is shared from the moment
   anything but you can read a head off it: in practice from its first push
   to `origin`, and beyond argument once a PR is open against it or a review
   round is announced against it. Who cut it is irrelevant, because a
   reviewer and a press seat read heads off that ref. PR #277 is the reason:
   its author hand-cut `docs/275-head-pinned`, force-pushed it at 11:51:01Z
   (`head_ref_force_pushed`, erasing `279e9600` back to `5c73ddc`), and
   defended it as "my own hand-cut branch, not a shared branch" — a reading
   the text permitted. Open a PR; the author never merges their own work.
   Authorship is read off the board, not off GitHub: every Moradin agent
   pushes as `joshuaflinn`, so between agents a PR's `author.login` is the same
   string and cannot distinguish author from presser. For a PR this company
   authors, the authoritative record of authorship is the tracking task's
   assignee, and the press seat reads it there. An outside author pushes under
   their own identity, and GitHub reads it as-is. Thrane holds the merge
   button — merge asks go to Thrane, not to Josh.
   A PR authored by Thrane routes to Korrin — a third seat who authored
   neither the change nor its review. Either owner may reserve any PR for
   their own hand by saying so on it, and either owner may revoke the routing.
4. **Verify before claiming done.** Run the build and the tests. Report actual
   output. If you couldn't verify something, say exactly that.
5. **Write it down.** Decisions with rejected alternatives go in the PR description.
   If you discover the PRD is wrong, say so — don't code around it silently.

## Workflow

- Work comes from **GitHub Issues** on this repo. One issue, one branch, one PR.
  Small PRs win.
- **Board of record.** GitHub is the record of the work; your task tracker
  (Paperclip, whatever harness you run in) is the record of who is doing it.
  Each issue stays canonical — no mirroring, and no second verdict on done
  anywhere else.
- **Review of record.** The verdict lands on GitHub too, not only on the
  board. Before the merge ask, the reviewer posts the verdict on the PR as a
  review comment (`gh pr review <n> --comment -F -`, verdict on stdin):
  accepted or rejected, a link to the review task, and the head SHA the
  verdict is bound to. GitHub bars approving your own PR and every agent
  pushes as one shared identity — so the record is a review comment, and
  required approvals on `main` stay at 0 for the same reason. A merged PR
  with an empty review timeline is a missing record: restore it before the
  next merge ask.

  **Scope is part of the verdict.** An accept is an accept of the head
  only when its scope is the head itself, not one question about the
  head. A reviewer who narrows a round's scope says so in the verdict,
  and the press honors the stated scope instead of reading past it.

  **One review round is one review task.** A round gets a single task —
  the author opens it at seating — and the round's scope is stated in one
  announcement; a round split across tasks produces verdicts that are each
  correct in their own scope and jointly unreadable at the press. While a round is
  open, the press waits, and it reads that state off the PR — the round
  is open when it is announced on the PR and closed when the verdict at
  that head is posted to it, or when the reviewer withdraws the round —
  never off tracker task state, which the
  press seat cannot see at the PR.

  **The author holds the head while a round is open.** Between the
  announcement and the verdict: no push, no merge of `main`. `gh pr review`
  binds a review object's `commit_id` to the PR's head at submit time, not to
  the head the verdict's text names, so a head move mid-round stamps the
  reviewer's verdict with a head nobody read. If the branch goes behind during
  a round, it waits — the verdict lands first, and *Verdict carry-over* then
  handles the head move against a diff already judged.

  **An author who must move the head asks the reviewer to withdraw the
  round.** A plain PR comment saying so; the reviewer closes the round with a
  review comment (`gh pr review <n> --comment -F -`) naming the head and
  stating that no verdict was reached — the round's terminal record, distinct
  from the procedural `gh pr comment` that withdraws a duplicate *announcement*
  under *When a duplicate round is already announced*. A withdrawn round read
  text the author retracted, so it does not count against the budget. Only the
  reviewer closes a round — an author never retires one by pushing over it.
  Once per PR — a second withdrawal ask on the same PR is not granted by the
  reviewer; the PR goes to Thrane, and whether it proceeds is a decision about
  the task.
- **The reviewer announces the round on the PR before reading the diff — and
  only the reviewer.** A review comment (`gh pr review <n> --comment -F -`)
  naming the head SHA under review, the questions the round is asking, and the
  review task. One `gh` call, and the
  open-round signal lives where a presser already reads — no board access
  required, because this file governs any agent in this repo, not one
  harness.

  **The author seats the round; the author does not announce it.** Seating is
  a plain PR comment — one line, `review-seat: PR #<n> round <N> head <sha>`,
  via `gh pr comment` — plus a review task carrying the head, the gate
  check-run id, and the author's questions. That marker is not an
  announcement: an author never calls `gh pr review` to open a round, and a
  seat task never states that the round "is announced". The author's questions
  are the round's proposal, not its scope; scope is set by the seat that read
  the diff, and the reviewer may widen them on an author-text round. PR #247
  is the reason: it carried author announcements at 05:39:40Z (review
  5528645993, three
  questions) and 05:56:27Z (5528801267), so each head held two announcements,
  and the timeline stayed readable only because the reviewer caught both by
  hand — widening round 1 to five questions, one of which was the gate read,
  and adopting round 2 in prose. Where an author announcement exists anyway,
  the reviewer's is the round: it names which author text it adopts or widens,
  and the verdict binds to it.

  **Every handoff step is idempotent — read the record before you write it.**
  Any step whose effect is that a seat now exists for the next stage —
  announcing a round, opening a seat task (an author seating a reviewer, a
  reviewer escalating, anyone opening a round), posting a carry-over re-bind,
  pressing — is performed once. Before performing it, read the record for that
  same step already performed at that PR, round and head, and stop if it is
  there: the PR's reviews and comments for an announcement (a widening
  announcement at a head that already carries one is not a second announcement
  and is still posted), a `review-seat` marker, or a re-bind already posted;
  the board for an open task already naming that PR, round and head. Reuse
  what is already there rather than writing a second one. The press re-reads
  the PR's merge state immediately before the merge call and treats `MERGED`
  as success, not an error, only after confirming the head it gated is the
  head GitHub records as merged (`gh api repos/joshuaflinn/hireling/pulls/<n>
  --jq '.merged, .head.sha'`, with `.head.sha` equal to the gated head), so a
  second presser is a no-op rather than a race. When `.merged` is true but
  `.head.sha` is not the gated head, do not press — escalate to Thrane, the
  same class as a branch collision.

  **A head gets exactly one open round.** A second round against a head that
  already carries an announced round with no verdict is not opened: the
  existing round's scope is widened, and the widening is announced the same
  way. Opening a second is a provisioning defect — note it in the PR body and
  report it on the tracking task, same handling as a branch collision.

  **When a duplicate round is already announced, the earliest reviewer
  announcement at that head is the round.** The widening remedy above assumes
  the second round has not been announced. When it has, the earliest reviewer
  announcement at that head is the round, by `submitted_at`, ties broken by the
  lower review id; an author announcement is never the survivor — it is
  adopted or widened by the reviewer's announcement, never withdrawn. Every
  later **reviewer** announcement is withdrawn by the seat that discovers the
  duplicate, on the PR by name, via `gh pr comment` — a procedural comment
  naming the review id of each announcement it withdraws and carrying no
  verdict. The round gets exactly one verdict, bound on the PR to every
  announcement it collapses and answering each collapsed or adopted
  announcement's questions by announcement and number —
  including any question the surviving announcement did not word that way. A
  verdict per seat is not an alternative: it recreates the PR #158 shape — two
  verdicts at one head with different scopes, each correct in its own terms,
  jointly unreadable at the press, resolved by whichever the press happened to
  read first. At a carry-over head the collapse keeps the narrow scope: a
  collapsed or adopted question outside the combination is declined as out of scope,
  and if real it opens a fresh counted round.

  **Two concurrent runs of one seat.** The collapse orders announcements on
  the PR because ordering on the PR is the one thing both runs read alike: two
  concurrent runs each read a PR that does not yet carry the other's comment,
  and a symmetric rule fails both ways — both runs claiming their own round
  gives one head two verdicts, PR #158's failure mode, and both deferring gives
  it none, which blocks the press outright. A duplicate-seat pair is one round
  against the budget, not two — nobody is charged for a seat they did not ask
  for. If the surviving seat dies before its verdict lands, Thrane
  re-seats the round from the stood-down seat's record, and the re-seated round
  is not charged to the author's budget. PR #250 round 2 is the instance:
  announcements `5529340890`
  and `5529342611` at head `3d653f7`, one verdict `5529401102` bound to both by
  review `5529415179`.

  **Every announced round at the head being merged has a verdict against it
  before the press** — a narrower accept does not close a wider round still
  writing, and where two verdicts at one head disagree, the wider scope
  counts. PR #158 is the reason: it merged at 20:26:54Z on a round-3 accept
  (review 5509509486, 20:24:29Z, one question of the round's three) while a
  wider round-3 review was still running, and the reject (review 5509546840)
  landed 44 seconds after the merge, at 20:27:38Z, with a real defect in it
  (gh#169, ADR 0002's superseded carry-over condition 2). Neither review
  seat was wrong on its own terms — the accept was right inside the one
  question it named, the reject right across the round's three. What failed
  was at the press: an announced three-question round and a verdict covering
  one of them were both on the PR, and the narrow accept was read as an
  acceptance of the head.
- **A review round opens with a check-runs read at the head under review.**
  Before the diff: name the head, then read the `gate` check at it with
  `gh api repos/joshuaflinn/hireling/commits/<sha>/check-runs`. The check-run
  is the certificate. A bench `just ci-local` is corroboration and never a
  substitute, because the two can disagree with the bench wrong: on PR #139
  the gate image carried TypeScript 7.0.2 against `web/`'s 5.9.3 pin, TS2871
  postdates 5.9, and so a green bench run at `f29e85d` *was* the defect. No
  bench run could have caught that; the check-run at the head did.

  **Name the head from the ref, not from the PR object** — `git ls-remote
  origin <branch>` is the primary form; `git fetch origin <branch>` then
  `git rev-parse FETCH_HEAD` is the fallback for when you need the objects
  locally. `FETCH_HEAD` is per-worktree and writable by any process sharing
  the workspace, so a value read from it can be another seat's fetch:
  measured in the `MOR-482` worktree on 2026-10-15, `git rev-parse
  FETCH_HEAD` returned `39ca281` (a different task's branch) and the raw
  `FETCH_HEAD` first line read `dc491fe` (`branch 'main'`), each a valid
  commit with no tell (gh#253). `gh pr view --json headRefOid` can serve a
  head minutes stale:
  on PR #139 it served the superseded head `f29e85d` for about two minutes
  after the push of `9116908` landed. The check-runs read at `f29e85d`
  returns `gate / failure` (workflow run 38417930166) — indistinguishable
  from a current red, while the gate at `9116908` was `success`, 16/16.
  Reading check-runs at the wrong head is the same fault as not
  reading them, and it fails both dangerous ways: a stale red bounces an
  engineer for work already fixed, and a stale green certifies a head nobody
  gated. Every verdict states the SHA it is bound to, so a wrong-head read is
  visible in the record afterwards — that is what makes this rule
  self-checking, and it costs nothing the record does not already carry.

  **A review object's `commit_id` is not evidence of what was reviewed.** The
  head a verdict is bound to is the head its text names. A push can move the
  stamp; it cannot rewrite the prose.

  **No announcement before the gate concludes.** The reviewer's announcement
  names the `gate` check-run's conclusion at the head under review, and the
  conclusion is `success` or `failure`: `queued` and `in_progress` are not
  conclusions, and `cancelled`, `timed_out`, `skipped`, `stale` and `neutral`
  are conclusions that certify nothing. If the gate has not concluded to
  `success` or `failure`, the reviewer does not announce: it waits and
  re-reads, and the idempotence clause above ("read the record before you
  write it") already covers that re-read, so a waiting reviewer does not open
  a duplicate round. Where no usable conclusion arrives, the reviewer does
  not announce, notes it on the PR, and reports it on the review task — the
  same handling as a provisioning defect.
  The author's seat marker is unchanged and does not wait — seating is a
  marker, not a certificate read, and making authors block on a gate they
  cannot speed up adds latency and fixes nothing; the seat task already
  carries the gate check-run id, which is what lets the reviewer read it. An
  `in_progress` gate read as if it were a conclusion is the same fault class
  as reading check-runs at the wrong head, and it fails the same two
  dangerous ways: a round announced before a red concludes can accept a head
  nobody gated, and one announced before a green concludes can bounce an
  engineer on nothing. PR #317 is the instance: the seat marker went up at
  `2026-10-15T20:20:31Z` and round 1 was announced at `20:21:38Z` (review
  `5539383360`); check-run `116649603910` (`gate`) at head
  `6c8f04fb4cd733dcf35fd014ede0510b9d8760de` was still `in_progress` and did
  not conclude until `20:26:05Z` — `failure`, `gate FAILED: 2/16` (review
  `5539498644`) — so for about four and a half minutes an announced round
  faced a head with no readable certificate, and the rule as written did not
  forbid it. No fault landed: the author gated on the bench and disclosed it,
  the reviewer caught both red legs by hand, and the verdict is correct —
  this closes a rule gap on the announcing side, not a fault assignment.

  **The announcement names whether the head is up to date with `main`.**
  Alongside the gate read, at the head already named: `git fetch origin main
  <branch>`, then `git merge-base --is-ancestor origin/main <head>` — exit 0 is
  up to date, exit 1 is behind. The fetch is not optional: `origin/main` is a
  local ref, and a stale one reports a behind head as current — a false pass, the
  same wrong-head fault class as reading check-runs at a superseded SHA. `gh
  pr view <n> --json mergeStateStatus` is corroboration, never a substitute;
  it is computed lazily and can read `UNKNOWN`. A behind head does not hold
  the review — the diff is still the diff — so announce and review as normal,
  and the verdict names the head move the press will need. `main` carries the
  strict up-to-date policy — the `main-gate` ruleset (id `24735400`), which at
  version `v53013064` enforces `required_status_checks` with
  `strict_required_status_checks_policy: true` — so that move is forced; where
  `main` has not edited a reviewed path it is the
  wholly-mechanical carry-over re-bind of *Verdict carry-over* and costs no
  round. This does not relieve *Bring the branch up to date before you ask
  for review* — it is the second seat on it.
- **Review rounds — two author-text rounds, then a verdict.** The budget
  counts rounds that review changed author text. Two of those, then a
  verdict — accept-with-follow-up-issues or reject-with-a-plan. There is no
  third author-text round: if the bar cannot be met in two, that is Thrane's
  decision about the task, not another round. PR #197 is the reason: five
  author-text rounds on a docs-only spec in one morning, rounds 3–5 of which
  should never have opened — a shrinking defect count feels like progress,
  and it is not a licence to keep re-heating the same billet.

  **A carry-over round does not count against the budget.** When condition 2
  of *Verdict carry-over* below (identical contribution) fails only because
  `main` moved an unrelated part of a shared file — the PR's own hunks
  byte-identical at both heads — the round was forced by the strict
  up-to-date policy, not earned by the author. Charging it to the budget
  would force premature verdicts on busy files (`gate.yml`, `AGENTS.md`,
  `justfile`, `Cargo.toml`); PR #158 paid that twice in one afternoon
  (gh#166). The price of not counting is narrowness: a carry-over round's
  only question is the *combination* — read the merged file as one unit and
  look for interaction defects. It may not re-litigate the author's diff;
  that diff already carries a verdict. Announcing a *wide* round at a
  carry-over head is itself a defect. A carry-over round's scope is not
  widened — the one-open-round widening remedy does not apply at a
  carry-over head. If a carry-over round finds a real defect, it rejects,
  and the author's fix commit then opens a counted author-text round — the
  narrow scope is a scope limit, not a lower bar.

  **Which kind a round is, checkable:** compare the PR's own hunks at both
  heads, *and* ask whether `main`'s incoming hunks touch a reviewed one. The
  first round on a PR has no prior head; it is an author-text round. Changed
  author hunks mean an author-text round — wide, counted. Byte-identical
  author hunks fork three ways on `main`'s side of the seam, and only one of
  the three opens a round, so name each outcome by whether a round follows:
  non-interacting `main` hunks are a plain carry-over re-bind — **no round**;
  interacting `main` hunks that need no fix are a carry-over re-bind on the
  reviewer's judgment branch of *Verdict carry-over* — **no round**; and
  interacting `main` hunks whose combination is defective in a way that needs
  added text at the merged head are the reconciliation re-bind round below —
  **a round, uncounted**, and not an author-text round. The first two are
  re-binds, not rounds: each posts a re-bind comment and never an announcement
  carrying a verdict, so a presser must not hold a pressable PR waiting for a
  verdict that will not come, and must not press past an open round. Say
  "re-bind," not "carry-over round," for both — the preceding paragraph's
  carry-over *round* is a third thing with its own trigger. Byte-identical
  hunks alone cannot tell those three apart, which is why the second question
  is part of the test.

  **A verdict is discharged when posted, and is not the author's to
  withdraw.** The head a verdict names carries it permanently. An author may
  disagree with a verdict and may ask for it to be revisited; they may not
  retract it. "Withdrawn — it named a head I replaced before anyone read it"
  is not a state a verdict can be put into, and a presser who believes it
  reads one round where there were two.

  **After an accept, a non-blocking follow-up rides in its own PR, not on
  the accepted branch.** That is what the follow-up issue is for: the accept
  names it, the author files it, and it is worked on a fresh branch. Pushing
  the fix onto the reviewed branch invalidates the head the verdict is bound
  to and spends review capacity the budget does not have. Once an accept is
  posted, an earlier direction to fix something in-PR is superseded — the
  item becomes a follow-up on a fresh branch, because an accept binds to a
  head — structurally, not by seniority — and a direction issued when no
  head was protected cannot authorise moving one. PR #277 is the instance:
  comment `6191002802` (11:29:40Z) told the author to close the enumeration
  in-PR; the round-2 accept `5532580105` (11:46:33Z, head `5c73ddc`)
  routed the same fix to a fresh branch — the accept wins.

  **If author text does land over a posted accept, it is an author fault**
  — not a provisioning defect; the harness did nothing. Handling matches a
  branch collision: name it in the PR body, report it on the author's
  tracking task. No penalty attaches, and it does not deadlock the branch.
  The remedy is the post-accept re-bind round below, and erasing the push
  is not one — PR #277 pushed `279e9600` at 11:46:54Z (21s after accept
  `5532580105` at 11:46:33Z) and force-pushed it away at 11:51:01Z instead
  of taking that round.
  Text arriving in the tree that is *not* the author's — a foreign path
  mid-task — is not this rule; that is the mid-task tree check in *Workflow*.

  **The reviewer re-binds under a post-accept round.** Its properties, all
  of them:

  - **Uncounted.** Like a carry-over round it was forced, not earned.
    Refusing on budget grounds would strand a green head with no verdict and
    no path to one.
  - **Scope is the delta only** — `git diff <discharged-head> <new-head>` —
    plus any interaction between that delta and the text already accepted.
    It may not re-litigate the accepted diff; that diff already carries a
    verdict. Narrow scope is a scope limit, not a lower bar.
  - **Same reviewer.** The only question the round asks is whether the delta
    still satisfies the verdict they posted. That judgment is theirs.
  - **Once per PR.** A second post-accept push on the same PR is not
    re-bound by the reviewer; the PR goes to Thrane, and whether it proceeds
    is a decision about the task.
  - Announced and verdicted like any round, per *Review of record*.

  **The reviewer re-binds under a reconciliation re-bind round.** This round
  is `main`'s; the post-accept round above is the author's. A verdict is posted
  at a head; the strict up-to-date policy of `ruleset 24735400 v53013064`
  forces a head move; `main` has edited an interacting part of a reviewed
  path; and the combination is defective in a way that needs added text at
  the merged head. The merge is clean, so *Verdict carry-over* condition 1
  flags nothing, and its condition 2 fails by the letter the moment the
  reconciliation text is added. The *A carry-over round does not count
  against the budget* clause does not cover
  it either: its uncounted branch requires `main` to have moved an
  *unrelated* part of the shared file, and its tail sends a defect-finding
  carry-over round to a counted author-text round, which a PR at its two
  rounds does not have. This round carries the post-accept properties, all of
  them:

  - **Uncounted.** It is forced by that strict up-to-date policy, not earned
    by the author. Refusing it on budget grounds strands a green accepted
    head with no verdict and no path to one.
  - **Scope is the combination and the reconciliation delta only** — the
    merged file read as one unit, plus `git diff <verdict-head> <new-head>`
    and the interaction between that delta and the text already accepted. It
    may not re-litigate the author's diff; that diff already carries a
    verdict. Narrow scope is a scope limit, not a lower bar.
  - **The author's reviewed hunks stay byte-identical, and that is the
    boundary condition.** If the fix moves the author's own hunks, or reaches
    past the seam into fresh argument, it is a counted author-text round —
    and on a PR already at its two rounds that is Thrane's decision about the
    task, taken before the round is announced.
  - **Same reviewer.** `main` edited a reviewed path and the incoming hunks
    interact with the reviewed ones, so *Verdict carry-over*'s mechanical
    branch is unavailable: naming the incoming hunks and judging the
    interaction is judgment, and the judgment is the reviewer's. No presser
    takes this PR on a silent or mechanical re-bind.
  - **Once per PR.** A second forced reconciliation on the same PR goes to
    Thrane, and whether it proceeds is a decision about the task.
  - Announced and verdicted like any round, per *Review of record*, gate read
    first. **The announcement and the verdict name the kind** — *uncounted
    reconciliation re-bind round* — so no presser miscounts it as a third
    author-text round. The press reads round state off those two artifacts, so
    the mandate belongs on them and not on the seat marker alone; the marker
    carries it too: `review-seat: PR #<n> uncounted reconciliation round head
    <sha>`.

  **PR #366 is this round's reason,** and the record is checkable. Round-2 accept
  `5545784972` at head `f1d0004e0fc3154e5c5351520b965af42a7f940b`; `main` at
  `999a6da8669399bc642a3feff09cf57f225890b1`, which edits the same paragraph.
  `git merge-tree --write-tree f1d0004 999a6da` is rc 0, tree
  `fbe2b376b94b60bbadfcb3986023f64308f8d5b6` — clean, and it produces a seam
  that contradicts itself: merged `AGENTS.md:869` reads `` !! home/ `` while
  merged `:874`, three lines later, calls "That `?? home/`" the oracle read
  from the worktree toplevel. Reconciling that takes added lines at the
  merged head, and both of this PR's author-text rounds were already spent.

  **PR #227 is the post-accept round's reason,** and the record is checkable. Round-2 accept at
  `a22443d` — review `5526943831`, submitted `2026-10-15T01:31:06Z`. The
  author filed the accept's follow-ups (gh#230, gh#231) in PR comment
  `6182320720` at `01:31:53Z` — proof the verdict was read. `acb573e`
  committed at `01:50:08Z`, nineteen minutes after the verdict and eighteen
  after it was acted on; `b423b86` at `01:52:15Z`. PR comment `6182647981`
  at `01:58:54Z` then called the verdict withdrawn as unread. `git diff
  a22443d acb573e` is 206 insertions / 124 deletions across
  `src/book_value.rs` and `src/tests/book_value_importer.rs` — the author's
  own text, on a head a verdict already named — so *Verdict carry-over*
  condition 2 fails on the knife-edge it names, and the accept never reached
  the new head. `src/tests/db.rs` also differs between `a22443d` and
  `b423b86`, but that is PR #228's `9abee78` and `6f0aa96` arriving with the
  merge of `main` — the blob test condition 2 warns against, not the
  author's. Attribution stays re-runnable on merged history with per-file
  `git log --no-merges a22443d..b423b86 -- <path>` plus
  `gh api repos/joshuaflinn/hireling/commits/<sha>/pulls`, where condition
  1's `--not origin/main` form reads empty once both PRs are on `main`.

  The press rule is separate from the budget and unchanged: every announced
  round at a head has a verdict against it before the press, a narrower
  accept does not close a wider round still writing, and where two verdicts
  at one head disagree, the wider scope counts (PR #158).
- **Verdict carry-over.** A verdict re-binds to a new head without a new
  review round only when **all three** of these hold, each posted as
  checkable evidence on the PR:
  1. **Clean merge.** The head move is a merge of `origin/main` into the
     branch with an empty conflict set and no new commits of the author's
     own. Evidence: `git log --merges -1` names the merge, and `git log
     --no-merges <old-head>..<new-head> --not origin/main` returns empty.
     The first command alone shows only the most recent merge — it cannot
     prove the head move carried nothing else.
  2. **Identical contribution.** The author's own text is unchanged at
     the new head. For every reviewed path, the added and removed lines
     of `git diff <old-base>...<old-head>` and `git diff
     <new-base>...<new-head>` are identical; `index` lines and `@@`
     offsets may differ, nothing else may. The bases are the merge-bases
     with `origin/main` at review time and at the new head. Mechanical
     form, run once per reviewed path, the two sides' outputs for that
     path compared equal: `git diff --output-indicator-old=O --output-indicator-new=N <base>...<head> -- <path> | grep -E '^([ON]|\\)'`. Where `main` has edited a
     reviewed file, the reviewer also names the incoming `main` hunks and
     states they do not interact with the reviewed ones. The knife-edge
     is the author's text: it moved, new round. A blob test is not
     enough: it also fires when `main` arrives in a reviewed file with
     nothing of the author's riding in — the PR #155 sequence; evidence
     in PR comments 6151802113 and 6151849718.
  3. **Gate green at the new head.** The `gate` check is `success` at the
     new head, read off the check-runs API. The gate always re-runs — the
     gate certifies the merged tree, the review certifies the diff, and a
     text comparison cannot see a semantic interaction with new `main`
     code.

  When all three hold, the re-bind goes on the
  PR as a review comment naming the old head, the new head, and the evidence
  for each condition. The routing splits on condition 2's own branch. Where
  `main` has not edited a reviewed file, the re-bind is wholly mechanical —
  a merge-commit shape, the author's added and removed lines compared equal,
  a check-runs status — so there is no judgment in it for a reviewer to
  supply: it does not have to be the reviewer, it does not open a review
  round, and routing it back to them buys a round-trip against a diff that
  did not move. Where `main` has edited a reviewed file, naming the incoming
  `main` hunks and stating they do not interact with the reviewed ones is
  judgment, and the judgment is the reviewer's — the re-bind routes to them.
  PR #157 is the live proof that this case is real: an accepted verdict whose
  head had to move while `main` landed interacting prose in the same bullet.
  The verdict itself is still the reviewer's and only the reviewer's; the
  mechanical re-bind is bookkeeping over their verdict, not a second one.
  That comment is the record, same as any verdict — a carry-over is never
  silent. If any condition fails, it is a new review round — counted and
  author-text, unless condition 2 fails only because `main`'s interacting
  hunks forced reconciliation text at the merged head, which is the uncounted
  *reconciliation re-bind round* in *Review rounds*. Full stop on the rest.

  The head move itself is not waste and is not negotiable. `main` carries the
  `main-gate` ruleset (id `24735400`), which at version `v53013064` requires
  the head be up to date before merge, and that is what forces condition 3 to
  mean anything: the `gate` workflow triggers on `pull_request`, `merge_group`
  and `push: branches: [main]`, and a pull-request run checks out
  `refs/pull/<n>/merge` — the head merged into `main` *as of when the run
  fired* (run 38410922309: `HEAD is now at 20b990b Merge c878e65 into
  c9df4f8`). No run re-fires when `main` moves underneath a still-open PR, so
  without the up-to-date requirement a PR can merge on a `gate` success that
  certifies a merge base `main` has since left behind. Dropping it would make
  condition 3 a stale certificate.
- **Claim before you code.** Only start an issue that is *unassigned*, and
  self-assign it on GitHub before writing any code. If a human holds it, it's
  theirs — pick another.
- **Work in your provisioned workspace.** The harness provisions every run a
  workspace: `<project>/MOR-<n>`, a sibling worktree on branch `MOR-<n>` cut
  from `origin/main`. That branch is the PR branch; the run's own harness
  prompt forbids switching or re-pointing it, so keep all commits on it.
  Inside that tree the HEAD stays on `MOR-<n>` for the run's whole life —
  `git checkout`/`git switch` onto another branch and `git rebase` of the
  provisioned branch are both out; a rebase moves the branch under the
  seats sharing the tree, and the force-push that would make it reachable
  is already forbidden above. That pin binds this tree only — the gate
  worktree recipe takes `--detach` in a *separate* worktree and takes no
  commits. The consequence: a concurrent seat in that same tree fails
  `workspace_validation_failed` (`git_worktree_branch_incoherence`), which
  reads to it as a provisioning defect. gh#149 is the structural cause —
  one worktree per issue *lineage*, so sibling seats share one HEAD —
  and PR #271, the gh#267 instance, cost a concurrent seat one
  heartbeat, no text.
  Never cut your own worktree for your own task — verify where you are with
  `git rev-parse --show-toplevel` and work there — with one exception:
  work the provisioned branch cannot carry — a side fix, another issue's
  docs change, a hotfix, a branch collision. The keep-all-commits
  instruction carries a premise: the provisioned branch is *this* task's
  PR branch, 1:1. Check the premise at pickup (`git branch
  --show-current`, then `gh pr list --head <branch> --state open`, then
  `git status --short`): if the branch is already another open PR's
  head, if it carries another task's commits, or if the
  tree holds modified or untracked paths this task will not touch, the
  premise has failed — committing would move a review head that a verdict
  may be bound to, or sweep a foreign path onto it. The tree check is
  co-equal with the branch checks: a correctly-provisioned workspace can
  still hold another seat's uncommitted work, and it passes both branch
  checks. Foreign WIP is not yours to clean — no `git checkout --`, no
  `git stash`, no delete (the stash stack is shared across worktrees, and
  the paths may be another seat's live work). Leave it. Do not commit to it;
  hand-cut a sibling worktree `hireling-<n>-<slug>` on a prefixed branch
  (`docs/<n>-<slug>`-style, Hard rule 3) cut fresh from `origin/main` —
  never a `MOR-<n>` name, that namespace is harness-owned and carries the
  stricter removal rule — note the collision and its evidence in the PR
  body, and report it on your tracking task. It is a provisioning defect,
  not an engineer error. The shared `hireling`
  checkout is reference only: read from it, never run in it, and let it
  drift behind `origin/main`. Hand-cut worktrees — a side fix, another
  issue's docs change, a hotfix, a branch collision — go in sibling
  directories, never run scratch dirs (those get cleaned). Removal has a
  hard edge: a sibling named for an *open* task
  (`MOR-<n>`) can be clean and merged by any check you like and still be a
  live harness-owned workspace — never remove one unless that task is
  closed. Hand-cut `hireling-*` siblings use the clean-tree test:
  a clean tree — `git status --porcelain` empty — means nothing tracked or
  visibly untracked in it can be lost, and any agent may remove
  such a sibling on sight — `git worktree remove` leaves the local branch
  ref in place, so the commits stay reachable whatever the merge state, and
  the clean-tree check carries that weight alone. The ancestry conjunct is
  dropped because `main` squash-merges and a merged branch is normally *not*
  an ancestor of `origin/main`; for a belt-and-braces merge check, read PR
  state off `gh pr list --head <branch> --state all` (merged or closed).
  Never `worktree remove --force`; a dirty tree belongs to its owner.
  A provisioned `MOR-<n>` branch pointer is not durable between runs. The
  harness re-points it to current `origin/main` when a new run in the lineage
  starts — `MOR-482`'s reflog carries four `reset: moving to <sha>` entries
  before any commit landed on it, and PR #277 binds agents, not the harness.
  Push every commit to `origin/MOR-<n>` in the same heartbeat you make it —
  unless a review round is open against the PR, where *The author holds the
  head while a round is open* governs: the head is held, so do not make the
  commit until the verdict lands. An unpushed commit survives only in one
  local disk's reflog. If that push
  would need `--force`, the branch has already moved under you — stop and
  escalate it as a provisioning defect, same handling as a branch collision.
  Do not force-push, and do not rebase to make the push fast-forward; that is
  the move PR #277 bans.
- **The tree check is not only a pickup check.** A foreign path can appear
  in your tree *mid-task* — after pickup, while a review verdict is live —
  and it gets the same handling as a branch collision. Do not clean it:
  `git checkout --`, `git stash`, and delete stay off the table for the same
  reasons as at pickup (the stash stack is shared across worktrees, and the
  text may be another seat's live work). Do not commit it blind either:
  stage your own paths by name, never `git add -A` or `git commit -a`,
  because that is the mechanism that sweeps a foreign path onto a head a
  verdict may be bound to. Staging by name keeps the foreign path out of
  your *commit* — not out of your commit's hook *when your commit stages a
  `.rs` or `.toml`*: the hook's fmt, clippy and test legs then read the
  whole tree, not the staged paths (`.githooks/pre-commit:58,62,69`, and
  `:46` for why), so `cargo fmt --all` rewrites a foreign `.rs` path in
  place, and clippy/test fail on foreign code that does not compile. A
  commit staging neither `.rs` nor `.toml` exits at
  `.githooks/pre-commit:40-42` before those legs run, so it is legal and
  green with the foreign path still in the tree — commit your own paths by
  name and escalate anyway. One leg does run before that exit: the JSON
  duplicate-key check at `.githooks/pre-commit:28-33` reads every tracked
  `*.json` from the working tree (`scripts/check_json_dup_keys.py:204,219`)
  and rejects invalid as well as duplicate-keyed JSON (`:228-230`), so a
  foreign tracked `.json` left mid-edit fails even a docs-only commit — no
  legal commit there either, same handling: stop and escalate. Where the
  legs *do* run and the foreign text
  does not compile and you will not adopt it, there is no legal commit —
  `--no-verify` is forbidden and the hook legs fail on the tree — so stop
  and escalate it as a provisioning
  defect, the same class as a branch collision, naming the owning seat where
  you can tell it and Korrin otherwise; do not clean it, do not commit it,
  do not route around the hook. Note it in the PR body and report it on your
  tracking task. If you judge the foreign text worth keeping, it is yours to
  verify line by line before it rides in your commit, and the PR body says
  so — you are its author at that point. PR #233 is the reason, and the
  record is checkable: a foreign edit landed malformed in
  `src/book_value.rs` mid-task, the pre-commit clippy leg caught it at
  `src/book_value.rs:107`, and no commit ever carried the broken state — a
  hook is a catch, not a control, which is why the rule is written down.
  This is foreign text; author text landing over a posted verdict is the
  author-fault rule in *Review rounds*.
- **Gate around contamination, not through it.** When the tree check fires,
  the provisioned tree usually will not compile, so `just ci-local` cannot
  run in it. Run the gate in a detached worktree at the PR head — the
  output certifies the reviewed tree, and a detached checkout takes no
  commits:

  ```
  git worktree add --detach ../hireling-gate-<n> "$(git rev-parse <pr-branch>)"
  cd ../hireling-gate-<n>
  npm --prefix web ci
  CARGO_TARGET_DIR="$PWD/../target-gate-<n>" just ci-local
  cd - >/dev/null
  git worktree remove ../hireling-gate-<n>
  rm -rf ../target-gate-<n>
  ```

  `CARGO_TARGET_DIR` points outside the worktree so the build cannot
  collide with a concurrent build in the sibling set, and the gate's
  build output never lands inside the tree `git worktree remove` has to
  clean up. Paste the gate output from *that* tree in the PR body and
  name the tree it ran in.
- **Three sync points, nothing continuous.** Pickup: self-assign and comment
  the branch name. PR open: `Closes #N` plus a link back to your tracking task.
  Merge: GitHub closes the issue.
- **Gate yourself before the PR.** Run `just ci-local` (fmt / clippy-deny block /
  tests / cargo-deny) and paste the result. Any count you carry into the PR body
  names the command that produced it — warning census, test count, line count. A
  figure nobody can re-run can only be believed, not checked, so the command goes
  on the page next to the number. CI runs **grizzly-gate** (standalone
  mode, pinned image) on every PR — a red gate is a red PR, no exceptions. The
  lint/test configs are vendored from Bear's rust-toolkit; if a lint fights you,
  propose tuning it in a PR, don't `#[allow]` around it.
- **A behaviour claim in the PR body names the test that proves it, per claimed
  input.** The count rule above extends to behaviour, and the fail-closed class is
  where it bites: a claim that the change *rejects*, *refuses* or *fails closed* on an
  input is a claim nothing exercises incidentally — a working feature proves the accept
  path and never the reject path — so each input the claim names carries its own
  assertion through the production path that owns it, and the body names that assertion.
  Where a named input has no assertion, the body says "not tested" for that input
  instead of stating the behaviour as fact. A list of inputs is a list of claims: one
  test behind a three-item list certifies two items nobody checked, the same fault as a
  single citation over a whole rules table. Shipped text is held to the same bar — a
  fail-closed claim in a doc comment is a behaviour claim, and it ships with the
  assertion or it does not ship. PR #337 is the reason, and the tell was in the claim
  itself: it named "chained `just a && just b`" as failing closed and "two new
  self-tests, arged-green/multiline-red" as its coverage in one sentence. At head
  `88aca54` that input derives `alpha`, and so does `run: just alpha; true` — a step
  that can never red, reported as a CI home that can.
- **Bring the branch up to date before you ask for review.** `main` is guarded
  by the `main-gate` ruleset (id `24735400`), which at version `v53013064`
  enforces a strict up-to-date status-check policy, so a branch BEHIND `main`
  is a hard block at the merge button however green the gate is. Merge
  `origin/main` in and push *before* the review ask, so the run that goes
  green is also the run that can merge. This narrows the carry-over case
  above; it does not remove it, because `main` can still move during a review
  — that is what *Verdict carry-over* is for.
- **Read `main`'s merge rules off the ruleset API, never branch protection.**
  `main` is guarded by the `main-gate` ruleset (id `24735400`, enforcement
  `active`), and `GET /repos/joshuaflinn/hireling/branches/main/protection`
  404s on this repo — which reads as "`main` is unprotected" and is wrong.
  What is actually required, at what strictness, is visible only via
  `gh api repos/joshuaflinn/hireling/rulesets`. Read it there before you
  assert anything about what blocks a merge. When a procedure doc states a
  ruleset rule, it states the rule operatively and cites a named ruleset
  version — `ruleset 24735400 v53013064` — never the live value.
- **Sweep for reachability before you hand over.** Walk the spec's
  requirement list — the requirement list, never the module's export list —
  and, for each one, name the production-path test that proves it and the
  call site that reaches it. The unit of the census is the **named action
  the requirement is implemented as**, whatever its shape: an exported
  function, a method on an object a factory returns, a store action, a
  handler a component is passed. A reachable factory says nothing about the
  actions it returns, so counting the factory is not a sweep. Where the
  requirement landed in a library module, grep for a caller of that action:
  `grep -rn "<action_name>" src web/src`. Hits confined to the defining
  module and its tests mean the requirement is unbuilt — a green unit test
  does not change that. Report the sweep in the PR body alongside the gate
  output, one row per requirement naming the action, its caller, and its
  production-path test. PR #311 is the reason the unit is written down: its
  sweep counted module exports, `createStashState` among them with callers
  in `web/src/App.svelte`, and reported everything reachable — while
  `adjustQty`, `editMeta` and `adjustBank`, the actions that factory
  returns, had no caller anywhere in `web/src` at head `c7e7e3c`.
  `adjustBank` is `specs/014-shared-inventory` FR-7's party bank and
  carried nine test references with no UI path to it. An export census that
  passes while three spec'd actions are unreachable cannot fail the way the
  sweep was meant to.

  **The sweep walks what the diff removes, not only what it adds.** Read the
  deleted lines for call sites
  (`git diff <base>...<head> -- src web/src | grep '^-' | grep -v '^---'` — a
  git pathspec is root-relative, so `-- src` alone matches nothing under
  `web/`, and the `--- a/<path>` headers read as deleted lines), and for
  every symbol whose last production call the diff deletes, run the same
  `grep -rn "<symbol>" src web/src`. Hits confined to the defining module
  and its tests mean the diff made that symbol unbuilt — report it, the same
  as an unbuilt requirement. PR #250 is the reason: its sweep was clean over
  every symbol the PR added, while replacing two `authorize(...)` calls with
  a hand-rolled membership `if` deleted the only production callers of
  `Resource::Stash` and `Resource::PartySettings`, so two of
  `specs/014-shared-inventory` design D4's three
  authz-matrix rows went dead in production with `gate` `success` at head
  `275b2c1` (check-run `116275069010`) and `src/auth/tests/authz.rs` still
  testing code nothing called.
- Priority order is P0 → P1 → P2 as tagged in the PRD's functional requirements.
- Feedback on the PRD itself goes to the PRD (via issue or PR against `docs/PRD.md`),
  not into code comments.

## Style

- Grug-brained (see Constitution Article II). Flat code, locality of behavior,
  name your intermediate variables.
- Integration tests are the sweet spot. The modifier engine gets exhaustive unit
  tests; everything else gets a curated end-to-end suite.
- **A test for a spec'd behavior must exercise the production path that owns
  it.** Testing the module underneath that path proves the module, not the
  behavior. Six instances of the same rule:
  - *Observability* (audit, log, metric): send an HTTP request through the
    configured router for HTTP behavior, or call the public orchestration
    function for non-HTTP behavior; then assert the persisted record or
    captured emission. Calling the recorder or emitter helper directly cannot
    catch a missing call from that production path.
  - *A user-visible affordance*: render it through the component, in the state
    the spec describes. `web/` runs `vitest run` against jsdom (config in
    `web/vite.config.js`); mount with `@testing-library/svelte`'s `render()`
    and assert what the user can see — by role, by text, by placeholder. A
    render that only passes the default or empty value never enters the branch
    the spec is about, so exercise the non-default state.
  - *An affordance that lives in the wiring* — anything riding an `$effect` or
    an event handler: drive it. `fireEvent.input`, `.click`, `.keyDown` against
    the mounted component, then assert the DOM the spec promises. jsdom runs
    effects and handlers, so "the store is correct and unit-tested" and "the
    helper accepts an `onCommit`" are not proof — a store nothing subscribes to
    and a callback nothing passes are unverified however many unit tests they
    carry. The event is what proves the handler is wired.
  - *A field the spec sources from imported data*: prove it with two fixtures
    that differ in that field, asserting two different results. One fixture
    asserting one literal also passes against a hardcoded constant.
  - *A number the game rules fix* (the level a feature is gained at, a DC, a
    dice step): assert both sides of the threshold — `threshold - 1` and
    `threshold` — with different results. One fixture on one side passes
    against every wrong threshold on that side. See
    [Rules exactness](#rules-exactness) for the full rule, citations included.
  - *A pinned wire contract* — a fixture naming the set a production path
    accepts: assert the fixture against the gate that enforces it, never
    against a literal restated in the test body. Feed each pinned name
    through that gate and assert it is accepted; panic on a pinned name no
    arm handles; and assert an outside name is refused on every target kind
    the dispatch routes to, or the accept loop goes green against a gate
    that accepts everything. That reaches a stale fixture and a dropped
    name, never a gained one — the loop only sees names the fixture lists;
    where the set is an enum, derive it off `EnumIter` and the gain is
    caught too. The shipped forms, enum and hand-written match:
    `the_pinned_vitals_fields_are_the_serde_wire_names_of_the_enum` and
    `the_pinned_effect_ops_are_the_write_paths_accepted_set` in
    `src/tests/sync/contract.rs`.
- Log generously on the backend: major branches, request IDs.
- Commit messages: imperative, one line, what + why if non-obvious.

## Rules exactness

A green gate cannot catch a rules error: no lint, no clippy rule and no test
count distinguishes a correct Pathfinder 2e number from a wrong one. PR #73
proposed a class-feature table wrong in eight of eight rows with `just ci-local`
green and 364 tests passed — review caught it before merge; the gate could not
have. The only guardrails that work are a citation that can be checked and an
assertion that moves when the number moves. Both are required for any constant,
table row, or branch that encodes a rules fact.

The rules corpus is conditions + equipment only (`src/import/model.rs`); there
is no corpus for class features, so class-feature tables and level thresholds
are hand-maintained by necessity. "Derive it from the corpus" is not available
here — which is why the citation rule carries the weight it does.

1. **Cite per row, not per table.** Any constant, table row, or branch that
   encodes a Pathfinder 2e rules fact names the source book and page for *that
   value* in its doc comment. A single citation covering a whole table is not
   acceptable — it certifies rows nobody checked, and a wrong citation is
   worse than none because the next reader stops checking. Where a value's
   book differs from the table's (Magus is *Impossible Magic*, not *Player
   Core*), the row says so. Write the citation off the source page — never
   from memory, and never from a review comment.
2. **A boundary assertion per asserted value.** A test guarding a rules
   threshold asserts both sides of it — at `threshold - 1` and at `threshold`,
   with different results — through the production path that owns the value.
   One fixture on one side of a threshold passes against every wrong threshold
   on that side, so it guards nothing. This is the rules-threshold instance of
   the two-fixture rule in [Style](#style): a sourced number is proven by two
   inputs that differ in it, asserting two different results.

## Humans

- **Josh** — PM, owner. **Dave** — co-dev, owner. Both build agentically.
- **Vex** — Josh's engineering agent (PM/eng on this project).
- Constitution amendments need both owners. Everything else, one owner's PR
  approval suffices.

## Tool output never goes to a host path

The shell's cwd is a checked-out worktree, not a neutral directory. A name that
looks absolute stops being one the moment the leading `/` is lost — stripped by
a wrapper (`"${HOST_PATH#/}"`), dropped in interpolation, or read slash-less out
of a tool response or a path variable — and once it is relative, a `mkdir -p`
over the same string creates the parent chain *under the worktree*, so the write
lands in the tree. The root shape of it is git-ignored by `.gitignore` (`/home/`,
`/.tmp/`, `/.paperclip/`), so plain `git status` does not show it — `--ignored`
does:

```
$ mkdir -p home/flinn/.tmp/paperclip-<run>
$ echo '{"e":5}' > home/flinn/.tmp/paperclip-<run>/x.json
$ git status --short                 # (no output — /home/ is ignored; see .gitignore)
$ git status --short --ignored
!! home/
```

Both lines are needed. Without the `mkdir -p` the redirect exits 1 with `No such
file or directory` and nothing is contaminated — measured in a clean tree. The
`mkdir -p` is the step that makes it reproduce, so it is part of the
reproduction, not a precondition to footnote.

A literal absolute path does not behave that way. Point `$W` at a directory that
is not beneath the tree, and each outcome is a command. Pointing `$W` *at* the
tree does not test the claim: `"$W/out/x.json"` is then an absolute path that
*is* under the tree, and the block demonstrates contamination instead.

- *Parent exists → lands at the absolute path, tree clean.* From a clean tree:

  ```
  $ W="$(mktemp -d)"                      # a directory NOT beneath the tree
  $ mkdir -p "$W/out"                     # rc 0
  $ echo '{"e":5}' > "$W/out/x.json"      # rc 0
  $ test -f "$W/out/x.json" && echo wrote # wrote
  $ git status --short                    # (no output)
  ```

- *Parent missing → rc 1, tree clean.* Same tree, no `mkdir`:

  ```
  $ echo '{"e":5}' > "$W/absent-9x/y.json"  # rc 1: No such file or directory
  $ git status --short                      # (no output)
  ```

  Read `git status` in a tree that is clean *before* the write. Contamination is
  sticky — an `!! home/` from an earlier repro stays listed under `--ignored` and
  reads as if the absolute write caused it.

What contaminated a workspace here was a host path carried without its leading
`/`, so the rule is about the name, not the redirection:

- Never name an output file from a host path **carried in from outside this
  worktree** — as a literal, a variable, a template, or a value read from a tool
  response or a path-stripping wrapper. Every directory outside this worktree is
  a candidate — `/tmp`, a service home, another repo's checkout.
- Never `>`, `tee`, or an `--output`/`-o`/`-f` flag to a path built by
  concatenating host-path components **carried in from outside this worktree**.

The one exemption is the directory this run's harness names for you, enumerated
in the paragraph below; it is not inferred from a path's look.

The bullets are about where the name came from, not its shape, because shape
cannot decide it: `$PAPERCLIP_RUN_SCRATCH_DIR/x.json` and
`"${WORKTREE_DIR#/}/x.json"` (`$WORKTREE_DIR` is a stand-in for a worktree path,
not an env var this harness sets) are both an absolute path in a variable, used as
a template — and only the second is what the bullets mean. From a clean tree, the
scratch variable passes whole and stays clean; the stripped worktree path needs
one `mkdir -p` and then contaminates:

```
$ echo '{"e":5}' > "$PAPERCLIP_RUN_SCRATCH_DIR/x.json"   # rc 0
$ git status --short                                     # (no output)
$ mkdir -p "${WORKTREE_DIR#/}"                           # the step that makes it reproduce
$ echo '{"e":5}' > "${WORKTREE_DIR#/}/x.json"            # rc 0
$ git status --short                                      # (no output — /home/ is ignored)
$ git status --short --ignored
!! home/
```

What the scratch dir is *not* is exempt. Used whole it is the prescribed
substitute; run through a stripping wrapper or a slash-less interpolation it
reaches the tree like any other absolute path. A stripping wrapper is not picky
about which absolute it eats. Here is the same tree's own path taken from
`git rev-parse --show-toplevel` — a value read straight out of a tool response —
stripped and run through the mechanism. In a tree clean before the write:

```
$ W="$(git rev-parse --show-toplevel)"
$ U="${W#/}"
$ mkdir -p "$U"                                      # rc 0
$ echo '{"e":5}' > "$U/x.json"                       # rc 0
$ git status --short                                 # (no output — /home/ is ignored)
$ git status --short --ignored
!! home/
```

The scratch dir is no further from the tree than that, and it fails the same way:
`mkdir -p "${PAPERCLIP_RUN_SCRATCH_DIR#/}"` then a write to a child of that
string is rc 0 and yields `!! home/` under `git status --short --ignored`.
Write to that same string *itself* and
`mkdir -p` has just made it a directory — rc 1, `Is a directory`, tree still
clean — the `mkdir -p` has left the directory chain in the tree and `git status`
cannot see it, because git does not track empty directories, so this form fails
safe only until something writes into it. From the worktree toplevel that write
reads `!! home/` under `git status --short --ignored`, never `?? home/`: `.gitignore`
anchors `/home/` at the root, so plain `git status` is silent on it. Run the same
write from a *tracked* subdirectory and the entry appears as `?? home/`, because
the entry is relative to cwd and `mkdir -p` built the chain under cwd — while the
toplevel still reads `?? src/home/`, since `/home/` is root-anchored and never
matches `src/home/`. `?? ./` appears only where cwd is itself
untracked, and there it fires whether or not anything contaminated the tree, so
it is not an oracle.
Whether the scratch dir sits beside `$HOME` or under it does not decide
anything, which is why `$HOME` is not the test; the condition is whether the
absolute value survives intact to the redirect. The earlier "outside every
worktree precisely so this cannot happen" over-claimed on that point.

Write output to a workspace-relative path you chose and clean it up, or — on a
Paperclip seat — under `$PAPERCLIP_RUN_SCRATCH_DIR`, passed whole. It is run-owned
and outside every worktree, which is why it is the substitute; it does not need to
sit outside `$HOME`, and it earns nothing by passing through a wrapper.

`set -C` is not a guard, for two reasons, and the first governs: it blocks only
*overwriting an existing file*, and a contaminating path is always new
(`set -C; printf x > fresh.json` returns 0, where the same redirect against an
existing file is refused). The second is that a tool writing its own output is
never blocked either — `tee` and `curl -o` both overwrite under `set -C` at rc
0 — and `set -C` is not inherited by a child shell.
