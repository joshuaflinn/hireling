# PRD Quality Report: Hireling PRD (Draft v2, 2026-09-16) — ROUND 2 RE-CHECK

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready gate).
PRD: `docs/PRD.md` (Draft v2 — "checker round 1 findings incorporated; all v1 open
questions resolved"). UX reference re-verified: `docs/reference/lorum_ipsum_dashboard.html`.
The PRD was not modified; this report is findings only. Round 1's report is superseded
by this file; its 20 findings are dispositioned in the table below.

---

## Part 1: Summary Dashboard

### Agent-Ready Gate: ⚠️ CONDITIONAL — one targeted fix from PASS

Every round-1 blocker is resolved: the GM seat is decided and specified, the stat
vocabulary is closed and blanket conditions are expressible, the tooltip corpus has a
source/coverage/license, re-import anchoring is fully defined, and the party/GM/stash
UX flows exist. Two new issues introduced by v2's own resolutions keep it at
CONDITIONAL:

- **Seeded-condition list outruns the stat vocabulary.** FG3's "decided" seed list
  includes slowed/stunned (action-economy losses), drained (max-HP reduction), and
  clumsy (ranged-only attack penalty) — none expressible in the closed `stat`
  vocabulary that the same section claims needs "no special-casing." 4 of the 9
  seeded P0 conditions will force an agent to either invent vocabulary or silently
  ship wrong math.
- **Cross-member sheet view is stated two ways.** FG5 says a player opening another
  member's sheet sees "the same party view … minus ownership write controls"; the UX
  Party Screens section says tapping a roster card "opens that character's full
  sheet, read-only unless you're the owner." Two implementable readings.

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 2
- 🔵 Info: 5

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (IDs P1–P5, C1–C3, G1, with
convention statement), Functional Requirements (P0–P2, six feature groups), User
Experience (entry point, core flow, party screens, edge cases), Narrative, Success
Metrics (+ Tracking Plan), Technical Considerations (Tooling, UI, API/Backend,
Hosting, Performance, Integration Points, Key Risks)

### Sections Missing: None (Open Questions section removed — all items resolved inline)

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by convention —
criteria live in the Functional Requirements; convention stated explicitly and every
story cites its covering FG#)

### Overall Assessment
v2 did its job: all 20 round-1 findings are fixed (18 clean, 2 fixed-with-residual),
and the document is now specific enough that an agent could build the sheet, import,
sync, auth, and provisioning without guessing. The one remaining substantive problem
is self-inflicted by the round-1 fix: the seeded-condition library was enumerated
without checking each condition against the newly-closed stat vocabulary, and four of
the nine seeds don't fit it. Reconcile those two lists and this passes the gate.

---

## Round 1 Disposition Table (all 20 findings)

| # | Sev | Finding (short) | Disposition | Evidence in v2 |
|---|-----|-----------------|-------------|----------------|
| 1 | 🔴 | GM seat scope undecided | **Fixed** | FG5 "GM view (decided: ships in POC)" — contents enumerated (roster, HP bars, down/max, effect chips with sources), read-only end to end; UX Party Screens has the GM flow; Open Questions section gone; G1 story cites FG5 |
| 2 | 🔴 | Modifier `stat` vocabulary undefined; blanket conditions unexpressible | **Fixed** | FG3 closed vocabulary: single stats + `all_checks` / `all_dcs` / `all_checks_and_dcs` blanket targets ("this is how *frightened* −1 works — no special-casing"); `attack` vs `damage` split with *inspire courage* rationale; engine-recomputed vs. static enumerated. ⚠️ Fix introduced a new issue — see Round-2 finding [R1] (seed list doesn't fit its own vocabulary) |
| 3 | 🟡 | Tooltip corpus: author/coverage/license undefined | **Fixed** | FG1: corpus seeded from Dave's prototype (verified present — `CONDITIONS` map with paraphrased text, AoN IDs, page cites at line 473); coverage = every Player Core condition; CUP/ORC notice stated. Residual: "game terms" beyond conditions still unscoped — see [R3] |
| 4 | 🟡 | Sync latency "sane networks" untestable | **Fixed** | FG2 now points at the Technical Metrics targets (p95 < 1s home wifi, < 3s cellular) |
| 5 | 🟡 | Re-import anchoring undefined | **Fixed** | FG1 anchoring rules: HP/temp-HP by character identity, slots and prep by rank+index, inventory by item name; unmatched entities kept + post-import diff; effects server-side. The two preserve-lists are reconciled (UX now defers to "the FG1 anchoring rules") |
| 6 | 🟡 | Ownership mapping undefined | **Fixed** | FG2: claimed at import, one character per account at POC, reassignment = manual admin/DB op |
| 7 | 🟡 | Offline LWW clock semantics undefined | **Fixed** | FG2: server-assigned monotonic version per field, server-receipt order (client clocks untrusted), field granularity enumerated, losing writes silently superseded + "syncing…" indicator |
| 8 | 🟡 | Level-adjust derivation scope undefined | **Fixed** | FG1: math rescale only (proficiency, HP, class DC); boosts/feats/skill increases require Pathbuilder re-export |
| 9 | 🟡 | Companions as effect targets undefined | **Fixed** | FG3: targets are roster characters only; companion buffs tracked manually at POC |
| 10 | 🟡 | Effect-library seeding undecided | **Fixed** | FG3 "Seeded effect library (decided)": nine conditions enumerated, spells freeform, modifier picker defined (stat → type → value). ⚠️ Enumeration introduced [R1] |
| 11 | 🟡 | Pathbuilder import contract undefined | **Fixed** | FG1: contract = the `#pbExport` reference export in the prototype (verified present at line 314); failure classes (a)(b)(c) defined; "version pinned" dropped and explained |
| 12 | 🟡 | Technical metrics without instrument | **Fixed** | `sync_roundtrip_ms` backend logging now specified; uptime via Heimdall named; crash-free explicitly downgraded to qualitative. Minor residual on what the timestamp measures — see [R4] |
| 13 | 🟡 | Shared Inventory traces to no story | **Fixed** | Story P5 added ("dump the night's loot into a shared stash and see who claimed what") citing FG4 |
| 14 | 🟡 | Stories: no criteria, no IDs, no convention | **Fixed** | IDs (P1–P5, C1–C3, G1), convention statement at section head, per-story FG# citations. Minor residual: ID/priority collision — see [R2] |
| 15 | 🟡 | No UX flow for party screen / cross-member / GM / stash | **Fixed** | UX "Party Screens" section added: party-screen composition + tap-through, GM view, stash (add/transfer/history). ⚠️ FG5's wording on cross-member views now conflicts slightly with it — see [R5] |
| 16 | 🔵 | Implementation prescription unmarked | **Fixed** | Technical Considerations opens with "stack choices … are settled house constraints … not negotiable requirements" |
| 17 | 🔵 | Personas names-only; Caster role ambiguity | **Fixed** | Caster explicitly "(a role any player holds mid-session, not a separate seat)"; GM persona now has UX coverage |
| 18 | 🔵 | `asgard/README.md` guardrails invisible to agents | **Fixed** | Guardrail values inlined: database `hireling`, role `hireling`, `CONNECTION LIMIT 20`, Langfuse precedent, sqlx migrate, compose for local dev. The external README reference is gone |
| 19 | 🔵 | House-infra deps: no owner/fallback | **Fixed** | FG2 "Degraded mode": backend down → read-only + queued writes; session-night infra health named a P0 operational dependency owned by Josh |
| 20 | 🔵 | Hostname / empty-state TBDs | **Fixed** | `hireling.flinntech.com` marked "(decided)"; empty party screen + import CTA "adopted as the design, no longer TBD" |

**Disposition totals: 20 fixed (18 clean; 2 fixed with a residual or introduced-issue
that surfaces below as [R1] / [R3] / [R5]). 0 partially fixed. 0 not fixed.**

---

## Part 2: Detailed Findings (Round 2 — new issues from the fresh full check)

### 3. Ambiguous Requirements

**[R1] 🟡 Seeded-condition list outruns the closed stat vocabulary (introduced by the round-1 fix)**
- **Where:** Feature Group 3 — seed list: "frightened, sickened, slowed, stunned,
  enfeebled, clumsy, drained, stupefied, plus off-guard … as ready-made effects on
  the stat vocabulary above"; vocabulary: `ac, fort, ref, will, perception, speed,
  attack, damage, spell_attack, spell_dc, class_dc, skill:<name>, all_checks,
  all_dcs, all_checks_and_dcs`.
- **What:** Four of the nine seeds carry math the vocabulary cannot express:
  *slowed* and *stunned* remove actions per turn (no `actions` target); *drained*
  also reduces max HP by level × value (no `max_hp` target); *clumsy* penalizes
  ranged attack rolls but not melee (vocab has only blanket `attack`). The section
  simultaneously promises "no special-casing."
- **Why it matters:** The seeded library is P0 content built directly against this
  vocabulary. An agent implementing the seeds hits the gap on condition #3 of 9 and
  must either extend the vocabulary itself (inventing product design — exactly what
  round-1 issue [2] was closed to prevent) or ship conditions with silently wrong
  math at the table.
- **Suggested fix:** Pick one and say it: (a) extend the vocabulary (`actions`,
  `max_hp`, `attack:ranged`/`attack:melee`), (b) trim the seed list to the
  expressible conditions (frightened, sickened, enfeebled, stupefied, off-guard)
  and mark slowed/stunned/drained/clumsy as text-only tooltip entries at POC, or
  (c) declare those four "partial — penalty modifiers seeded, action/HP bookkeeping
  manual," matching the companion-buff precedent already set for FG3.

**[R2] 🟡 Cross-member sheet view stated two ways**
- **Where:** FG5 — "The same party view (with normal read permissions) is what a
  player sees when they open another member's sheet — minus ownership write
  controls" vs. UX Party Screens — "Tapping a card opens that character's full
  sheet, read-only unless you're the owner."
- **What:** UX says players get the full sheet read-only; the FG5 sentence reads as
  players getting the GM's reduced party view. Probably meant "the same *view
  system*," but as written the two statements specify different UIs for the same
  action.
- **Why it matters:** Cross-member viewing is the party-linked differentiator's most
  used surface; an agent picking the wrong reading ships a summary card where a full
  read-only sheet was intended.
- **Suggested fix:** Rewrite the FG5 sentence to match the UX ("players opening
  another member's sheet get the full sheet, read-only; the GM account gets the
  party view and never an edit control"), or vice versa if the intent changed.

### 6. Section Completeness / 9. Testability

**[R3] 🔵 "Game terms" tooltip coverage beyond conditions unscoped**
- **Where:** FG1 — "Conditions and game terms render as hoverable/pinnable popups";
  coverage statement only enumerates "every Player Core condition."
- **What:** The corpus seed and coverage line cover conditions; "game terms"
  (traits? action names? spell schools?) have no coverage statement, and the
  prototype's own spell-tooltip fallback ("No rules entry for this spell in the
  dashboard yet") shows spells are deliberately partial.
- **Suggested fix:** One line — "POC tooltip coverage: Player Core conditions only;
  other terms fall back to an AoN search link (the prototype's existing behavior)."

**[R4] 🔵 `sync_roundtrip_ms` measures broadcast, not client receipt**
- **Where:** Success Metrics — "the backend timestamps every state-change broadcast;
  `sync_roundtrip_ms` is logged per event."
- **What:** The targets are phrased as end-to-end propagation ("propagates … to all
  connected party members"), but a server-side broadcast timestamp measures send,
  not arrival/render on the client. On cellular — the case the 3s target exists for
  — the gap between those two is the whole measurement.
- **Suggested fix:** Either log a client-ack round trip (one `ack` frame per
  broadcast per client) or reword the metric to "p95 server broadcast latency."

### 5. Traceability

**[R5] 🔵 Story IDs P1–P5 collide with priority label P1**
- **Where:** User Stories — IDs `P1…P5`; story P5's citation reads "(FG4, P1)" where
  `P1` means the feature group's *priority*, not story P1. The convention statement
  only authorizes FG# citations.
- **What:** Story IDs reuse the P0/P1/P2 priority alphabet, so "(FG4, P1)" is
  ambiguous to both humans and ticket generators, and the stray priority citation
  breaks the stated citation convention.
- **Suggested fix:** Rename story IDs to `S1…S9` (or `US-P1` style) and drop the
  priority from citations, or state that citations may include the group's priority.

### 3. Ambiguity (residual, minor)

**[R6] 🔵 Example modifiers attribute Bless bonuses to Will/Perception (rules-inaccurate examples)**
- **Where:** UX Step 3 — "modifier `+1 status to attack rolls, Perception…`" for
  Bless; Narrative — "`Will +14 = +13 base +1 status (Bless, from Bear)`."
- **What:** *Bless* grants +1 status to attack rolls only; it does not touch
  Perception or Will. These are illustrative format examples, not seeded data
  (spells are freeform at POC), so the engine is unaffected — but in a PRD whose
  entire purpose is killing rules errors, examples read as ground truth.
  (Pre-existing from v1; not flagged in round 1; flagged now under the fresh check.)
- **Suggested fix:** Swap the examples to an accurate pairing (e.g. Will via
  *inspire courage*'s… no — simplest: keep the provenance *format* but use
  `Attack +12 = +11 base +1 status (Bless, from Bear)`).

**[R7] 🔵 Claim-history record shape undefined**
- **Where:** FG4 — "A simple log of who took what, when."
- **What:** Fields beyond actor/item/timestamp are unspecified (quantity? direction
  — stash→character vs. character→stash?). "Simple" is a weasel word but the
  surface is small and P1.
- **Suggested fix:** One line: "log entry = timestamp, actor, item, quantity,
  direction (deposit/claim)."

*(Categories 1, 2, 4, 7, 8, 10: no new findings. Structural completeness is full;
no cross-section contradictions beyond [R5]; the new-in-v2 Tooling & Quality Gate
section (rust-toolkit / grizzly-gate) is proportionate to the zero-new-infrastructure
goal, justified inline, and carried in Integration Points with a named owner (Bear)
and a fallback (vendoring/pinning) — not flagged.)*

---

## Next Steps

1. Back to **prd-builder** in update mode for two paragraphs: [R1] (reconcile the
   seed list with the vocabulary — recommend option (c), it matches precedents the
   PRD already set) and [R2] (one-sentence rewrite in FG5).
2. The five 🔵 items are single-line fixes; batch them into the same pass.
3. Re-check after that pass is expected to flip the gate to ✅ PASS — then
   **prd-decomposer** or straight to engineering; P0 scope is small and the owners
   are the builders.

---
---

# ROUND 3 RE-CHECK — Hireling PRD (Draft v2.1, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready gate),
run fresh against the full document. PRD: `docs/PRD.md` (Draft v2.1 — "checker
round 1 + round 2 findings incorporated"). UX reference re-verified:
`docs/reference/lorum_ipsum_dashboard.html` (`#pbExport` at line 314, `CONDITIONS`
map at line 480 — both anchors intact). The PRD was not modified; rounds 1–2
content above is retained; this section is appended.

## Round 2 Disposition (both warnings + all five info items)

| # | Sev | Finding | Disposition | Evidence in v2.1 |
|---|-----|---------|-------------|------------------|
| R1 | 🟡 | Seed list outran stat vocabulary | **Fixed** | FG3 seeds split into two tiers: **automatic** (frightened, sickened → −X status to `all_checks_and_dcs`; off-guard → −2 circumstance to `ac`) — all three verified expressible in the closed vocabulary — vs. **manual-tracking** (clumsy, enfeebled, stupefied, drained, slowed, stunned) shipping with tooltip + duration note, no engine math, badged "tracked manually," explicitly citing the companion-buff precedent. The "no special-casing" claim now only has to cover the three automatic seeds, and it holds. Residual on blanket expansion — see [R3-1] below |
| R2 | 🟡 | Cross-member view stated two ways | **Fixed** | FG5 now carries one rule, labeled as such: "any account can open any character's full sheet read-only from the party view — ownership gates writes, nothing gates reads." UX Party Screens ("full sheet, read-only unless you're the owner") agrees; FG2's "everyone in the party reads everything" agrees. One reading, three sections |
| R3 | 🔵 | "Game terms" coverage unscoped | **Fixed** | FG1: "POC coverage: every Player Core condition; other game terms are out of POC scope and get added on demand" |
| R4 | 🔵 | `sync_roundtrip_ms` measures broadcast | **Fixed** | Success Metrics: "This measures server-side broadcast latency; client-receipt confirmation is a productization refinement, not POC scope" — metric and instrument now honestly scoped |
| R5 | 🔵 | Story IDs collided with P1 priority | **Fixed** | IDs renamed US-1…US-9 with an explicit "identifiers, not priorities" note; citations are FG#-only (verified all nine) |
| R6 | 🔵 | Bless examples rules-inaccurate | **Fixed** | UX Step 3 is now "`+1 status to attack rolls`"; both provenance examples (FG3, Narrative) read `Strike +14 = +13 base +1 status (Bless, from Bear)` — attack rolls only |
| R7 | 🔵 | Claim-history record shape | **Fixed** | FG4: append-only log of `{ item, quantity, from, to, actor, timestamp }` |

**Disposition totals: 7 of 7 fixed (2 warnings, 5 info). 0 partially fixed. 0 not fixed.**

## Part 1: Summary Dashboard (Round 3)

### Agent-Ready Gate: ⚠️ CONDITIONAL — one one-line fix from PASS

Every round-2 finding is resolved and the fresh full check found no new critical
issues. One gap sits exactly on the engine's flagship seed:

- **Blanket-target expansion set is undefined.** FG3 says blanket targets "expand
  to every covered stat" but never says which covered stats each blanket covers.
  Read literally ("every single stat"), *frightened* would wrongly penalize
  `damage` (not a check) and `speed` (not a check), and — depending on the
  reader — either wrongly include or wrongly exclude `ac` from `all_dcs` (AC *is*
  a DC in PF2e, so frightened does reduce it). The three automatic seeds' math is
  only correct under one specific mapping, and the PRD doesn't state it.

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 1
- 🔵 Info: 2

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (US-1–US-9, with convention
statement), Functional Requirements (P0–P2, six feature groups), User Experience
(entry point, core flow, party screens, edge cases), Narrative, Success Metrics
(+ Tracking Plan), Technical Considerations (Tooling, UI, API/Backend, Hosting,
Performance, Integration Points, Key Risks)

### Sections Missing: None

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by convention —
criteria live in the Functional Requirements; convention stated explicitly and
every story cites its covering FG#; all FG# citations verified)

### Overall Assessment
v2.1 closed out everything round 2 raised — the two-tier seed split is the right
call and it lands cleanly. One residual the split exposed rather than created:
the blanket-target expansion rule needs its actual stat mapping written down,
because the document's own stop-the-line-on-engine-bugs stance makes a guessed
mapping the most expensive possible place to be wrong. One sentence fixes it;
then this passes.

---

## Part 2: Detailed Findings (Round 3 — fresh full check)

### 3. Ambiguous Requirements / 9. Testability

**[R3-1] 🟡 Blanket-target expansion mapping undefined (exposed by the R1 fix)**
- **Where:** FG3 — "Blanket targets expand to every covered stat before stacking
  is evaluated"; the closed vocabulary lists single stats `ac, fort, ref, will,
  perception, speed, attack, damage, spell_attack, spell_dc, class_dc,
  skill:<name>` plus blankets `all_checks, all_dcs, all_checks_and_dcs`.
- **What:** "Every covered stat" is the entire single-stat list if read plainly —
  but PF2e checks and DCs are a *subset*. `damage` rolls are not checks; `speed`
  is not a check; and `ac` *is* a DC (so *frightened*'s `all_checks_and_dcs`
  correctly reduces AC, while a lay reading might exclude it since `ac` sits in
  the list as a defense). The literal expansion makes the three automatic seeds
  rules-wrong; the correct expansion is recoverable only by an agent that
  independently knows PF2e's check/DC taxonomy — exactly the tribal-knowledge
  reliance the agent-ready gate exists to catch.
- **Why it matters:** The engine is declared "Constitution Article IV —
  stop-the-line on bugs," and the automatic seeds are the P0 content built
  directly on this rule. An agent guessing the mapping ships frightened that
  either taxes damage rolls or leaves AC untouched — visible, at-the-table wrong
  math on the product's flagship demo case.
- **Suggested fix:** One line in FG3, e.g.: "Expansion: `all_checks` →
  `attack, spell_attack, fort, ref, will, perception, skill:*`; `all_dcs` →
  `ac, spell_dc, class_dc`; `all_checks_and_dcs` → the union. `damage` and
  `speed` are never covered by blankets." (PM to confirm the AC-is-a-DC
  inclusion; that is the rules-accurate reading.)

### 3. Ambiguity (minor)

**[R3-2] 🔵 UX Step 3 implies a seeded, pickable Bless**
- **Where:** UX Core Experience Step 3 — "Bear taps 'new effect' → picks Bless
  (or freeforms it)"; FG3 — "Spells and other sources are freeform at POC."
- **What:** The seed library is conditions-only (two tiers); *Bless* is a spell
  and is not seeded in either tier, so there is nothing to "pick." The
  parenthetical half-saves it, but the step still reads as if a Bless entry
  exists in a picker.
- **Suggested fix:** "→ freeforms Bless →" or "→ names it Bless →". One word.

### 6. Section Completeness (minor)

**[R3-3] 🔵 Roster-coverage metric parses as the GM importing a sheet**
- **Where:** Success Metrics — "Roster coverage: 5/5 players + GM have accounts
  and imported sheets."
- **What:** The GM account reads everything and writes nothing, owns no
  character, and (per FG2, one character per account) imports no sheet. The
  metric's grammar distributes "imported sheets" over all six accounts.
- **Suggested fix:** "5/5 players have imported sheets and 6/6 accounts (incl.
  GM) are provisioned and have logged in."

*(Categories 1, 2, 4, 5, 7, 8, 10: no findings. Structural completeness is full;
no cross-section contradictions — the round-2 cross-member conflict is gone and
FG2/FG5/UX now state one read rule; traceability verified story→FG for all nine
stories and goal→metric; stack prescriptions remain marked as settled house
constraints; personas flow through UX; external deps (Pathbuilder, Authentik,
Grizzly-Endeavors) carry owners and fallbacks.)*

---

## Round 3 Next Steps

1. Back to **prd-builder** in update mode for [R3-1] — one line in FG3 stating
   the blanket expansion sets (the only gate blocker). [R3-2] and [R3-3] are
   one-line edits; batch them in.
2. Re-check after that pass is expected to flip the gate to ✅ PASS — then
   **prd-decomposer** or straight to engineering; P0 scope is small and the
   owners are the builders.

---
---

# ROUND 4 RE-CHECK — Hireling PRD (Draft v2.2, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready
gate), run fresh against the full document. PRD: `docs/PRD.md` (Draft v2.2 —
"checker rounds 1–3 findings incorporated"). UX reference re-verified:
`docs/reference/lorum_ipsum_dashboard.html` (`CONDITIONS` map intact at line
480 — 28 condition entries). The PRD was not modified; rounds 1–3 content
above is retained; this section is appended.

## Round 3 Disposition (one warning + two info items)

| # | Sev | Finding | Disposition | Evidence in v2.2 |
|---|-----|---------|-------------|------------------|
| R3-1 | 🟡 | Blanket-target expansion mapping undefined | **Fixed** | FG3 now carries the decided, rules-exact expansion sets: `all_checks` = `attack, spell_attack, fort, ref, will, perception`, every `skill:<name>` — explicitly NOT `damage`, NOT `speed`; `all_dcs` = `ac, class_dc, spell_dc` (AC-is-a-DC confirmed, matching round 3's recommended reading); `all_checks_and_dcs` = the union. Internal consistency verified: the sets **exhaustively partition the closed vocabulary** — every single stat is classified exactly once (7 checks, 3 DCs, 2 neither), so no stat's blanket membership is left to inference. Automatic seeds verified against the mapping: frightened/sickened −X status to `all_checks_and_dcs` hits every d20 roll + all three DCs and nothing else; off-guard −2 circumstance to `ac` untouched by the blankets. Cross-checked against the prototype corpus: frightened's tooltip ("all checks and DCs") and off-guard's ("−2 circumstance penalty to AC") agree with the engine math. The "no special-casing" claim now fully holds. One residual wording nit — see [R4-2] |
| R3-2 | 🔵 | UX Step 3 implied a seeded, pickable Bless | **Fixed** | UX Step 3 now reads "names it Bless (spells are freeform at POC; only conditions are seeded)" — the suggested fix verbatim, and consistent with FG3's "Spells and other sources are freeform at POC" |
| R3-3 | 🔵 | Roster metric parsed as the GM importing a sheet | **Fixed** | Success Metrics now reads "5/5 players have imported sheets; all six accounts active (the GM needs no sheet)" — grammar no longer distributes sheets over the GM account; consistent with FG2 (one character per account; GM writes nothing) and US-9 |

**Disposition totals: 3 of 3 fixed (1 warning, 2 info). 0 partially fixed. 0 not fixed.**

## Part 1: Summary Dashboard (Round 4)

### Agent-Ready Gate: ⚠️ CONDITIONAL — one provenance line from PASS

Every round-3 finding is resolved and verified, and the expansion-set fix is
cleaner than asked for (it partitions the whole vocabulary, so nothing is left
to tribal knowledge). The fresh full check surfaced one gap the earlier rounds
missed — it sits, again, on the seed library:

- **Four seeded conditions have no tooltip text in the declared corpus.** FG1
  declares Dave's prototype `CONDITIONS` map the starting corpus ("not a scrape
  and not model-generated"), but the map has no entries for **sickened** (an
  *automatic* P0 engine seed), **enfeebled**, **drained**, or **slowed** (three
  of the six manual-tracking seeds that "ship in the picker with their rules
  tooltip"). An agent building the seed picker cannot satisfy both "seed ships
  with rules tooltip" (FG3/US-3) and "content is not model-generated" (FG1)
  without asking where that text comes from.

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 1
- 🔵 Info: 1

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (US-1–US-9, with convention
statement), Functional Requirements (P0–P2, six feature groups), User Experience
(entry point, core flow, party screens, edge cases), Narrative, Success Metrics
(+ Tracking Plan), Technical Considerations (Tooling, UI, API/Backend, Hosting,
Performance, Integration Points, Key Risks)

### Sections Missing: None

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by convention —
criteria live in the Functional Requirements; convention stated explicitly and
every story cites its covering FG#; all FG# citations re-verified)

### Overall Assessment
v2.2 closes everything round 3 raised, and the blanket expansion is now the
strongest-specified rule in the document. What remains is a corpus-provenance
gap on the same seed library — pre-existing (present since the seeds were named
in v2.1, missed by rounds 2–3), one sentence to fix, and the last thing between
this PRD and a clean PASS.

---

## Part 2: Detailed Findings (Round 4 — fresh full check)

### 3. Ambiguous Requirements / 6. Section Completeness / 9. Testability

**[R4-1] 🟡 Seed tooltip corpus gap: 4 of 9 seeded conditions absent from the declared content source**
- **Where:** FG3 seeded effect library (automatic: frightened, **sickened**,
  off-guard; manual-tracking: clumsy, **enfeebled**, stupefied, **drained**,
  **slowed**, stunned — bold = missing) vs. FG1 rules tooltips ("The content
  corpus is seeded from Dave's prototype — its condition and rules text … is
  the starting corpus, not a scrape and not model-generated") and
  `docs/reference/lorum_ipsum_dashboard.html` line 480 (`CONDITIONS` map: 28
  entries; sickened/enfeebled/drained/slowed appear only inside monster
  ability text, never as tooltip entries; "drained" appears nowhere at all).
- **What:** The PRD promises POC coverage of "every Player Core condition"
  (FG1) and that the manual-tracking seeds "ship in the picker with their
  rules tooltip" (FG3); US-3 requires every condition to explain itself on
  hover. But the one sanctioned content source lacks the text for four of the
  nine seeds — including sickened, one of only three automatic engine seeds.
  The PRD never says how corpus gaps get filled.
- **Why it matters:** An agent building the seed picker hits this on day one
  of FG3 work. Its options are to author the tooltip text (violating the
  "not model-generated" provenance rule, on Player Core rules text, under a
  CUP/ORC notice) or to ship picker entries without tooltips (violating FG3
  and US-3 on a P0 seed). Both failure modes land on the product's flagship
  feature, and choosing between them is a PM call, not an agent call.
- **Suggested fix:** One sentence in FG1 (or the FG3 seed library), e.g.:
  "Conditions missing from the prototype corpus (sickened, enfeebled, drained,
  slowed, …) are hand-authored in the same paraphrased, AoN-linked, page-cited
  style and reviewed against Player Core before build." Name the owner.

### 2. Conflicting Information (residual wording, non-blocking)

**[R4-2] 🔵 Stacking sentence still says "every covered stat"**
- **Where:** FG3 stacking math — "Blanket targets expand to every covered stat
  before stacking is evaluated."
- **What:** This was the sentence R3-1 flagged, and the fix landed next to it
  without touching it. With the expansion bullet now present, "covered" is
  effectively defined (per-blanket, rules-exact), and the specific rule wins
  over the general one — but a hyper-literal read of "every covered stat" as
  "all 12 single stats" is still textually available.
- **Suggested fix:** "Blanket targets expand to their defined expansion sets
  before stacking is evaluated." Three words; removes the last trace of R3-1.

*(Categories 1, 4, 5, 7, 8, 10: no findings. Structural completeness is full;
no cross-section contradictions — the expansion bullet, stacking rules, seed
tiers, UX Step 3, Narrative, and both provenance examples now all state the
same attack-roll-only Bless and the same frightened footprint; traceability
verified story→FG for all nine stories and goal→metric; stack prescriptions
remain marked as settled house constraints; personas flow through UX and the
fixed roster metric; external deps (Pathbuilder, Authentik, Cloudflare,
Grizzly-Endeavors) carry owners and fallbacks.)*

---

## Round 4 Next Steps

1. Back to **prd-builder** in update mode for [R4-1] — one sentence naming the
   provenance (and owner) for seed tooltip text the prototype corpus lacks.
   [R4-2] is a three-word edit; batch it in.
2. That pass is expected to flip the gate to ✅ PASS — then **prd-decomposer**
   or straight to engineering; P0 scope is small and the owners are the
   builders.

---
---

# ROUND 5 RE-CHECK — Hireling PRD (Draft v2.3, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready
gate), run fresh against the full document. PRD: `docs/PRD.md` (Draft v2.3 —
"checker rounds 1–4 findings incorporated"). Prototype corpus re-verified
directly: `docs/reference/lorum_ipsum_dashboard.html` `CONDITIONS` map at line
480 — 28 key entries counted; zero key occurrences of `sickened`, `enfeebled`,
`drained`, or `slowed` confirmed by grep. The PRD was not modified; rounds 1–4
content above is retained; this section is appended.

## Round 4 Disposition (one warning + one info item)

| # | Sev | Finding | Disposition | Evidence in v2.3 |
|---|-----|---------|-------------|------------------|
| R4-1 | 🟡 | Seed tooltip corpus gap: 4 seeded conditions absent from the declared source | **Fixed** | FG1 now carries the provenance sentence: the prototype's "28-condition map is missing four POC conditions (sickened, enfeebled, drained, slowed): those entries are **hand-authored in the same style** (paraphrase + AoN link + page cite) and human-reviewed before they ship — never model-generated straight into the product." Cross-section consistency verified end to end: the four named conditions are exactly the corpus-less members of the FG3 seed tiers (automatic: sickened; manual: enfeebled, drained, slowed — while frightened, off-guard, clumsy, stupefied, stunned all have prototype entries), so FG3's "ship in the picker with their rules tooltip" now has a legal text source for all nine seeds, and US-3's hover-explanation is covered. The "not a scrape and not model-generated" corpus claim and the hand-authored carve-out no longer collide — the carve-out is explicit. One residual nit — see [R5-1] |
| R4-2 | 🔵 | Stacking sentence said "every covered stat" | **Fixed** | FG3 now reads "Blanket targets expand to their defined expansion sets before stacking is evaluated" — the suggested three-word fix verbatim; grep confirms no "covered stat" wording survives anywhere in the document. R3-1's last trace is gone |

**Disposition totals: 2 of 2 fixed (1 warning, 1 info). 0 partially fixed. 0 not fixed.**

## Part 1: Summary Dashboard (Round 5)

### Agent-Ready Gate: ✅ PASS — Agent-ready

All three gate tests pass:

- **Specificity:** the closed stat vocabulary, rules-exact blanket expansion
  sets, stacking rules, seed tiers with engine math or explicit "tracked
  manually" badges, import failure classes (a)(b)(c), re-import anchors,
  per-field server-receipt-order reconciliation, and quantified sync targets
  (p95 < 1s wifi / < 3s cellular) leave nothing to tribal knowledge. The seed
  tooltip corpus now has a declared provenance for 100% of seeded conditions
  (28 prototype-sourced + 4 hand-authored-and-reviewed).
- **Completeness:** every user-facing surface has a UX flow with entry point,
  happy path, and error behavior (import failure messages, offline queueing,
  WebSocket reconnect, re-import diff); edge cases are enumerated in Advanced
  Features & Edge Cases; stories are individually addressable (US-1…US-9) and
  the acceptance-criteria convention is stated explicitly with per-story FG#
  citations.
- **Unambiguity:** zero 🔴 Critical issues; zero 🟡 Warnings; priorities
  (P0/P1/P2) are consistent across sections.

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 0
- 🔵 Info: 1

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (US-1–US-9, with
convention statement), Functional Requirements (P0–P2, six feature groups),
User Experience (entry point, core flow, party screens, edge cases),
Narrative, Success Metrics (+ Tracking Plan), Technical Considerations
(Tooling, UI, API/Backend, Hosting, Performance, Integration Points, Key
Risks)

### Sections Missing: None

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by
convention — criteria live in the Functional Requirements; convention stated
explicitly and every story cites its covering FG#; all nine citations
re-verified against the requirement groups)

### Overall Assessment
v2.3 closes the last gate blocker: the seed tooltip corpus now has a declared,
license-clean provenance for every seeded condition, and the stacking rule's
final vague wording is gone. The fresh full check found no new criticals and
no new warnings — the engine, sync, import, and access surfaces are each
specified to the point where an agent with zero PF2e background can build
against them. One info-level nit remains; it does not block anything.

---

## Part 2: Detailed Findings (Round 5 — fresh full check)

### 6. Section Completeness (residual nit, non-blocking)

**[R5-1] 🔵 Hand-authored tooltip entries have no named author/reviewer**
- **Where:** FG1 rules tooltips — the new provenance sentence says the four
  corpus-missing entries are "hand-authored in the same style … and
  human-reviewed before they ship," but does not say *who* authors or reviews
  them. Round 4's suggested fix ended with "Name the owner"; the landed
  sentence doesn't.
- **What:** For the agent-ready gate this is fully resolved — the agent now
  knows the text arrives human-supplied and is never to generate it. The
  residue is a PM-side task-ownership gap only: with three owners named at the
  document head (Josh, Dave, Vex), "human-reviewed" doesn't say which human
  signs off on Player Core rules text shipping under the CUP/ORC notice.
- **Why it matters:** Minor. Nobody builds the wrong thing; but when the four
  entries get written, "someone reviews them" has no assignee, and the
  document's own convention elsewhere (FG2 names Josh for infra health;
  Integration Points names Bear for Grizzly-Endeavors) is to name the owner.
- **Suggested fix:** Two words — e.g. "hand-authored by Dave in the same style
  … and human-reviewed (Josh) before they ship." Any named pair works.

*(Categories 1, 2, 3, 4, 5, 7, 8, 9, 10: no findings. Structural completeness
is full. Cross-section consistency re-verified fresh: FG1's corpus claims
(28 + 4 hand-authored = every Player Core condition for POC tooltip coverage)
match the prototype's actual 28-entry map and FG3's nine seeds exactly; the
automatic seeds' math (frightened/sickened −X status to `all_checks_and_dcs`,
off-guard −2 circumstance to `ac`) still agrees with both the expansion sets
and the prototype's tooltip text; UX Step 3's "spells are freeform at POC;
only conditions are seeded" agrees with the two-tier library; Narrative and
both provenance examples state the same attack-roll-only Bless. No
contradictions between Goals/Non-Goals/Requirements/Metrics; traceability
verified story→FG for all nine stories and goal→metric; stack prescriptions
remain marked as settled house constraints; personas flow through UX; external
deps (Pathbuilder, Authentik, Cloudflare, Grizzly-Endeavors, Asgard) carry
owners and fallbacks.)*

---

## Round 5 Next Steps

1. **Gate is ✅ PASS.** The PRD is agent-ready. Optionally batch [R5-1] (two
   words) into the next PRD touch — it is not worth a dedicated edit pass.
2. Next step in the pipeline: **prd-decomposer** to break into sequenced epics
   with SpecKit prompts, or hand the PRD (plus Dave's prototype as the UX
   reference) directly to engineering — P0 scope is small and the owners are
   the builders.

---
---

# ROUND 6 RE-CHECK — Hireling PRD (v3.0, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready
gate), run fresh against the full document. PRD: `docs/PRD.md` (v3.0 —
"AGENT-READY v2.4 + Dave's review round incorporated": layouts, GM stat
density, import special-cases, PB write-back ruling, seeded spell library,
stash sell/bank, conflict pre-warn). The PRD was not modified; rounds 1–5
content above is retained; this section is appended.

## Dave's 7 Changes — Landing Verification

| # | Change | Landed? | Coherence notes |
|---|--------|---------|-----------------|
| 1 | Two purposeful layouts, both P0, desktop first | **Yes** | UX Step 1 carries the new wording verbatim ("Two purposeful layouts, both P0, desktop built first… not a responsive collapse"); the old "collapsed to a single column" language is gone (grep-verified). FG1's live sheet UI still points at Dave's prototype as the visual/interaction baseline — consistent, since the desktop layout *is* that prototype. Entry point needs no layout mention. ⚠️ Residual: the phone/tablet layout is P0 with no design artifact — see [R6-4] |
| 2 | GM stat density (P1) + initiative modifier vs. order ruling | **Yes, clean** | FG5 gains the P1 stat-block extension (current/max HP, AC, saves, Perception, spell/class DCs, initiative *modifier*, key skills); Non-Goals now carries the explicit "displaying an initiative *modifier* is a stat, not tracking" carve-out, and FG5 repeats "Initiative *order* and turn tracking remain non-goals." Both sections state the same rule; no contradiction. Read-only end-to-end is preserved ("all read-only"). One small undefined term — see [R6-7] |
| 3 | Import: per-feature class-feature handling | **Yes, clean** | FG1: "handled per-feature as real imports surface them; every quirk lands as an importer test case — the reference export covers exactly one build, not the feature space." Coherent with the failure-class model (unknown fields log-and-continue) and honest about the single-build contract |
| 4 | Pathbuilder write-back non-goal | **Yes, clean** | Non-Goals entry is fully reasoned (one-way pipe, unofficial schema, corruption risk, re-import anchoring already preserves session state). Cross-checked: nothing in FG1–FG6 writes back to PB; the re-import anchoring requirement it cites is intact. No Non-Goal→Requirement violation |
| 5 | Seeded spell library (P1) with scope fences | **Yes, with a cross-section staleness** | FG3 lands the full mechanic (cast → degree-of-success pick → effects auto-apply) and both fences (party-targeted only; enemies aren't roster entities = combat tracking; non-library spells stay freeform "exactly as P0"). FG3 internally reconciles freeform-vs-library via that "exactly as P0" layering. ⚠️ But UX Step 3 and FG3's seed bullet still say "spells are freeform at POC / only conditions are seeded" without the P0 qualifier — and Bless, the UX walkthrough's example, is now a named library seed. See [R6-1]. Traceability: verified — US-6 (apply effect to chosen members, app does the math) covers the library flow; no new story needed. Template content provenance is undeclared — see [R6-3] |
| 6 | FG4 sell / party bank / quartermaster | **Yes, with a UX gap** | FG4 gains all three, well-specified (sell = quartermaster-only, book-value default editable, proceeds to bank; bank = gp/sp/cp ledger from sales + manual adjustments, visible to all; quartermaster = one character, owner-set toggle, claims stay open to all). ⚠️ UX Party Screens still describes the stash as "one list, three actions" — sell, the bank view, and quartermaster designation have no UX home. See [R6-2]. Story trace is nominally intact via US-5's FG4 citation but the story text predates selling — see [R6-5]. Metrics: no new metric needed — POC metrics are deliberately table-observed and the adoption/question-count metrics already cover FG4 |
| 7 | FG3 conflict pre-warning (P1) | **Yes, clean in FG3** | Target picker flags same-type suppression at pick time with a worked example ("Becky: +1 status active — Bless would be suppressed"); consistent with the stacking rules and the provenance section's suppressed-source display. Traces to US-6/US-8 via FG3. Minor: not reflected in the UX composer's UI-elements list — see [R6-6] |

## Round 5 Disposition (the one info item)

| # | Sev | Finding | Disposition | Evidence in v3.0 |
|---|-----|---------|-------------|------------------|
| R5-1 | 🔵 | Hand-authored tooltip entries had no named author/reviewer | **Fixed** | FG1 now reads "hand-authored **by Dave** in the same style (paraphrase + AoN link + page cite) and **reviewed by Josh** before they ship" — the suggested fix, with the named pair |

**Disposition totals: 1 of 1 fixed. Round-5 baseline regression sweep:** blanket
expansion sets intact and still partition the vocabulary; "defined expansion
sets" stacking wording intact; two-tier seed split intact; both provenance
examples still `Strike +14 = +13 base +1 status (Bless, from Bear)`; story IDs
US-1…US-9 with FG#-only citations intact; roster-coverage metric grammar
intact; `sync_roundtrip_ms` scoping intact. **No regressions against the
round-5 PASS baseline.**

## Part 1: Summary Dashboard (Round 6)

### Agent-Ready Gate: ⚠️ CONDITIONAL — four targeted fixes from PASS

All seven of Dave's changes landed, the round-5 baseline is regression-free,
and the spell-library scope fences are internally coherent within FG3. The
blockers are all *cross-section propagation* gaps — v3.0 updated the
requirement groups but not the UX section that an agent builds screens from:

- **UX Step 3 still declares "spells are freeform at POC; only conditions are
  seeded"** while FG3's P1 library names Bless (the step's own example) as a
  seed ([R6-1]).
- **The stash UX flow predates sell/bank/quartermaster** — "one list, three
  actions" vs. FG4's four-plus capabilities, with no UX home for the bank
  ledger or the quartermaster toggle ([R6-2]).
- **Spell outcome templates have no declared content provenance** — the same
  class of gap that was the sole round-4 blocker, now on ~20 spells × 4
  degrees of structured rules-derived content ([R6-3]).
- **The phone/tablet layout is P0 with no design reference** — desktop has
  Dave's prototype; the phone layout has one sentence of intent ([R6-4]).

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 4
- 🔵 Info: 3

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (US-1–US-9, with
convention statement), Functional Requirements (P0–P2, six feature groups),
User Experience (entry point, core flow, party screens, edge cases),
Narrative, Success Metrics (+ Tracking Plan), Technical Considerations
(Tooling, UI, API/Backend, Hosting, Performance, Integration Points, Key
Risks)

### Sections Missing: None

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by
convention — criteria live in the Functional Requirements; convention stated
explicitly and every story cites its covering FG#; all nine citations
re-verified; the new P1 capabilities trace through existing citations — spell
library and conflict pre-warning via US-6/US-8 → FG3, sell/bank via
US-5 → FG4)

### Overall Assessment
v3.0 is a good review round landed slightly shallow: every change is coherent
inside its home section, the write-back and initiative rulings are stated in
both places they matter, and nothing regressed — but the UX section wasn't
updated for three of the seven changes, and the spell library inherited the
exact content-provenance gap that blocked round 4. Four short fixes (two
sentences in UX, one provenance line, one design-source line) and this
returns to PASS.

---

## Part 2: Detailed Findings (Round 6 — fresh full check)

### 2. Conflicting Information

**[R6-1] 🟡 "Spells are freeform at POC; only conditions are seeded" is stale against the P1 spell library**
- **Where:** UX Core Experience Step 3 — "names it Bless (spells are freeform
  at POC; only conditions are seeded)"; FG3 seeded effect library — "Spells
  and other sources are freeform at POC" vs. FG3 seeded spell library (P1) —
  "seed the top ~20 the table actually uses — Bless, Fear, Guidance, Heal…".
- **What:** P1 is in POC scope (FG4 stash is P1 and ships in the POC; GM stat
  density is P1), so "at POC" now includes the spell library — and the UX
  walkthrough's own example spell, Bless, is a named library seed. FG3
  internally reconciles the layering ("Spells outside the library stay
  freeform… exactly as P0"), but the UX step and the seed bullet assert the
  pre-library world without the priority qualifier. Round 3 flagged the milder
  inverse of this as [R3-2] 🔵; v3.0 made it materially more wrong.
- **Why it matters:** An agent building the composer from the UX flow ships a
  freeform-only Bless; an agent building from FG3 P1 ships a seeded Bless with
  a degree-of-success picker. The correct layering is recoverable from FG3,
  but the flagship walkthrough now misstates POC scope on the product's
  flagship feature.
- **Suggested fix:** Qualify both spots — UX Step 3: "(spells freeform at P0;
  the P1 spell library seeds Bless — see FG3)"; FG3 seed bullet: "freeform
  outside the P1 spell library." One clause each.

### 5. Traceability (Requirements → UX)

**[R6-2] 🟡 UX stash flow predates sell / party bank / quartermaster**
- **Where:** UX Party Screens — "**Stash (P1):** one list, three actions — add
  item, transfer to/from a character (pick from roster), and a claim-history
  log view" vs. FG4 — "**Sell:** Quartermaster-only action…", "**Party
  bank:** Shared currency ledger (gp/sp/cp)… visible to all party members",
  "**Quartermaster:** One character is designated quartermaster (owner-set,
  admin-style toggle)".
- **What:** FG4 now has four-plus capabilities; the UX flow still enumerates
  exactly three actions. Sell has no entry point in any described screen; the
  bank ledger is "visible to all" but has no named home; the quartermaster
  toggle's location is unspecified; and two edge cases are silent — what
  happens with no quartermaster designated (sell disabled? blocked at
  designation?), and whether a sale writes a claim-history entry (the log's
  shape is `{item, quantity, from, to, actor, timestamp}` — a sale's "to" is
  the bank, which is not a character).
- **Why it matters:** The stash screen is built straight from this paragraph;
  as written the agent builds the v2.4 stash and the three new FG4 bullets
  have no screen to land on.
- **Suggested fix:** Extend the stash paragraph: add sell (visible/enabled
  only to the quartermaster's owner; disabled with a reason when no
  quartermaster is designated), the bank ledger view (running balance +
  adjustment entries), where the quartermaster toggle lives, and one line
  stating sales append to the claim-history log with `to: party bank`.

### 3. Ambiguous Requirements / 6. Section Completeness

**[R6-3] 🟡 Spell outcome templates have no declared content provenance (R4-1-class gap)**
- **Where:** FG3 seeded spell library — spells "carry structured outcome
  templates: cast → the composer offers the spell's degrees of success… → the
  defined effects apply to the chosen targets automatically." No statement of
  who defines those per-degree effect sets or how they're reviewed.
- **What:** Round 4's sole gate blocker ([R4-1]) was exactly this shape:
  rules-derived content with an undeclared source, sitting on the engine.
  v3.0 re-introduces the pattern at larger scale — ~20 spells × up to 4
  degrees of structured effect content, where a wrong template is wrong math
  on the stop-the-line engine. FG1's provenance rule ("hand-authored by Dave,
  reviewed by Josh, never model-generated") covers tooltip *text* only; the
  templates are mechanical content and fall outside it.
- **Why it matters:** An agent's options are to derive the templates from
  Player Core/AoN itself (model-generated rules content — the thing FG1 bans
  for tooltips, on a product whose purpose is killing rules errors) or to
  block and ask. Choosing is a PM call, not an agent call.
- **Suggested fix:** One sentence in the library bullet, mirroring the FG1
  carve-out: "Outcome templates are hand-authored by Dave from Player Core
  and reviewed by Josh before they ship — never model-generated." (Any named
  pair works.)

**[R6-4] 🟡 Phone/tablet layout declared P0 with no design reference or described flow**
- **Where:** UX Step 1 — "Two purposeful layouts, both P0, desktop built
  first… The phone/tablet layout is a dedicated at-table design for one-handed
  use, not a responsive collapse of the desktop." User Goals — "Use it
  one-handed on a phone, mid-combat."
- **What:** The desktop layout has a buildable reference (Dave's prototype,
  cited in FG1 and UX). The phone layout — same priority — has one sentence of
  intent and no design artifact, no described flow differences, no named
  author. The pre-v3.0 wording ("collapsed to a single column") was at least
  an implementable rule; the new wording raises scope (a second purposeful
  design) while removing the only implementable guidance.
- **Why it matters:** An agent asked to build P0 scope must invent the phone
  UX — product design by agent, the failure mode this document has spent five
  rounds eliminating. "Desktop built first" defers but does not resolve it.
- **Suggested fix:** One line naming the source and gate, e.g.: "The phone
  layout is designed by Dave (prototype artifact to land in `docs/reference/`
  like the desktop baseline) before phone build starts; agents do not derive
  it from the desktop layout." Or explicitly mark it "design pending."

### 5. Traceability (Stories → Requirements)

**[R6-5] 🔵 US-5's text predates selling and the party bank**
- **Where:** User Stories — US-5 "dump the night's loot into a shared stash
  and see who claimed what, so the party loot list stops living in a group
  chat" (FG4) vs. FG4's new sell/bank/quartermaster capabilities.
- **What:** Traceability is nominally intact via the FG# citation convention,
  and round precedent (finding 13, round 1) accepted story-per-group
  coverage. But the story's action and benefit describe deposit-and-claim
  only; quartermaster-gated selling and treasury management are a distinct
  table need the story text never mentions.
- **Suggested fix:** Broaden US-5's wording ("…sell loot into a shared party
  bank, so the party loot list and treasury stop living in a group chat") —
  one clause, no new story needed.

**[R6-6] 🔵 FG3's P1 composer additions absent from the UX composer's UI-elements list**
- **Where:** UX Step 3 UI Elements — "effect composer (name, modifier picker
  from the FG3 stat vocabulary, duration note, target picker from party
  roster)" vs. FG3's new conflict pre-warning (target-picker conflict flags)
  and spell library (degree-of-success picker replacing manual modifier
  definition for library spells).
- **What:** Both P1 additions change the composer's interaction surface; the
  UX description lists neither. Lower-stakes than [R6-1]/[R6-2] because FG3
  fully specifies the behavior — this is a screen-inventory gap, not a
  behavioral contradiction.
- **Suggested fix:** Extend the parenthetical: "…target picker from party
  roster (with same-type conflict flags, P1); for library spells (P1), a
  degree-of-success picker replaces the modifier picker."

### 3. Ambiguity (minor)

**[R6-7] 🔵 GM stat density "key skills" subset undefined**
- **Where:** FG5 — "current/max HP, AC, saves, Perception, spell/class DCs,
  initiative *modifier*, key skill modifiers."
- **What:** Every other element of the GM card is a defined stat; "key skills"
  is an undefined subset of the ~17 PF2e skills.
- **Suggested fix:** Define it in a phrase — e.g. "key skills = the
  character's trained skills" or "the top 4 skills by modifier."

*(Categories 1, 4, 7, 8, 9, 10: no new findings. Structural completeness is
full. Stack prescriptions remain marked as settled house constraints. No scope
creep: the spell library and conflict pre-warning serve the core buff-math
goal directly; sell/bank serves US-5's loot-list goal; GM stat density serves
US-9 — every addition traces to a stated goal, and the initiative-order
re-ban keeps the combat-tracker line held. Personas flow through UX unchanged;
the GM persona's read-only constraint survived the stat-density addition.
Testability of pre-existing requirements unaffected. External deps unchanged —
Pathbuilder, Authentik, Cloudflare, Grizzly-Endeavors, Asgard all still carry
owners and fallbacks; the PB write-back non-goal strengthens the Pathbuilder
dependency story rather than adding to it.)*

---

## Round 6 Next Steps

1. Back to **prd-builder** in update mode for the four warnings: [R6-1] and
   [R6-2] are the cross-section propagation fixes (two UX sentences); [R6-3]
   is one provenance sentence mirroring the FG1 tooltip carve-out; [R6-4] is
   one line naming the phone layout's design source/owner (Dave) and gate.
2. The three 🔵 items are single-line edits; batch them into the same pass.
3. Re-check after that pass is expected to restore ✅ PASS — the v3.0 content
   decisions themselves are all sound; only their propagation is incomplete.

---
---

# ROUND 7 RE-CHECK — Hireling PRD (v3.1, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready
gate), run fresh against the full document. PRD: `docs/PRD.md` (v3.1 — "v3.0 +
round-6 propagation fixes"). UX reference re-verified:
`docs/reference/lorum_ipsum_dashboard.html` — `#pbExport` at line 314,
`CONDITIONS` map at line 480, and the 780px single-column mobile reflow
(`@media (max-width:780px)` → `flex-direction:column`) confirmed at line 46.
The PRD was not modified; rounds 1–6 content above is retained; this section
is appended.

## Round 6 Disposition (all four warnings + all three info items)

| # | Sev | Finding | Disposition | Evidence in v3.1 |
|---|-----|---------|-------------|------------------|
| R6-1 | 🟡 | "Spells freeform at POC; only conditions seeded" stale vs. P1 library | **Fixed, one residual clause** | UX Step 3 now reads "if it's in the seeded spell library (P1), its outcome template applies automatically; otherwise it's freeform via the picker" — the stale parenthetical is gone and the walkthrough's own example (Bless) now routes through the library. ⚠️ The fix was two clauses; the second did not land: FG3's seed bullet still says "Spells and other sources are freeform at POC" without the priority qualifier. See [R7-1] |
| R6-2 | 🟡 | UX stash flow predated sell/bank/quartermaster | **Fixed, one residual edge case** | UX Party Screens rewritten: add item, transfer, sell (quartermaster-only, value prompt defaulting to book, proceeds to the party bank shown in the stash header), claim-history log view; quartermaster toggle in party settings (owner action). Every FG4 capability now has a screen and an entry point, consistent with FG4's wording. ⚠️ One round-6 sub-question unanswered: whether a sale appends to the claim-history log. See [R7-2] |
| R6-3 | 🟡 | Spell outcome templates: no content provenance | **Fixed** | FG3 library bullet gains "**Provenance:** outcome templates follow the FG1 tooltip rule — hand-authored by Dave in the prototype's paraphrase style (rules-accurate, AoN-linked), reviewed by Josh before shipping; never model-generated straight into the product." Mirrors the FG1 carve-out exactly, including the named pair. The R4-1-class gap is closed on the spell library |
| R6-4 | 🟡 | Phone/tablet layout P0 with no design reference | **Fixed** | UX Step 1 now names an implementable floor — "the prototype's existing mobile treatment (780px single-column reflow)," **verified present** in the reference (`@media (max-width:780px)` collapsing `.cols` to a single column) — plus a gate: "its own design pass before frontend build — reviewed by Dave." An agent can build to the floor today; the design pass is sequenced, owned, and gated. Round 6's failure mode (agent inventing the phone UX) is closed |
| R6-5 | 🔵 | US-5 predated selling and the party bank | **Fixed** | US-5 now reads "dump the night's loot into a shared stash — and as quartermaster, sell what we don't keep into the party bank — and see who claimed what." Story text covers the FG4 sell/bank surface; FG4 citation intact |
| R6-6 | 🔵 | P1 composer additions absent from UX UI-elements list | **Fixed** | UX Step 3 gains "**P1 additions:** seeded-spell outcome tapper (pick degree of success → effects auto-apply) and conflict flags in the target picker (same-type suppression warned pre-assignment)" — both FG3 P1 behaviors now have a screen-inventory home, wording consistent with FG3 |
| R6-7 | 🔵 | GM stat density "key skills" undefined | **Fixed** | FG5 now reads "the character's three highest skill modifiers ('key skills', auto-selected)" — the subset is defined and the selection rule is mechanical |

**Disposition totals: 7 of 7 fixed (4 warnings, 3 info) — two with small
residuals surfacing below as [R7-1] / [R7-2]. 0 partially fixed. 0 not fixed.**

**Round-6 baseline regression sweep:** blanket expansion sets intact and still
partition the vocabulary; "defined expansion sets" stacking wording intact;
two-tier seed split intact; FG1 tooltip provenance (28 + 4 hand-authored by
Dave, reviewed by Josh) intact; both provenance examples still `Strike +14 =
+13 base +1 status (Bless, from Bear)`; story IDs US-1…US-9 with FG#-only
citations intact (all nine re-verified); roster-coverage metric grammar
intact; `sync_roundtrip_ms` scoping intact; PB write-back and
initiative-order non-goals intact and still double-stated. **No regressions
against the round-6 baseline.**

## Part 1: Summary Dashboard (Round 7)

### Agent-Ready Gate: ✅ PASS — Agent-ready

All three gate tests pass:

- **Specificity:** the v3.0 additions are now specified end to end — spell
  library (seeds, degrees of success, auto-apply, scope fences, provenance),
  sell/bank/quartermaster (actor, value default, destination, toggle
  location), GM stat density (every element a defined stat or a defined
  subset), conflict pre-warning (trigger, timing, worked example). Nothing
  relies on tribal knowledge.
- **Completeness:** every user-facing surface has a UX home with entry point
  and behavior — the stash flow covers all FG4 capabilities, the composer
  lists its P1 elements, and the phone layout has an implementable floor plus
  a gated design pass. Stories are individually addressable (US-1…US-9) with
  the acceptance-criteria convention stated and per-story FG# citations.
- **Unambiguity:** zero 🔴 Critical issues; zero 🟡 Warnings; priorities
  (P0/P1/P2) consistent across sections; the UX walkthrough and FG3 now state
  the same freeform-vs-library layering.

### Summary
- 🔴 Critical Issues: 0
- 🟡 Warnings: 0
- 🔵 Info: 2

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories (US-1–US-9, with
convention statement), Functional Requirements (P0–P2, six feature groups),
User Experience (entry point, core flow, party screens, edge cases),
Narrative, Success Metrics (+ Tracking Plan), Technical Considerations
(Tooling, UI, API/Backend, Hosting, Performance, Integration Points, Key
Risks)

### Sections Missing: None

### Story Coverage: 9 of 9 user stories carry acceptance criteria (by
convention — criteria live in the Functional Requirements; convention stated
explicitly and every story cites its covering FG#; all nine citations
re-verified against the requirement groups)

### Overall Assessment
v3.1 lands the round-6 propagation pass: all seven fixes are in, the two
externally-checkable claims (spell-library provenance, the 780px mobile floor)
verify against their sources, and the fresh full check found no new criticals
or warnings. Two info-level residuals remain — both single-clause edits, both
shavings off the round-6 fixes rather than new issues. The PRD is
agent-ready.

---

## Part 2: Detailed Findings (Round 7 — fresh full check)

### 3. Ambiguous Requirements (residuals, non-blocking)

**[R7-1] 🔵 FG3 seed bullet still says "freeform at POC" without the priority qualifier (R6-1 residual)**
- **Where:** FG3 seeded effect library — "Spells and other sources are
  freeform at POC: the composer offers a modifier picker…"
- **What:** Round 6's suggested fix was one clause in each of two spots; the
  UX Step 3 clause landed, this one did not. The harm is much reduced: the
  flagship walkthrough is now correct, and the adjacent library bullet
  reconciles the layering in-section ("Spells outside the library stay
  freeform via the modifier picker, exactly as P0"), so an agent reading FG3
  recovers the right rule. But "at POC" still literally asserts the
  pre-library world, and round 6 established that P1 ships in POC.
- **Suggested fix:** Three words — "Spells and other sources are freeform at
  P0 (outside the P1 spell library): the composer offers…"

**[R7-2] 🔵 Whether a sale appends to the claim-history log is unstated (R6-2 residual)**
- **Where:** FG4 — "**Claim history:** An append-only log of `{ item,
  quantity, from, to, actor, timestamp }` per transfer" and "**Sell:** …
  removes the item, and adds the proceeds to the party bank"; UX Party
  Screens — sell and the log view are both described, with no stated
  relationship.
- **What:** Round 6 asked whether a sale writes a log entry (a sale's `to` is
  the bank, not a character); v3.1 gives sell and the log adjacent UX homes
  but never says they interact. The log's stated purpose — "settles
  arguments" about who took what — argues it should; an agent could build
  either way. (The other round-6 sub-question — the zero-quartermaster state
  — is adequately covered: "quartermaster only" with no quartermaster
  designated means nobody can sell, derivable from the text as written.)
- **Suggested fix:** One line in FG4 or the UX stash flow, e.g.: "Sales
  append to the claim-history log with `to: party bank`."

*(Categories 1, 2, 4, 5, 6, 7, 8, 9, 10: no findings. Structural completeness
is full. No cross-section contradictions — UX Step 3, FG3's library bullet,
and the P1 composer elements now state one freeform-vs-library layering; the
stash UX and FG4 state the same four capabilities with the same actors;
US-5's broadened text matches FG4; FG5's defined "key skills" matches its own
stat list. Traceability verified story→FG for all nine stories and
goal→metric; the new P1 surfaces trace through existing citations (spell
library and conflict flags via US-6/US-8 → FG3, sell/bank via the broadened
US-5 → FG4). Stack prescriptions remain marked as settled house constraints.
No scope creep — v3.1 adds no features, only propagation. Personas flow
through UX; "quartermaster" follows the Caster convention (a role a player
holds, not a seat). Testability of pre-existing requirements unaffected.
External deps (Pathbuilder, Authentik, Cloudflare, Grizzly-Endeavors, Asgard)
unchanged, owners and fallbacks intact.)*

---

## Round 7 Next Steps

1. **Gate is ✅ PASS.** The PRD is agent-ready. [R7-1] and [R7-2] are
   single-clause edits; batch them into the next PRD touch — neither is worth
   a dedicated pass.
2. Next step in the pipeline: **prd-decomposer** to break into sequenced
   epics with SpecKit prompts, or hand the PRD (plus Dave's prototype as the
   UX reference) directly to engineering — P0 scope is small and the owners
   are the builders.
