# PRD Quality Report: Hireling PRD (Draft v1, 2026-09-16)

Reviewed per the `prd-checker` skill (all 10 check categories + agent-ready gate).
PRD: `docs/PRD.md`. UX reference reviewed: `docs/reference/lorum_ipsum_dashboard.html`.
The PRD was not modified; this report is findings only.

---

## Part 1: Summary Dashboard

### Agent-Ready Gate: ⚠️ CONDITIONAL — Close but not ready

An agent can build large parts of this (sheet UI, import, auth, sync plumbing) from the
document plus Dave's prototype, but the following blockers will force guesswork or a
halt on the product's core differentiator:

- **Effect-model contract hole:** the modifier `stat` vocabulary is never enumerated, and
  the `{type, stat, value}` schema cannot express blanket conditions like *frightened*
  (−1 status to *all* checks and DCs). The engine is P0 and called "the whole rules
  engine" — its central data contract has a gap.
- **GM read-only seat scope is undecided** (Open Question says "ship or cut"), yet a GM
  user story and the Narrative assume it ships, and no functional requirement defines
  what the GM/party view contains.
- **Rules-tooltip content corpus is undefined** — who writes the paraphrased rules text,
  for how many conditions/terms, and under what license. (Dave's prototype already
  contains a paraphrased corpus; the PRD never says to reuse it.)
- **0 of 8 user stories carry acceptance criteria or story IDs**, and the PRD never
  states the convention that pass/fail conditions live in the Functional Requirements.
- **Re-import "re-anchored" semantics undefined** — nothing says how live state is
  matched to the new base sheet (name match? stable IDs? what breaks when a player
  renames something in Pathbuilder?).
- **No UX flow for the party screen, viewing another member's sheet, the GM view, or
  Shared Inventory (P1)** — Dave's prototype covers only the single-character sheet,
  so the novel party-linked UX exists only as prose fragments.

### Summary
- 🔴 Critical Issues: 2
- 🟡 Warnings: 13
- 🔵 Info: 5

### Sections Found:
tl;dr, Goals (Business/User/Non-Goals), User Stories, Functional Requirements (P0–P2),
User Experience (entry point, core flow, edge cases), Narrative, Success Metrics
(+ Tracking Plan), Technical Considerations (UI, API/Backend, Hosting, Performance,
Integration Points, Key Risks), Open Questions

### Sections Missing: None

### Story Coverage: 0 of 8 user stories carry acceptance criteria (and none carry IDs;
no convention statement says criteria live in the Functional Requirements)

### Overall Assessment
This is a genuinely good PRD — clear goals, disciplined non-goals, honest metrics, and
an unusually well-specified stacking rule — but it is not yet agent-ready on the one
feature that matters most: the effect engine's data contract is underspecified, the GM
seat's scope is still an open question, and every party-linked UX surface beyond the
single sheet is prose without a flow. Fix the two criticals and the top warnings and
this passes the gate cleanly.

---

## Part 2: Detailed Findings

### 2. Conflicting Information

**[1] 🔴 GM read-only seat: scope undecided, story assumes shipped, requirements silent**
- **Where:** Non-Goals ("GM tooling… Bruce gets a read-only seat. Nothing in MVP
  requires the GM to do anything."), User Stories → The GM ("glance at the party's real
  HP and active effects"), Narrative ("glances at the party view"), Feature Group 5
  (six accounts include Bruce), Open Questions ("GM read-only seat: ship in POC or
  cut? (Draft assumes ship…)").
- **What:** A user story and the narrative promise a GM party view; the Open Questions
  section says whether it ships is still undecided; and no functional requirement
  defines the view's contents. "Everyone in the party reads everything. GM seat is
  read-only" (Feature Group 2) states a permission, not a UI.
- **Why it matters:** An agent cannot resolve an open scope question. If it builds the
  view, it invents its contents; if it doesn't, a P0-adjacent user story fails.
  Undecided scope at PRD time becomes rework at decompose time.
- **Suggested fix:** Decide ship-or-cut. If ship: add a Feature Group requirement
  enumerating the party/GM view (roster, HP bars, active effect chips — explicitly read-
  only, no interaction affordances) and close the open question. If cut: cut the GM
  story and the Narrative beat.

### 3. Ambiguous Requirements

**[2] 🔴 Modifier `stat` vocabulary undefined; schema can't express blanket conditions**
- **Where:** Feature Group 3 — "A modifier is `{ type: circumstance | status | item |
  untyped, stat, value }`" and "Detrimental conditions (frightened, off-guard…) ride
  the same engine as effects with negative modifiers."
- **What:** `stat` has no defined vocabulary. The sheet has many targets (AC, Fort/Ref/
  Will, Perception, ~18 skills, strikes, spell attack/DC, class DC, speed, Bulk…), and
  the PRD never enumerates which the engine recomputes. Worse, *frightened* is "−1
  status penalty to all checks and DCs" — the single-`stat` schema cannot express
  "all," so the stated conditions-can't-be-special-cased claim fails on the model's
  own example. Does a modifier apply to damage rolls? To DCs? Undefined.
- **Why it matters:** The engine is P0 and described as "the whole rules engine." An
  agent must either invent the stat taxonomy (which *is* the product design) or stop
  and ask — exactly what the gate forbids. Every provenance string and every stacking
  test depends on this enumeration.
- **Suggested fix:** Add a stat enumeration (e.g., `ac | fort | ref | will | perception
  | skill:<name> | attack | spell_attack | spell_dc | class_dc | speed | all_checks |
  all_dcs`) or an explicit wildcard/aggregate rule, and state whether `attack` includes
  damage. Enumerate which sheet fields are engine-recomputed vs. static-from-import.

**[3] 🟡 Rules-tooltip content corpus: author, coverage, and license undefined**
- **Where:** Feature Group 1 — "Conditions and game terms render as hoverable/pinnable
  popups with paraphrased rules text and Archives of Nethys links."
- **What:** "Paraphrased" by whom, covering which conditions and "game terms," sourced
  how? Archives of Nethys content is ORC-licensed / Paizo Community Use — the PRD is
  silent on provenance and licensing for shipped rules text. Notably, Dave's prototype
  already embeds a paraphrased condition/spell corpus with AoN links and page cites —
  the PRD never says whether that corpus is the seed.
- **Why it matters:** Content work is the hidden iceberg of this requirement; an agent
  will either scrape AoN (license risk) or hallucinate paraphrases (rules-wrongness at
  the table — the exact failure mode the app exists to kill).
- **Suggested fix:** State the corpus source (recommended: "seed from the prototype's
  condition/rules data"), the POC coverage list (which conditions carry math vs.
  text-only), and a license note for the paraphrased text.

**[4] 🟡 Sync latency requirement untestable as written ("sane networks")**
- **Where:** Feature Group 2 — "propagates to all connected party members via WebSocket
  in under a second on sane networks."
- **What:** "Sane networks" is a vague quantifier. The Technical Metrics section pins
  the real targets ("p95 state-change propagation < 1s on home wifi, < 3s on
  cellular"), but the requirement itself can't be tested.
- **Why it matters:** The requirement and the metric are two statements of one
  behavior; they can drift.
- **Suggested fix:** Point the requirement at the metric ("meets the Technical Metrics
  latency targets") or inline the p95 numbers in the requirement.

**[5] 🟡 Re-import "re-anchoring" rules undefined**
- **Where:** Feature Group 1 — "Re-import replaces the base sheet while preserving live
  state (HP, slots used, active effects)"; UX edge case — "live state (HP, active
  effects, inventory deltas) is preserved and re-anchored."
- **What:** Nothing defines the anchor. Match items/slots/effects by name? By
  Pathbuilder IDs? What happens when a player renames a weapon, changes a spell in a
  prepped slot, or levels up in Pathbuilder before re-export — is that slot "used" or
  fresh? Also, the two statements disagree: the UX list includes "inventory deltas,"
  the FR list doesn't.
- **Why it matters:** Re-import mid-campaign is a listed edge case with data-loss
  potential; the matching rule is the whole feature.
- **Suggested fix:** Specify the anchor key per entity (e.g., spell slots by rank+index,
  items by name, effects unaffected since they live server-side), the behavior on
  unmatched entities (keep? drop? surface a diff?), and reconcile the two lists of
  what is preserved.

**[6] 🟡 Ownership model: Authentik user → character mapping undefined**
- **Where:** Feature Group 2 — "A character's owner is the sole writer of that
  character"; Feature Group 5 — "Six pre-provisioned accounts."
- **What:** How is ownership established — first import claims? Admin-assigned? Can one
  user own two characters (a player running a PC plus a companion-adjacent NPC)? Can
  ownership transfer (player switches characters between campaigns)?
- **Why it matters:** Permissions are P0 and every write path checks this rule; the
  assignment mechanism is unspecified.
- **Suggested fix:** One line: "ownership is claimed at import; one character per
  account at POC; reassignment is a manual admin/db operation."

**[7] 🟡 Offline reconciliation: "last-writer-wins per field" clock semantics undefined**
- **Where:** Feature Group 2 — "writes made offline queue and reconcile last-writer-
  wins per field on reconnect."
- **What:** Last by whose clock — server receipt order or client timestamp? Client
  clocks on phones are unreliable; server-receipt-order LWW punishes the client that
  was offline longest (its queued writes always lose). Per-field granularity is also
  undefined (is HP one field? temp-HP? slot pips individually?).
- **Why it matters:** This is the correctness rule for the offline story the PRD
  sells ("spotty convention-hall wifi"). Two reasonable implementations give
  different table-visible outcomes.
- **Suggested fix:** State the rule explicitly (recommended: server assigns a monotonic
  version per field on receipt; define the field granularity), plus what the user sees
  when their queued write loses.

**[8] 🟡 "Manual level adjust: re-derives stats" — derivation scope undefined**
- **Where:** Feature Group 1 — "Level up/down control that re-derives stats, for
  tables that level mid-session."
- **What:** Level change in PF2e touches proficiency bonuses, HP, and class DC — but
  also ability boosts (5/10/15/20), skill increases, and feats at even levels. Which
  does "re-derives" apply? (The prototype recomputes from the export at arbitrary
  level, which answers part of this, but the PRD doesn't say that's the model.)
- **Why it matters:** An agent that applies full leveling rules will demand feat/
  boost choices the UI has no surface for; one that only rescales math will produce
  sheets the table calls wrong at boost levels.
- **Suggested fix:** State the scope explicitly (recommended: "math rescale only —
  ability boosts, feats, and skill increases require a Pathbuilder re-export").

**[9] 🟡 Effect targets and companions: roster membership undefined**
- **Where:** Feature Group 3 — `targets[]`; UX Step 3 — "target picker from party
  roster"; Feature Group 1 — "companions/minions panel."
- **What:** Can a companion/minion be an effect target? They have AC/saves/HP in
  PF2e and *bless* affects them at real tables. Are they party-roster entries or
  sub-panels of their owner's sheet? The picker says "party roster"; the roster is
  "characters with owners"; companions are neither defined as characters nor excluded.
- **Why it matters:** First table session with a druid or summoner hits this.
- **Suggested fix:** One line either way — e.g., "targets are roster characters only;
  companion buffs are tracked manually at POC" or "companions appear as targetable
  sub-entities under their owner."

**[10] 🟡 Effect library seeding undecided (open question driving a P0 feature's UX)**
- **Where:** Open Questions — "Effect library: pre-seed common effects… vs. freeform-
  only at POC. (Draft: pre-seed the conditions that carry math; spells freeform with
  sensible modifier pickers.)"
- **What:** The draft leaning is good, but it is still phrased as an open question, and
  "sensible modifier pickers" is undefined (a picker implies a stat list — blocked on
  issue [2]).
- **Why it matters:** The effect-composer UX (the caster's core flow) differs
  materially between the two options.
- **Suggested fix:** Adopt the draft leaning as a decision, enumerate the seeded
  conditions, and define the modifier picker from the stat taxonomy in [2].

**[11] 🟡 Pathbuilder import contract undefined (integration point without a contract)**
- **Where:** Feature Group 1 — "Paste or upload a Pathbuilder 2e JSON export";
  Integration Points — "the export schema is unofficial and can drift… version pinned
  per import."
- **What:** Which export fields are required vs. optional? What is the error taxonomy
  ("human-readable parse error" — for which failure classes: invalid JSON, wrong
  schema, missing fields, unknown fields)? What does "version pinned per import" mean
  operationally (the export carries no version field — pinned to what)?
- **Why it matters:** Import is the sole character source and the first thing every
  user touches; "validates and reports unknown fields" is not a contract an agent can
  implement against without the field list. (The prototype embeds a real export — a
  usable sample contract the PRD could point to.)
- **Suggested fix:** Enumerate the consumed field list (or point at the prototype's
  embedded export as the reference shape), define the failure classes and their
  messages, and drop or define "version pinned."

**[12] 🟡 Technical metrics have no instrument (Tracking Plan TBD contradicts metric targets)**
- **Where:** Success Metrics — "Sync latency: p95… < 1s on home wifi, < 3s on cellular.
  Session uptime: 100%… Crash-free PWA sessions: > 99%." vs. Tracking Plan — "[TBD —
  metrics are observed at the table, not instrumented. Deferred deliberately.]"
- **What:** Three quantitative targets exist with no measurement method. "Observed at
  the table" can cover adoption and the question count; it cannot produce a p95 or a
  crash-free percentage.
- **Why it matters:** Untestable metrics are vibes with numbers; the POC verdict is
  supposed to be "explicit go/no-go, not vibes."
- **Suggested fix:** Either add minimal instrumentation (the deferred event list is
  right there — `sync_roundtrip_ms` covers the latency metric) or downgrade those
  three metrics to qualitative observations and say so.

### 5. Traceability

**[13] 🟡 Shared Inventory (P1) traces to no user story or stated goal**
- **Where:** Feature Group 4 (party stash, transfers, claim history).
- **What:** No user story mentions the stash; no user or business goal requires it.
  The Narrative uses it, and "settles arguments" is a real benefit, but per the
  scope test: cut Feature Group 4 and every P0 goal still stands.
- **Why it matters:** Hidden scope enters exactly this way — a P1 group with no trace
  survives decomposition because it looks small, then grows a transfer UI, a log UI,
  and conflict handling.
- **Suggested fix:** Add one user story (player or table persona: "dump loot in the
  stash and see who took what") or explicitly mark the group as table-polish justified
  by the adoption goal.

**[14] 🟡 Stories carry no acceptance criteria, no IDs, and no convention statement**
- **Where:** User Stories (all 8).
- **What:** The Functional Requirements are largely testable and do cover most stories,
  so this is a 🟡 not a 🔴 — but the PRD never says pass/fail conditions live there,
  and stories are an unanchored bullet list (no S1…S8 IDs or stable headings).
- **Why it matters:** Every story-to-ticket handoff re-derives criteria from another
  section, and decomposition (prd-decomposer/SpecKit) needs addressable stories to
  trace epics back to. Silence on the convention is a gap, not a convention.
- **Suggested fix:** Add story IDs and one line — "acceptance criteria for all stories
  live in the Functional Requirements" — then verify every story maps to a requirement
  (the GM story currently doesn't; see [1]).

### Requirements → UX coverage

**[15] 🟡 No UX flow for the party screen, cross-member sheet views, GM view, or stash**
- **Where:** User Experience — covers the single sheet (Steps 1–4) well, but: "lands on
  the party screen" (contents undefined); "Everyone in the party reads everything"
  (no flow for viewing another member's sheet); GM view (see [1]); Feature Group 4
  (stash/transfer/claim-history flows absent); entry-point empty state still [TBD].
  Dave's prototype is a single-character sheet — it fills none of these gaps.
- **What:** The prototype is correctly cited as the sheet's "visual and interaction
  baseline," but the *party-linked* surfaces — the product's differentiator — have no
  interaction specification beyond named UI elements ("effect chips," "target picker").
- **Why it matters:** An agent will invent the party screen, the effect composer's
  layout, and the stash flows. Some invention is fine at POC; unreviewed invention on
  the differentiating UX is how the third-session adoption vote fails.
- **Suggested fix:** Add one flow each: party screen (what's on it, click-through to
  member sheets), GM view (subset of party screen, read-only), and stash
  (add/transfer/history). Resolve the entry-point TBD with the stated safe default.

### 4. Technical Agnosticism

**[16] 🔵 Heavy implementation prescription — likely deliberate, but unmarked**
- **Where:** Technical Considerations — Svelte (no SvelteKit), hand-rolled CSS, Rust/
  axum single binary, `sqlx migrate`, WebSocket-per-party, cloudflared.
- **What:** Normally these are PM overreach; here the PM is also the co-dev, the
  "zero-new-infrastructure" goal makes infra choices genuine constraints, and the
  stack choice is justified in-line ("Josh's current-direction stack; also agent-
  friendly").
- **Why it matters:** Only if this PRD later feeds RPGMastermind or another reader who
  treats it as requirements — the prescriptions travel as if they were constraints.
- **Suggested fix:** One line at the top of Technical Considerations: "stack choices in
  this section are house constraints, not negotiable requirements."

### 8. Persona Consistency

**[17] 🔵 Personas are names only; GM persona absent from UX flows**
- **Where:** User Stories — personas are "Josh, Bear, Becky, Jake, Dave," "any player
  running a buff/debuff," "Bruce."
- **What:** Fine for a friends-and-family POC (the personas are literally the users),
  but the GM persona never appears in a UX flow (consequence of [1]/[15]), and
  "The Caster" is a role any player holds — worth saying so explicitly to keep it from
  reading as a sixth seat.
- **Suggested fix:** One clarifying line on the Caster role; GM flow follows from
  fixing [1].

### 10. Cross-PRD Dependencies

**[18] 🔵 External references an agent can't see: `asgard/README.md` guardrails, house policy**
- **Where:** Technical Considerations — "per-role CONN LIMIT per `asgard/README.md`
  guardrails," "per the standing house policy."
- **What:** Both references are real house documents, but neither ships with the repo;
  an agent (or Dave's agent) building the db layer can't read the guardrails it is
  told to follow.
- **Suggested fix:** Inline the specific guardrail values (conn limit, role naming) in
  the PRD or copy the relevant README excerpt into `docs/`.

**[19] 🔵 House-infra dependencies lack a named owner/fallback — acceptable here, but note it**
- **Where:** Integration Points — Authentik, Cloudflare Tunnel, Asgard/Mimir postgres.
- **What:** All three are existing house systems owned by the same household, so risk
  is low and the flag is informational: "Session uptime: 100% during scheduled game
  nights" makes Mimir + tunnel health a silent P0 dependency with no stated fallback
  (e.g., read-only cached sheet is already covered; *write* outage behavior during a
  session is not).
- **Suggested fix:** One line on degraded-mode behavior ("backend down → PWA serves
  last-known state read-only, writes queue") — it mostly follows from offline
  tolerance, but say it.

### 6. Section Completeness (residual)

**[20] 🔵 Hostname TBD; entry-point empty-state TBD**
- **Where:** Hosting & Ops — "hostname on flinntech.com [TBD: hireling.flinntech.com]";
  UX Entry Point — "[TBD — need: what a user sees if they log in before any character
  exists… Proposal: empty party screen with import CTA; safe default.]"
- **What:** Two open TBDs with safe defaults already proposed. Neither blocks an agent
  (Cloudflare config is an ops step; the empty-state proposal is sound), but both
  should be promoted from TBD to decision so the document is self-contained.
- **Suggested fix:** Adopt both proposals as written.

---

## Next Steps

1. Take this report back to **prd-builder** in update mode: resolve the two criticals
   ([1] GM scope decision, [2] modifier stat taxonomy) first — everything else is a
   paragraph each.
2. Re-run prd-checker; expect the gate to flip to ✅ once [1], [2], [5], and [15] land.
3. Then **prd-decomposer** for epic breakdown — or straight to engineering, since the
   P0 scope is small and the owners are the builders.
