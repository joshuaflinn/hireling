# Contributing to Hireling

Friends-and-family project, professional standards. Two owners (Josh, Dave), both
building with AI agents. This file is the human side of the workflow; `AGENTS.md`
is the agent side; `CONSTITUTION.md` governs both.

## The loop

1. **Pick a GitHub Issue.** Work that's not an issue doesn't exist. If you have an
   idea, file it — triage is a conversation, not a gate.
2. **Branch.** `issue-<n>-<slug>` off `main`. One issue, one branch, one PR.
3. **Build.** However you like — by hand, by agent, by interpretive dance. The PR
   is what's judged.
4. **PR.** Small beats clever. Description carries the *why*: what changed, what
   you rejected, what you verified (tests/build output, not vibes). CI runs
   **grizzly-gate** (Bear's quality gate, standalone mode) — the gate is the
   first reviewer; a human is the second.
5. **Human review, human merge.** The reviewer is never the person (or session)
   that wrote it. Either owner can merge; constitution changes need both.

## Ground rules

- **The PRD is the spec.** If the code and `docs/PRD.md` disagree, one of them is
  wrong — figure out which before merging. PRD changes are PRs too.
- **Respect the non-goals.** No combat tracker, no character builder, no dice
  roller, no GM homework. The constitution explains why.
- **Agents propose, humans dispose.** Never merge agent output you haven't read.
  If you can't explain what a PR does, it isn't ready.
- **Keep the stack boring.** New dependency? Justify it in the PR.
- **Be decent.** Six users, all friends. Feedback on code, not people. Dave's
  bestiary data entry is art and we treat it as such.

## Setup

[TBD — filled in when the skeleton lands: rust toolchain, `sqlx-cli`, Node/pnpm,
`docker compose up` for local postgres, `cargo run` / `pnpm dev`.]
