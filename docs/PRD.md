# Hireling PRD
**Status:** v3.1 (2026-09-16) — v3.0 + round-6 propagation fixes
(7 findings: layouts, GM stat density, import special-cases, PB write-back ruling,
seeded spell library, stash sell/bank, conflict pre-warn)

**Owners:** Josh Flinn (PM), Dave (co-dev), Vex (PM/eng agent)
**Repo:** github.com/joshuaflinn/hireling

## tl;dr

Hireling is a party-linked Pathfinder 2e character tracker: a PWA where each player's
sheet (imported from Pathbuilder) stays live and in sync with the rest of the party.
When one character casts *bless*, the bonus appears — already mathed — on every
affected sheet, with a hover breakdown showing where every number comes from. POC
scope is one party of five players plus a read-only GM; the architecture leaves the
door open to feed RPGMastermind later without committing to it now.

## Goals

### Business Goals
- **Validate the party-sync concept at a real table.** The hypothesis: live, shared
  sheet state removes the table's two biggest friction taxes — buff math and
  "what does that do again?" Prove or kill it with six friendly users.
- **Zero-new-infrastructure.** Runs on existing lab assets (Mimir, Cloudflare,
  Authentik, Asgard). No paid services, no new vendors.
- **Optionality for RPGMastermind.** Keep the modifier engine and sync layer modular
  enough that a future harvest is a port, not a rewrite. Explicitly *not* designing
  for the harvest — "if it works, we'll see."

### User Goals
- Never manually recompute a stat because someone cast something.
- Never ask "what does that condition do again?" — the answer is one hover away.
- Trust that what I see on my sheet is what the party sees, right now.
- Use it one-handed on a phone, mid-combat, without a tutorial.

### Non-Goals
- **Character builder.** Pathbuilder owns character creation. Import only, forever
  (for this product's POC life).
- **Combat tracking.** No initiative *order*, no round counter, no turn timer
  (displaying an initiative *modifier* is a stat, not tracking — that's fine).
  The GM manages the table; we don't add to their plate.
- **Pathbuilder write-back.** Data flows one way: Pathbuilder → Hireling. The
  engine never generates "re-importable" PB exports — the schema is unofficial,
  a malformed write-back risks corrupting a character, and re-import anchoring
  already preserves session state (inventory deltas, HP, effects). Revisit only
  if the table genuinely misses it.
- **GM workload.** The GM seat is read-only. Nothing in MVP requires the GM to do
  anything — including click.
- **Positioning/aura automation.** Humans decide who is in the aura. The app does
  arithmetic, not adjacency.
- **Dice roller.** This table rolls physical dice, religiously. MVP+1 at earliest,
  likely not until a public release.
- **Duration countdown automation.** Casters track their own rounds; the app holds
  a duration *note*, not a ticking clock.
- **Multi-system support.** PF2e Remaster only. No PF1e, no 5e, no "generic mode."
- **Self-serve accounts / public release.** Friends-and-family POC.

## User Stories

Story IDs are US-1…US-9 (these are identifiers, not priorities — priorities live
on the requirement groups). Acceptance criteria for all stories live in the
Functional Requirements; each story cites its covering requirement group (FG#).

### The Player (Josh, Bear, Becky, Jake, Dave)
- **US-1.** As a player, I want to import my Pathbuilder JSON and immediately have a
  usable live sheet, so that I never retype a character. (FG1)
- **US-2.** As a player, I want my HP, spell slots, and consumables to update on
  everyone's screen when I change them, so the table stops playing telephone. (FG2)
- **US-3.** As a player, I want every condition and buff on my sheet to explain itself
  on hover, so I stop interrupting the GM with rules questions. (FG1, FG3)
- **US-4.** As a player, I want my sheet usable on my phone over spotty
  convention-hall wifi, so a dropped connection doesn't kill my turn. (FG2)
- **US-5.** As a player, I want to dump the night's loot into a shared stash —
  and as quartermaster, sell what we don't keep into the party bank — and see
  who claimed what, so the party loot list stops living in a group chat. (FG4)

### The Caster (a role any player holds mid-session, not a separate seat)
- **US-6.** As a caster, I want to apply an effect to chosen party members and have
  the app do the stacking math on each target's sheet, so nobody mis-adds a status
  bonus. (FG3)
- **US-7.** As a caster, I want to toggle targets off when they leave my aura and end
  the effect when it expires, because *I* am the authority on my spell — not the
  app. (FG3)
- **US-8.** As a caster, I want to see at a glance who is currently under my effects,
  so I can answer "wait, am I still blessed?" without scrolling. (FG3)

### The GM (Bruce — read-only, deliberately unburdened)
- **US-9.** As the GM, I want to glance at the party's real HP and active effects, so
  I can calibrate encounters — without being asked to click anything, ever. (FG5)

## Functional Requirements

### Feature Group 1 — Character Core (Priority: P0)
- **Pathbuilder import:** Paste or upload a Pathbuilder 2e JSON export; the full
  sheet derives from it. The import contract is defined by the reference export
  embedded in `docs/reference/lorum_ipsum_dashboard.html` (the `#pbExport` JSON
  block) — that shape is the spec. Failure classes, each with a human-readable
  message: (a) invalid JSON, (b) valid JSON but not a Pathbuilder export (missing
  required top-level keys), (c) unknown/unexpected fields — logged, import
  continues. There is no version pin (the export carries no version field);
  robustness comes from class (c). Class features that reshape slot layouts
  (Staff Nexus, wizard school spells, flexible spellcasting…) are handled
  per-feature as real imports surface them; every quirk lands as an importer
  test case — the reference export covers exactly one build, not the feature
  space.
- **Re-import anchoring:** Re-import replaces the base sheet while preserving live
  state. Anchors: HP/temp-HP by the character's identity (Authentik account →
  character, see FG2); spell-slot usage by slot rank + index; prep selections by
  slot rank + index; inventory deltas by item name. Entities that fail to match
  after re-import (renamed weapon, swapped spell) are kept, not dropped, and
  surfaced in a post-import diff for human review. Active effects are unaffected —
  they live server-side, not in the export.
- **Live sheet UI:** HP/temp-HP, AC, saves, Perception, skills with proficiency
  ranks, strikes/actions, spells (slots, focus, innate, cantrips) with prep and
  expenditure tracking, feats, inventory with containers (extradimensional
  contents excluded from Bulk), companions/minions panel. UX reference: Dave's
  Lorum Ipsum prototype (`docs/reference/lorum_ipsum_dashboard.html`) is the
  visual and interaction baseline.
- **Rules tooltips:** Conditions and game terms render as hoverable/pinnable
  popups with paraphrased rules text and Archives of Nethys links. **The content
  corpus is seeded from Dave's prototype** — its condition and rules text
  (paraphrased, AoN-linked, page-cited) is the starting corpus, not a scrape and
  not model-generated. The prototype's 28-condition map is missing four POC
  conditions (sickened, enfeebled, drained, slowed): those entries are
  **hand-authored by Dave in the same style** (paraphrase + AoN link + page cite)
  and reviewed by Josh before they ship — never model-generated straight into the
  product. POC coverage: every Player Core condition; other game
  terms are out of POC scope and get added on demand. Paraphrased
  rules text ships under the Paizo Community Use Policy / ORC notice in the repo.
- **Manual level adjust:** Level up/down control, scoped to **math rescale only** —
  proficiency bonuses, HP, and class DC scaling re-derive from level (the
  prototype's model). Ability boosts, feats, and skill increases are **not**
  applied by the app; those require a Pathbuilder re-export.
- **Persistence:** All live state survives refresh, reinstall, and device switch
  (server-side, not localStorage).

### Feature Group 2 — Party Sync (Priority: P0)
- **Party model:** A party has a roster of characters with owners. Schema is
  multi-party from day one (`party_id` on everything relevant); the POC UI only
  ever exposes one party. Second campaign = config, not migration.
- **Ownership:** A character's owner is its sole writer. Ownership is **claimed at
  import** (the importing Authentik account owns the character). One character per
  account at POC; reassignment is a manual admin/DB operation, not a UI feature.
  The caster is the sole writer of effects they created (even on other people's
  sheets). Everyone in the party reads everything. The GM account reads
  everything, writes nothing.
- **Real-time sync:** Any state change propagates to all connected party members
  via WebSocket, meeting the Technical Metrics latency targets (p95 < 1s on home
  wifi, < 3s on cellular).
- **Offline tolerance:** PWA caches last-known state for read; writes made offline
  queue and reconcile on reconnect. **Reconciliation rule:** the server assigns a
  monotonic version per field on receipt (server-receipt order wins — client
  clocks are untrusted). Field granularity: HP, temp-HP, each spell slot
  individually, each inventory item's quantity, each effect as a whole. A queued
  write that loses is silently superseded; the client's view re-renders to synced
  state. A subtle "syncing…" indicator shows while the queue is non-empty. No
  error theatre.
- **Degraded mode:** Backend unreachable → the PWA serves last-known state
  read-only and queues writes, exactly like offline. Session-night infra health
  (Mimir, tunnel) is a P0 operational dependency owned by Josh.

### Feature Group 3 — Buff/Effect Engine (Priority: P0)
- **Effect model:** `{ name, source_character, targets[], modifiers[], duration_note, active }`.
  Targets are **roster characters only**; companions/minions are not targetable
  entities at POC (their buffs are tracked manually on the owner's sheet).
  A modifier is `{ type, stat, value }`:
  - `type`: `circumstance | status | item | untyped`. Negative `value` = penalty.
  - `stat` — the closed vocabulary the engine recomputes:
    - Single stats: `ac`, `fort`, `ref`, `will`, `perception`, `speed`,
      `attack`, `damage`, `spell_attack`, `spell_dc`, `class_dc`,
      `skill:<name>` (one per PF2e skill)
    - Blanket targets: `all_checks`, `all_dcs`, `all_checks_and_dcs`
      (this is how *frightened* −1 works — no special-casing)
    - **Blanket expansion sets (rules-exact, decided):** `all_checks` = `attack`,
      `spell_attack`, `fort`, `ref`, `will`, `perception`, and every
      `skill:<name>` — every d20 roll, so NOT `damage` and NOT `speed`.
      `all_dcs` = `ac`, `class_dc`, `spell_dc` (AC is a DC per Player Core).
      `all_checks_and_dcs` = the union — frightened's exact footprint.
  - `attack` covers attack **rolls** only; damage rolls are `damage`. Both exist
    because e.g. *inspire courage* grants +1 status to each.
- **Engine-recomputed vs. static:** every numeric derived stat on the sheet is
  engine-recomputed from base + active effects. Non-numeric content (names, feat
  text, inventory items, spell lists) is static-from-import.
- **Stacking math (the whole rules engine):** among active modifiers on a stat,
  typed bonuses don't stack — the highest circumstance, highest status, and
  highest item bonus each apply once; untyped bonuses stack fully; penalties take
  the worst per type (untyped penalties stack). Blanket targets expand to their
  defined expansion sets before stacking is evaluated. The engine recomputes every derived
  stat on every affected sheet whenever any effect changes.
- **Provenance breakdown:** Every derived number on a sheet shows its math on
  hover — e.g. `Strike +14 = +13 base +1 status (Bless, from Bear)`, including
  *suppressed* sources (`+1 status (Bless) — Inspire Courage +1 also active,
  not stacked`). This is the feature that kills "what does that do again?"
- **Manual lifecycle:** The effect's creator adds/removes targets and ends the
  effect. The app never auto-expires, never checks range. Duration is a text
  note ("10 rounds", "while in aura"), displayed, not enforced.
- **Effect visibility:** Each sheet shows effects affecting it (with sources);
  the caster's view shows all their active effects and current targets.
- **Conflict pre-warning (P1):** The composer's target picker flags stacking
  conflicts *before* assignment — characters whose active same-type bonus would
  suppress the new effect are marked at pick time ("Becky: +1 status active —
  Bless would be suppressed"), so the caster decides with the math already done.
- **Seeded effect library (decided, two tiers):**
  - **Automatic seeds** — fully expressible in the stat vocabulary, engine math
    applies: **frightened** and **sickened** (−X status to `all_checks_and_dcs`),
    **off-guard** (−2 circumstance to `ac`).
  - **Manual-tracking seeds** — the rest of the valued conditions ship in the
    picker with their rules tooltip and a duration note but **no engine math**,
    badged "tracked manually": clumsy, enfeebled, stupefied (ability-scoped
    check/DC subsets the closed vocabulary can't express), drained (adds max-HP
    math), slowed and stunned (action economy, not modifiers). Same precedent as
    companion buffs: humans track what the engine can't. Extending the vocabulary
    to ability-scoped penalties is an engine-v2 conversation, not POC scope.
  - Spells and other sources are freeform at POC: the composer offers a modifier
    picker built from the stat vocabulary (stat → type → value), a name, a
    duration note, and the target picker.
- **Detrimental conditions ride the same engine** as effects with negative
  modifiers — one mechanism, both directions.
- **Seeded spell library (P1):** The party's commonly-cast spells (seed the top
  ~20 the table actually uses — Bless, Fear, Guidance, Heal…) carry structured
  outcome templates: cast → the composer offers the spell's degrees of success
  (crit success / success / failure / crit failure) → the caster taps what
  happened → the defined effects apply to the chosen targets automatically.
  No manual modifier definition for library spells. **Scope fences:** the
  library covers party-targeted effects; effects landing on *enemies* stay
  player-managed per the non-goals (enemies aren't roster entities — modeling
  them is combat tracking). Spells outside the library stay freeform via the
  modifier picker, exactly as P0. **Provenance:** outcome templates follow the
  FG1 tooltip rule — hand-authored by Dave in the prototype's paraphrase
  style (rules-accurate, AoN-linked), reviewed by Josh before shipping; never
  model-generated straight into the product.

### Feature Group 4 — Shared Inventory (Priority: P1)
- **Party stash:** A shared loot list with item, quantity, Bulk, and notes.
- **Transfers:** Move items between a character's inventory and the stash; both
  sides update live.
- **Claim history:** An append-only log of `{ item, quantity, from, to, actor,
  timestamp }` per transfer — settles arguments.
- **Sell:** Quartermaster-only action on a stash item — prompts for sale value
  (defaults to book value, editable for in-game negotiation), removes the item,
  and adds the proceeds to the party bank.
- **Party bank:** Shared currency ledger (gp/sp/cp) feeding from sales and
  manual adjustments; visible to all party members.
- **Quartermaster:** One character is designated quartermaster (owner-set,
  admin-style toggle); only that character's owner may sell from the stash.
  Claims/transfers remain open to all.

### Feature Group 5 — Account & Access (Priority: P0)
- **Authentik OIDC login:** Six pre-provisioned accounts (Josh, Bear, Dave, Becky,
  Jake, Bruce). No self-serve signup, no password code in the app.
- **GM view (decided: ships in POC):** Bruce's account lands on the **party view** —
  roster with per-character HP bars, down/max state, and active effect chips with
  sources. Read-only end to end: no edit affordances rendered for the GM account,
  no notifications, no action required of him, ever. **GM stat density (P1):**
  the party view's per-character cards extend to full glanceable stat blocks —
  current/max HP, AC, saves, Perception, spell/class DCs, initiative *modifier*,
  and the character's three highest skill modifiers ("key skills", auto-selected)
  — all read-only. Initiative *order* and turn tracking
  remain non-goals (stat display is data, not combat tracking).
  **Cross-member viewing (one
  rule, everywhere):** any account can open any character's full sheet read-only
  from the party view — ownership gates writes, nothing gates reads.
- **PWA install:** Installable on iOS/Android/desktop; app icon, splash, standalone
  display mode.

### Feature Group 6 — Later (Priority: P2)
- **Dice roller** (MVP+1; only if the table asks, which they won't — public-release feature).
- **Multi-party UI** (Dave's second campaign will force this; schema is ready).
- **GM encounter view** (only if Bruce asks; default remains zero-GM-work).

## User Experience

### Entry Point & First-Time Experience
- User browses to `https://hireling.flinntech.com` → Authentik login → lands on
  the party screen.
- First run (no character yet): empty party screen with an "Import your character"
  CTA (paste Pathbuilder JSON or upload the file). On success, the sheet appears
  and the character joins the party roster. A user who logs in before any
  character exists in the party sees the empty party screen with the import CTA —
  adopted as the design, no longer TBD.

### Core Experience (at the table)
- **Step 1:** Player opens the PWA → their sheet, current as of last sync.
  - UI Elements: **Two purposeful layouts, both P0, desktop built first.** The
    desktop (PC-resolution) layout is the primary build — Dave's three-column
    prototype design. The phone/tablet layout is a dedicated at-table design
    for one-handed use, not a responsive collapse of the desktop: its floor is
    the prototype's existing mobile treatment (780px single-column
    reflow), and it gets its own design pass before frontend build — reviewed
    by Dave, whose prototype already proves the taste.
- **Step 2:** Something changes HP — player taps +/-; the change renders locally
  instantly and syncs out.
  - Validation: HP clamped to [0, max]; temp HP absorbs damage first (standard
    PF2e order), shown as a distinct bar segment.
- **Step 3:** Bear casts *Bless* on Josh and Becky → Bear taps "new effect" →
  names it Bless → if it's in the seeded spell library (P1), its outcome
  template applies automatically; otherwise it's freeform via the picker
  (`+1 status to attack rolls`) →
  selects targets Josh, Becky → both sheets recompute, and every
  affected number shows its provenance.
  - UI Elements: effect composer (name, modifier picker from the FG3 stat
    vocabulary, duration note, target picker from party roster); effect chips on
    each sheet. **P1 additions:** seeded-spell outcome tapper (pick degree of
    success → effects auto-apply) and conflict flags in the target picker
    (same-type suppression warned pre-assignment).
- **Step 4:** Josh steps out of the aura → Bear removes Josh from targets → Josh's
  sheet reverts. No questions asked, literally.

### Party Screens
- **Party screen (all accounts):** roster of character cards — name, portrait
  initial, HP bar with down/max state, active effect chips with sources. Tapping
  a card opens that character's full sheet, read-only unless you're the owner.
- **GM view:** the party screen with zero interactive affordances. Bruce's account
  never renders an edit control.
- **Stash (P1):** the stash screen — add item, transfer to/from a character
  (pick from roster), sell (quartermaster only: prompts for sale value,
  defaults to book; proceeds land in the party bank shown in the stash
  header), and the claim-history log view. The quartermaster toggle lives in
  party settings (owner action). Transfers render live on both inventories.

### Advanced Features & Edge Cases
- **Conflicting effects:** Bless (+1 status) and a Bard's Inspire Courage (+1
  status) don't stack — the sheet shows `+1 status (Bless)` and names the
  suppressed source in the breakdown.
- **Offline at the table:** Sheet remains fully readable; writes queue; a subtle
  "syncing…" indicator appears. Losing writes are silently superseded (FG2 rule).
- **Re-import mid-campaign:** Pathbuilder re-export replaces base stats per the
  FG1 anchoring rules; unmatched entities are kept and surfaced in a post-import
  diff for human review.
- **Two devices, one owner:** Server-receipt-order per-field versioning; no
  locking UI.
- **Error states:** Failed import shows the FG1 failure-class message, not a
  stack trace. Dropped WebSocket auto-reconnects silently.

## Narrative

It's round two of the fight nobody was supposed to survive. Bear's cleric —
the party's battery — drops *Bless* and calls out "Josh, Becky,
you're in the aura." Last year this is where the table lost five minutes: Josh
digging for what bless does, Becky asking if it stacks with her *guidance*, the GM
re-explaining emanations. Tonight, neither of them looks up from their phones.
Josh's sheet already shows it: `Strike +14 = +13 base +1 status (Bless, from Bear)`.
Becky's attack modifier ticks up on its own. When Josh's fighter lunges too far
forward chasing the caster, Bear taps Josh's name off the target list and the
bonus quietly disappears from Josh's sheet before the GM finishes describing the
miss. Nobody asked anything. After the session, the party dumps the night's loot
into the shared stash from their seats, and Bruce — who never had to touch the
app once — glances at the party view and files away that everyone is one bad crit
from unconscious for next week's ambush. The app did arithmetic. The humans did
everything else. That's the deal.

## Success Metrics

### User-Centric Metrics
- **Table adoption:** Hireling is the only sheet open for a full session — no
  paper/PDF fallback — by the third session of use. (Self-reported by table vote.)
- **The question count:** "What does that do again?" asked at the table trends to
  ~zero for buffs/conditions once the engine is live. (Informal tally; the table
  will tell us.)
- **Roster coverage:** 5/5 players have imported sheets; all six accounts active
  (the GM needs no sheet).

### Business Metrics
- **POC verdict after 3 sessions:** keep investing, pivot, or kill. Explicit
  go/no-go, not vibes.
- **Dave's second campaign:** whether it onboards via config row (schema works)
  or demands engineering (schema failed).

### Technical Metrics
- **Sync latency:** p95 state-change propagation < 1s on home wifi, < 3s on
  cellular. **Instrumented:** the backend timestamps every state-change broadcast;
  `sync_roundtrip_ms` is logged per event (we own the server — this is free).
  This measures server-side broadcast latency; client-receipt confirmation is a
  productization refinement, not POC scope.
- **Session uptime:** 100% during scheduled game nights, measured by the existing
  house monitoring (Heimdall) against the service health endpoint.
- **Crash-free use (qualitative at POC):** no crash reports at the table. Six
  friendly users will tell us in person; formal crash instrumentation is a
  productization concern.

### Tracking Plan
- POC is six friendly users; most metrics are observed at the table, not
  instrumented. Instrumented exceptions: `sync_roundtrip_ms` (backend log) and
  service uptime (Heimdall). Deferred deliberately until productization:
  `effect_applied`, `effect_expired_manual`, `import_succeeded/failed`.

## Technical Considerations

*Stack choices in this section are settled house constraints, decided by the
owners — they are not negotiable requirements open for rediscovery.*

### Tooling & Quality Gate
- **Project scaffolding:** the backend crate adopts Bear's **rust-toolkit**
  conventions — the clippy deny block (~45 lints with inline reasoning),
  `clippy.toml` test exemptions, `rustfmt.toml`, `deny.toml` (cargo-deny
  supply-chain gate), pre-commit hooks, `justfile`, sibling test layout. The
  configs are **vendored at scaffold time** and tuned to this repo thereafter.
- **CI gate:** Bear's **grizzly-gate** image runs in **standalone checker mode**
  as the PR gate in GitHub Actions — fmt/lint/test plus SAST/secret/SCA in one
  versioned, fail-closed pass, pinned by image tag (upstream:
  `Grizzly-Endeavors/grizzly-gate`). The signed-image/admission-controller mode
  is platform-scale and explicitly out of POC scope. Agents run the local
  equivalent (`just ci-local`) before opening a PR — the gate is the reviewer.

### UI Architecture
- **Framework:** Svelte (Vite build), PWA via standard service worker + manifest.
  No SvelteKit — the backend owns routing/auth, keeping the frontend a static
  bundle. (Grug rule: nothing client-side that the server already does.)
- **Styling:** Hand-rolled CSS following Dave's prototype design language (dark
  fantasy, Palatino headers, gold accents). No component library.
- **State:** Local store mirrors server state; optimistic local render, server
  authoritative.

### API & Backend
- **Language/runtime:** Rust (axum), single binary. Josh's current-direction stack;
  also agent-friendly (the compiler is the cheapest code reviewer we have).
- **Sync:** WebSocket per party; server broadcasts state diffs. REST for
  import/auth/bootstrap only.
- **Authentication:** Authentik OIDC (existing house IdP). Six static accounts.
- **Database:** Postgres on **Asgard** — the house shared instance (Mimir,
  postgres 16, reachable only over the `asgard-net` bridge, no published ports).
  Hireling gets its own hall: database `hireling`, role `hireling` with
  `CONNECTION LIMIT 20` (matching the Langfuse hall precedent). Migrations via
  `sqlx migrate`, checked into the repo. Local dev runs a throwaway postgres
  container via compose.

### Hosting & Ops
- **Mimir** (Unraid, existing Docker host) → **cloudflared** tunnel (existing
  pattern: dwarfcampaign wiki) → **`hireling.flinntech.com`** (decided).
- Backups: the `hireling` hall rides Asgard's existing backup rotation; app
  itself is a stateless container (rebuild-from-git).

### Performance & Scalability
- Target: 6 concurrent users. Design headroom: one order of magnitude (60) with
  zero changes. Beyond that is success, and success gets a redesign conversation.

### Integration Points
- **Pathbuilder 2e JSON export** — the sole character source. Contract: the
  reference export embedded in `docs/reference/lorum_ipsum_dashboard.html`.
  *Risk: the export schema is unofficial and can drift. Mitigation: the FG1
  failure classes — invalid JSON and missing required keys fail loud and
  human-readable; unknown fields log and continue. Worst case is a manual
  re-export, never data loss (re-import preserves live state).*
- **Archives of Nethys** — outbound reference links in tooltips (read-only).
- **Authentik** — OIDC provider (existing).
- **Cloudflare Tunnel** — ingress (existing).
- **Grizzly-Endeavors** — rust-toolkit (vendored configs) and grizzly-gate
  (pinned image). Maintained by Bear; vendoring/pinning means his free time is
  not on our critical path.

### Key Risks
- **Modifier-engine edge cases** (weird stacking, untyped penalties, blanket
  conditions): mitigated by exhaustive unit tests against the core rulebook's
  worked examples. The engine is Constitution Article IV — stop-the-line on bugs.
- **Pathbuilder schema drift:** FG1 failure classes; worst case is a manual
  re-export, never data loss.
- **Scope creep toward a combat tracker:** every roadmap conversation will want
  it. The PRD says no. Point at this line.
