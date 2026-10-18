# The content ask — E13 seeded spell library (for Josh and Dave)

**From**: Korrin (PM seat, Moradin). **On**: gh#15 / MOR-764. **Status of the ask**: raised for
answering; **it blocks no code** — the code half is specced, designed and planned to ship
against an empty library, and merges on its own gate. When the content lands it is a data-only
PR and the code does not change.

**Read this first: what we are NOT asking.** We are not asking anyone here to write a spell
name or a line of template prose. The PRD says the templates are Dave's, reviewed by Josh,
never model-generated; that stands absolutely. This document is the *shape* to fill in, so the
ask is "fill this in," never "tell us what to build."

---

## 1. The ask for Josh — the spell list (config ownership)

**How many:** 20 entries, ±5. One entry per spell the party actually casts at the table. Fewer
is fine and better if the list is honest — 12 spells everybody uses beats 20 we half remember.

**What Josh decides, concretely:** the set of spells and their order of inclusion. That is all
the PRD asks of him ("the definitive list is importer seed config owned by Josh, chosen from the
party's observed usage"). Nothing about the file format is Josh's problem — he can hand the list
over as a plain line-per-line list of spell names in any channel, and a seat will transcribe it
into the config file **with Dave's templates attached**, then open the PR for Josh's review.

**One product decision that is Josh's, and only one — a single line answers it:**

> **Q1. When a caster taps a library spell's outcome, should it spend the spell slot?**
> Our answer for the build is **no** — the library is a composition shortcut; slots stay as
> import/live-state tracks them, and library casting never touches them. Say "yes" instead and
> it is a PRD change and a separate epic, because it couples this surface to import-owned state.
> *Nothing is waiting on this answer; it only tells us whether to file the follow-up.*

## 2. The ask for Dave — one template per spell (content authorship)

Per spell, Dave authors:

| what | how much | notes |
|---|---|---|
| The four outcome strings (`prose`) | one sentence to one short sentence-pair each | the prototype's paraphrase style — rules-accurate, AoN-linked. This is what the caster *reads* while tapping a degree. For a degree that does nothing, write the nothing plainly ("No effect.") — do not leave it blank |
| The effect rows per outcome (`outcomes`) | usually 1–2 rows on success, 1–3 on crit success, **empty** on failure for most buffs | each row is: stat + type + value + target rule. This is the mechanical half — the only part the engine executes |
| The duration note | one short string | free text; it is a note, not a timer |
| The citation | one string | book + page for the rules facts in that entry — per AGENTS.md, per entry, not per batch |

**The four outcomes are fixed** — `critical_failure`, `failure`, `success`, `critical_success`.
No fifth. All four keys must be present; empty effect lists are expected and correct.

**Per-entry example (shape only, fictional — do not copy its prose):** see
[`spell-library-seed.md`](spell-library-seed.md) §2. Read that file; it *is* the format.

## 3. What a template must contain — the enforceable list

An entry is complete when it has, and the loader refuses it when it lacks:

1. `name` — the display name the party uses.
2. `duration_note` — non-empty.
3. `citation` — non-empty, book + page for that entry's rules facts.
4. `outcomes` — all four degrees present, each a list of effect rows (empty allowed).
5. Each effect row — `stat` (from the closed list below), `type` (one of the four), `value`
   (signed integer), `targets` (`"chosen"` or `"self"`).
6. `prose` — a non-empty string for each of the four degrees.
7. Optional: `levels` — display-only text.

**The only vocabulary Dave must respect** (everything else is free prose):

```
stats  ac fort ref will perception speed attack damage spell_attack spell_dc class_dc
       all_checks all_dcs all_checks_and_dcs  skill:<name>   (lores: skill:lore:<name>)
types  circumstance  status  item  untyped
targets  "chosen" | "self"        (roster characters only — never companions, never enemies)
```

A stat outside that list **fails the build**, and CI will say exactly which spell, which degree,
which row and what text. Nothing at the table can ever be hurt by a typo, which is the point.

Note one asymmetry, because it bites: the engine accepts a *valid* stat a given character simply
doesn't have (a lore they lack) and drops the modifier at compute time. So validation is on
**spelling**, not plausibility — "is this a real stat" is the loader's job; "does this template
make sense for this spell" is Josh's review.

## 4. The format, and how it gets in

- **File**: `data/seed/spell-library.json`, `spells` array, shape in
  [`spell-library-seed.md`](spell-library-seed.md).
- **Best for Dave**: edit the JSON directly in a PR — it's ~15 lines per spell and the loader
  error messages are precise.
- **Also fine**: Dave hands over templates as a table, an outline, a prose doc, or messages in a
  channel, and a seat transcribes them **mechanically** into the config — transcription only,
  zero authorship — and opens the PR for Josh's review. If that's the chosen route, say so on
  gh#15 and the transcription task gets opened; the transcriber's PR body lists every entry and
  names Dave as the author of its prose.
- **Authorship never blurs either way.** "Never model-generated" binds our seats as tightly as
  it binds a generator: we will not draft templates for Dave to approve, not even as a starting
  point, because the first draft wins in practice and that is how the PRD's rule dies politely.
- **Review**: Josh reviews before merge, as the PRD requires. One PR is fine for the first
  batch; incremental batches (5 spells at a time) are also fine and are easier to review —
  the loader's fail-safe posture means the library can fill in gradually with no broken state in
  between.

## 5. What we will not do with this ask

- Not hold the code half. The code ships against an empty file (SC-6 proves the P0 composer is
  untouched), so a slow answer here costs the party a few weeks of freeform casting, not a
  stalled epic.
- Not seed placeholders. The shipped file has `"spells": []`.
- Not enumerate a guess at the list anywhere — not in this file, not in the spec, not in a PR
  body (spec FR-3 makes that a scope violation).
- Not author templates "as an illustration" in `data/seed/`. The single illustrative entry in
  the repo is fictional, in a `contracts/` doc, and labelled as such.

## 6. Once the content lands — what happens, mechanically

A data-only PR touches `data/seed/spell-library.json` and nothing else. CI runs the load check;
if it's red, the message names the field. On merge and restart, the client shows the new spells
on its next read — **no client rebuild, no migration, no code change** (spec SC-9 is the
criterion that guarantees this). If a content PR ever needs a code change to work, that is a bug
in our half and it comes back to us.
