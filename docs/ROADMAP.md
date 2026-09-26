# Hireling Roadmap

The map, not the territory. This file carries **no state of its own** — facts
live with their owners, and this page only points at them:

- **What we're building and why** → `docs/PRD.md` (the law's first deputy)
- **Build order, phases, dependencies** → `docs/EPICS.md`
- **The ledger of actual work** → GitHub issues
- **What's forbidden** → `CONSTITUTION.md`

If this file and one of those disagree, this file is wrong. Fix the pointer,
not the fact.

## Where we are

**Phase 1 — the product.** Phase 0 is closed: scaffold, schema, OIDC auth and
the rules importer are merged (issues #3–#6). Two lanes run in parallel off
that floor — Lane A opens with E5 Pathbuilder import (#7), Lane B with E7
party sync (#9). Both converge on E8, the buff engine (#10), which needs E4,
E5 and E7 before it can start.

## The shape of the build

| Phase | Contents | Issues |
|-------|----------|--------|
| 0 — Foundations | Scaffold, DB schema, OIDC auth, rules importer | #3–#6 |
| 1 — The product | Lane A: import → sheet → tooltips. Lane B: sync → party view. E8 buff engine is the sync point | #7–#12 |
| 2 — Go live | Mimir container, tunnel, hireling.flinntech.com | #13 |
| 3 — P1 depth | Shared inventory, spell library, GM stat density, curation editing | #14–#17 |
| 4 — P2 parking lot | Phone layout, dice roller, multi-party UI, GM encounter view, scenario-aware views | #18, #19 |

Full dependency map and per-epic specify prompts: `docs/EPICS.md`.

## Ideas inbox

**Anyone can drop an idea here. One line, your name, a date. No triage
pressure, no format police.**

This box is pre-work capture, not work — per `CONTRIBUTOR.md`, work that's
not an issue doesn't exist. When an idea gets picked, it becomes a GitHub
issue and leaves this list. Bad ideas stay here comfortably forever; that's
fine too.

<!-- Example format:
- 2026-09-18 · Dave · "dice tray that only appears when you tap a weapon" 
-->

_(Empty so far. First one's yours, Dave.)_
