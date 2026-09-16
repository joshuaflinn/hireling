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
