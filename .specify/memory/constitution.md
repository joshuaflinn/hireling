# Hireling Constitution

The non-negotiables. Every PR, every agent session, every roadmap conversation is
accountable to this document. If code conflicts with the constitution, the code is wrong.

## Article I — The PRD is law

`docs/PRD.md` defines what we are building and, more importantly, what we are **not**
building. The non-goals are load-bearing:

- No character builder. Pathbuilder owns character creation.
- No combat tracker. No initiative, no rounds, no turn timers.
- No GM workload. The GM seat is read-only by design.
- No dice roller until MVP+1, and only if real users ask.

Every project will drift toward a combat tracker. This one doesn't. Point at this line.

## Article II — Grug-brained development

Complexity is the enemy. Every decision reduces it or holds the line.

- Default answer to new abstractions, patterns, layers, frameworks, and services: **no**.
- Deliver 80% of the value with 20% of the code.
- Three similar lines beat a premature abstraction. DRY is a guideline, not a law.
- No speculative architecture. Build what the PRD requires, nothing more.
- It's OK to say "this is too complex." That's senior judgment.

## Article III — Humans decide, agents propose

Both owners build with AI agents. That's the workflow, not a secret.

- All work lands via PR. Agents never push to `main`.
- A human reviews and merges every PR. The reviewer is never the session that wrote it.
- Agents verify before claiming done: run the tests, run the build, paste the evidence.
- An agent that cannot verify says so. No plausible-sounding completion reports.

## Article IV — The modifier engine is sacred

The buff/effect engine (PF2e bonus stacking + provenance) is the technical heart of the
product and the one module designed for life beyond this repo.

- Pure and isolated: base stats + active effects in, derived stats + provenance out. No I/O.
- Exhaustively unit-tested against the core rulebook's stacking rules and worked examples.
- A bug in the engine is a stop-the-line event: reproduce with a failing test first, then fix.

## Article V — Boring stack

Rust (axum) backend, Postgres hall on Asgard, Svelte + hand-rolled CSS frontend, PWA.
New dependencies need a written justification in the PR. "It's popular" is not one.

## Article VI — The product promise

Players own their sheets. Casters own their effects. The app does arithmetic; humans do
everything else. Any feature that transfers judgment from a player to the app — aura
tracking, auto-expiry, positioning — violates the product's core deal and the PRD's scope.

## Governance

- This constitution supersedes the PRD, which supersedes any individual decision.
- Amendments require a PR approved by **both** owners (Josh and Dave).
- When the SDD/SpecKit pipeline engages, its constitution artifacts sync FROM this file.
