# Contract: the spell-library seed file (`data/seed/spell-library.json`)

**Epic**: E13 · gh#15 · owner of this document: Korrin (schema and validation). Owner of the
*contents*: Josh (spell list) and Dave (outcome templates, reviewed by Josh).
**Status**: the schema below is what the loader accepts and what the content ask is written
against. A content PR that does not match it is red at the load check, with a message naming
the offending field.

This file is the only place in the repo where a spell name appears as an example, and it is an
**illustration of the shape**, not a candidate entry. See spec FR-3.

---

## 1. The file at this epic's merge time

```json
{
  "_readme": [ "see the keys below; content is owned by Josh and Dave" ],
  "schema": 1,
  "spells": []
}
```

Empty is a valid, tested, shipped configuration (spec FR-1, US-3.3, SC-6). The `_readme`
block carries the entry-shape documentation in the file itself, exactly as
`data/seed/condition-tiers.json` does, so whoever edits it never has to leave the file.

## 2. Entry shape

One entry per spell. All keys required unless marked optional.

```jsonc
// ─────────────────────────────────────────────────────────────────────────────
// SCHEMA ILLUSTRATION ONLY — this is NOT seed data and is NOT a candidate spell.
// It exists to make the shape legible. It is fictional. Do not copy its prose
// into data/seed/spell-library.json; do not treat its name as a list suggestion
// (spec FR-3). Real entries are Dave's, reviewed by Josh.
// ─────────────────────────────────────────────────────────────────────────────
{
  "name": "Example Cantrip",              // display name; the effect's name at the table
  "levels": "cantrip",                    // optional free-text; display only, never evaluated
  "duration_note": "1 round",             // required; rides the effect as its duration note
  "citation": "Player Core 1, <page>",    // required; source for the rules facts in this entry
  "outcomes": {
    "critical_failure": [ /* effect definitions, or [] for "nothing happens" */ ],
    "failure":            [],             // empty list is the common, correct case for a buff
    "success": [
      {
        "stat":  "attack",                // closed vocabulary — see §3
        "type":  "status",                // circumstance | status | item | untyped
        "value": 1,                       // signed integer; penalties negative
        "targets": "chosen"               // §4
      }
    ],
    "critical_success": [
      { "stat": "attack",   "type": "status", "value": 2, "targets": "chosen" },
      { "stat": "fort",     "type": "status", "value": 1, "targets": "chosen" }
    ]
  },
  "prose": {                               // required per outcome; Dave's paraphrase voice
    "critical_failure": "…what happens, in the prototype's paraphrase style…",
    "failure":            "…",
    "success":            "…",
    "critical_success":   "…"
  }
}
// ───────────────────────────────── end of schema illustration ─────────────────
```

**Field notes.**

| field | required | enforced by | why it is there |
|---|---|---|---|
| `name` | yes | loader (non-empty) | becomes the effect name; identity for display only — two effects may share a name (shipped engine rule) |
| `levels` | no | — | display text; the loader never evaluates it, so a wrong value is cosmetic, never a wrong number |
| `duration_note` | yes | loader (non-empty) | the composer's duration note; the PRD's duration is a note, not a timer |
| `citation` | yes | loader (non-empty) | AGENTS.md rules-exactness §1 — a rules fact carries its source per row; here per entry |
| `outcomes` | yes | loader | **the four keys are a fixed set**; each maps to a list of 0..n effect definitions |
| `outcomes.<deg>[].stat` | yes | loader, against `engine::vocab` | fail-loud at config load, never at the table (the AI guardrail) |
| `outcomes.<deg>[].type` | yes | loader, against `engine::vocab` | same |
| `outcomes.<deg>[].value` | yes | loader (signed integer, bounded like the composer's) | penalties negative, as everywhere in the engine |
| `outcomes.<deg>[].targets` | yes | loader, against §4 | the selection rule; roster-only always |
| `prose` | yes | loader (a non-empty string per outcome key) | the paraphrase the caster reads while tapping |

An outcome list of zero effect definitions is legal and means "nothing happens" — the caster's
tap states that plainly (spec FR-7). A *missing* degree key is a defect (defect class D4).

## 3. The vocabulary (not a second list — a pointer)

`stat` accepts exactly the canonical text the engine publishes in `engine/src/vocab.rs`:

- the 11 single stats: `ac`, `fort`, `ref`, `will`, `perception`, `speed`, `attack`, `damage`,
  `spell_attack`, `spell_dc`, `class_dc`
- the 3 blankets: `all_checks`, `all_dcs`, `all_checks_and_dcs`
- `skill:<name>` — core skills bare (`skill:acrobatics`), lores as `skill:lore:<name>`

`type` accepts exactly `circumstance`, `status`, `item`, `untyped`.

**The loader validates against the engine's re-exported constants, not a copy of them**
(spec FR-14). The list above is a rendering of those constants for human readers of this
document; if it and `engine/src/vocab.rs` ever disagree, `engine/src/vocab.rs` is right and this
document is the bug. (The same posture `src/import/seed.rs` already takes: `pub use
hireling_engine::vocab::{…}` — one source, no drift.)

## 4. Target selection rules

A template does not name characters — the caster picks them at the table (spec FR-8). The
`targets` field is a selection *rule*:

| value | meaning | offered how |
|---|---|---|
| `"chosen"` | every target the caster picked in the composer's picker | the ordinary case |
| `"self"` | the caster only | the picker is not consulted for this definition |

Anything else is a defect (class D5). Companions, minions and enemies are never targetable —
that is a shipped engine/PRD rule and **no value in this file can widen it**.

Two rules deliberately absent, and why: `"all_allies"` (an auto-broadcast that targets the
whole roster without asking is a product decision the PRD does not make) and `"enemy:*"`
(non-goal — enemies are not roster entities). If a real spell needs either, that is feedback to
the PRD, not an addition to this table.

## 5. What the loader refuses (the defect classes)

Each class below is individually asserted by its own fixture through the production load path
(spec FR-13, SC-5). Refusal means: the server does not start, and CI on the PR that introduced
the defect is red with the same message.

| class | defect | message shape |
|---|---|---|
| D1 | `stat` text outside §3 | `spell "<name>" outcome "<deg>" row <i>: unknown stat "<text>"` |
| D2 | `type` outside the four | `spell "<name>" outcome "<deg>" row <i>: unknown modifier type "<text>"` |
| D3 | `value` not an integer, or outside the composer's bound | `spell "<name>" outcome "<deg>" row <i>: value out of range` |
| D4 | a degree key missing, or an unknown degree key present | `spell "<name>": outcome "<deg>" missing` / `unknown outcome "<deg>"` |
| D5 | `targets` outside §4 | `spell "<name>" outcome "<deg>" row <i>: unknown target rule "<text>"` |
| D6 | a required field absent or empty (`name`, `duration_note`, `citation`, `prose` entry) | `spell "<name>": <field> required` |
| D7 | duplicate `name` across entries | `duplicate spell name "<name>"` |
| D8 | malformed JSON / wrong top-level shape / unsupported `schema` version | the parser's error, or `unsupported spell-library schema <n>` |
| D9 | an effect list whose definitions all target a stat no roster sheet could carry (e.g. `speed` blanket misuse) | accepted — validation is vocabulary and shape, **not** plausibility. A legal-but-useless template is Josh's review, not a lint |

**Zero-length `outcomes.<deg>` is never a defect.** **An empty `spells` array is never a
defect.** Both are the fail-safe direction, and both are asserted green.

## 6. Invalid stat text — the case the guardrail names

The guardrail's example is "a template referencing an invalid stat fails loud at config load,
not at the table." `"attak"` is not storable as a `Stat` — and note the shipped contrast: the
*engine* drops a modifier naming a stat the sheet has no instance for (a lore the character
lacks), silently, by design. So the two layers fail differently and both are right:

- **library load** → refuses. The template is author-time data; a typo there is a bug, not a
  table state, and must never reach the table.
- **engine at compute time** → drops for want of an instance. A valid stat the sheet doesn't
  have is a real situation at the table.

Keeping those two behaviours distinct is why the loader validates **text against the closed
vocabulary** (author-time correctness) and does not try to validate against any character's
sheet (which would be the wrong question at the wrong time).
