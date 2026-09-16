# Hireling PRD

**Status:** Draft v1 (2026-09-16) — for review by Josh & Dave
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
  Authentik). No paid services, no new vendors.
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
- **Combat tracking.** No initiative, no round counter, no turn timer. The GM
  manages the table; we don't add to their plate.
- **GM tooling.** Bruce gets a read-only seat. Nothing in MVP requires the GM to
  do anything.
- **Positioning/aura automation.** Humans decide who is in the aura. The app does
  arithmetic, not adjacency.
- **Dice roller.** This table rolls physical dice, religiously. MVP+1 at earliest,
  likely not until a public release.
- **Duration countdown automation.** Casters track their own rounds; the app holds
  a duration *note*, not a ticking clock.
- **Multi-system support.** PF2e Remaster only. No PF1e, no 5e, no "generic mode."
- **Self-serve accounts / public release.** Friends-and-family POC.

## User Stories

### The Player (Josh, Bear, Becky, Jake, Dave)
- As a player, I want to import my Pathbuilder JSON and immediately have a usable
  live sheet, so that I never retype a character.
- As a player, I want my HP, spell slots, and consumables to update on everyone's
  screen when I change them, so the table stops playing telephone.
- As a player, I want every condition and buff on my sheet to explain itself on
  hover, so I stop interrupting the GM with rules questions.
- As a player, I want my sheet usable on my phone over spotty convention-hall wifi,
  so a dropped connection doesn't kill my turn.

### The Caster (any player running a buff/debuff)
- As a caster, I want to apply an effect to chosen party members and have the app
  do the stacking math on each target's sheet, so nobody mis-adds a status bonus.
- As a caster, I want to toggle targets off when they leave my aura and end the
  effect when it expires, because *I* am the authority on my spell — not the app.
- As a caster, I want to see at a glance who is currently under my effects, so I
  can answer "wait, am I still blessed?" without scrolling.

### The GM (Bruce — read-only, deliberately unburdened)
- As the GM, I want to glance at the party's real HP and active effects, so I can
  calibrate encounters — without being asked to click anything, ever.

## Functional Requirements

### Feature Group 1 — Character Core (Priority: P0)
- **Pathbuilder import:** Paste or upload a Pathbuilder 2e JSON export; the full
  sheet derives from it. Re-import replaces the base sheet while preserving live
  state (HP, slots used, active effects).
- **Live sheet UI:** HP/temp-HP, AC, saves, Perception, skills with proficiency
  ranks, strikes/actions, spells (slots, focus, innate, cantrips) with prep and
  expenditure tracking, feats, inventory with containers (extradimensional
  contents excluded from Bulk), companions/minions panel. UX reference: Dave's
  Lorum Ipsum prototype (`docs/reference/lorum_ipsum_dashboard.html`) is the
  visual and interaction baseline.
- **Rules tooltips:** Conditions and game terms render as hoverable/pinnable
  popups with paraphrased rules text and Archives of Nethys links.
- **Manual level adjust:** Level up/down control that re-derives stats, for tables
  that level mid-session.
- **Persistence:** All live state survives refresh, reinstall, and device switch
  (server-side, not localStorage).

### Feature Group 2 — Party Sync (Priority: P0)
- **Party model:** A party has a roster of characters with owners. Schema is
  multi-party from day one (`party_id` on everything relevant); the POC UI only
  ever exposes one party. Second campaign = config, not migration.
- **Real-time sync:** Any state change propagates to all connected party members
  via WebSocket in under a second on sane networks.
- **Ownership & permissions:** A character's owner is the sole writer of that
  character. The caster is the sole writer of effects they created (even on other
  people's sheets). Everyone in the party reads everything. GM seat is read-only.
- **Offline tolerance:** PWA caches last-known state for read; writes made offline
  queue and reconcile last-writer-wins per field on reconnect. HP desync between
  two devices of the same owner resolves to the most recent write.

### Feature Group 3 — Buff/Effect Engine (Priority: P0)
- **Effect model:** `{ name, source_character, targets[], modifiers[], duration_note, active }`.
  A modifier is `{ type: circumstance | status | item | untyped, stat, value }`,
  where a negative value is a penalty. Detrimental conditions (frightened,
  off-guard…) ride the same engine as effects with negative modifiers — one
  mechanism, both directions.
- **Stacking math (the whole rules engine):** among active modifiers on a stat,
  typed bonuses don't stack — the highest circumstance, highest status, and
  highest item bonus each apply once; untyped bonuses stack fully; penalties take
  the worst per type (untyped penalties stack). The engine recomputes every
  derived stat on every affected sheet whenever any effect changes.
- **Provenance breakdown:** Every derived number on a sheet shows its math on
  hover — e.g. `Will +14 = +13 base +1 status (Bless, from Bear)`. This is the
  feature that kills "what does that do again?"
- **Manual lifecycle:** The effect's creator adds/removes targets and ends the
  effect. The app never auto-expires, never checks range. Duration is a text
  note ("10 rounds", "while in aura"), displayed, not enforced.
- **Effect visibility:** Each sheet shows effects affecting it (with sources);
  the caster's view shows all their active effects and current targets.

### Feature Group 4 — Shared Inventory (Priority: P1)
- **Party stash:** A shared loot list with item, quantity, Bulk, and notes.
- **Transfers:** Move items between a character's inventory and the stash; both
  sides update live.
- **Claim history:** A simple log of who took what, when — settles arguments.

### Feature Group 5 — Account & Access (Priority: P0)
- **Authentik OIDC login:** Six pre-provisioned accounts (Josh, Bear, Dave, Becky,
  Jake, Bruce). No self-serve signup, no password code in the app.
- **PWA install:** Installable on iOS/Android/desktop; app icon, splash, standalone
  display mode.

### Feature Group 6 — Later (Priority: P2)
- **Dice roller** (MVP+1; only if the table asks, which they won't — public-release feature).
- **Multi-party UI** (Dave's second campaign will force this; schema is ready).
- **GM encounter view** (only if Bruce asks; default remains zero-GM-work).

## User Experience

### Entry Point & First-Time Experience
- User browses to `https://<host>` → Authentik login → lands on the party screen.
- First run: "Import your character" prompt (paste Pathbuilder JSON or upload the
  file). On success, the sheet appears and the character joins the party roster.
- [TBD — need: what a user sees if they log in before any character exists in the
  party. Proposal: empty party screen with import CTA; safe default.]

### Core Experience (at the table)
- **Step 1:** Player opens the PWA → their sheet, current as of last sync.
  - UI Elements: Dave's three-column sheet layout (prototype), collapsed to a
    single column on phone.
- **Step 2:** Something changes HP — player taps +/-; the change renders locally
  instantly and syncs out.
  - Validation: HP clamped to [0, max]; temp HP absorbs damage first (standard
    PF2e order), shown as a distinct bar segment.
- **Step 3:** Bear casts *Bless* on Josh and Becky → Bear taps "new effect" →
  picks Bless (or freeforms it) → modifier `+1 status to attack rolls,
  Perception…` → selects targets Josh, Becky → both sheets recompute, and every
  affected number shows its provenance.
  - UI Elements: effect composer (name, modifiers, duration note, target picker
    from party roster); effect chips on each sheet.
- **Step 4:** Josh steps out of the aura → Bear removes Josh from targets → Josh's
  sheet reverts. No questions asked, literally.

### Advanced Features & Edge Cases
- **Conflicting effects:** Bless (+1 status) and a Bard's Inspire Courage (+1
  status) don't stack — the sheet shows `+1 status (Bless)` and notes the
  suppressed source in the breakdown.
- **Offline at the table:** Sheet remains fully readable; writes queue; a subtle
  "syncing…" indicator appears. No error theatre.
- **Re-import mid-campaign:** Pathbuilder re-export replaces base stats; live
  state (HP, active effects, inventory deltas) is preserved and re-anchored.
- **Two devices, one owner:** Last write wins per field; no locking UI.
- **Error states:** Failed import shows a human-readable parse error, not a stack
  trace. Dropped WebSocket auto-reconnects silently.

## Narrative

It's round two of the fight nobody was supposed to survive. Bear's cleric —
the party's battery — drops *Bless* and calls out "Josh, Becky,
you're in the aura." Last year this is where the table lost five minutes: Josh
digging for what bless does, Becky asking if it stacks with her *guidance*, the GM
re-explaining emanations. Tonight, neither of them looks up from their phones.
Josh's sheet already shows it: `Will +14 = +13 base +1 status (Bless, from Bear)`.
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
- **Roster coverage:** 5/5 players + GM have accounts and imported sheets.

### Business Metrics
- **POC verdict after 3 sessions:** keep investing, pivot, or kill. Explicit
  go/no-go, not vibes.
- **Dave's second campaign:** whether it onboards via config row (schema works)
  or demands engineering (schema failed).

### Technical Metrics
- **Sync latency:** p95 state-change propagation < 1s on home wifi, < 3s on
  cellular.
- **Session uptime:** 100% during scheduled game nights. (Mimir + tunnel health.)
- **Crash-free PWA sessions:** > 99%.

### Tracking Plan
- [TBD — POC is six friendly users; metrics are observed at the table, not
  instrumented. If we productize: effect_applied, effect_expired_manual,
  import_succeeded/failed, sync_roundtrip_ms. Deferred deliberately.]

## Technical Considerations

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
- **Database:** SQLite (moderated WAL). Six users; anything bigger is a product
  decision, not a POC one.
- **Modifier engine:** Pure, isolated Rust module — takes base stats + active
  effects, returns derived stats + provenance breakdown. Fully unit-tested; this
  is the module RPGMastermind would harvest, and the only one designed for it.

### Hosting & Ops
- **Mimir** (Unraid, existing Docker host) → **cloudflared** tunnel (existing
  pattern: dwarfcampaign wiki) → hostname on flinntech.com [TBD: hireling.flinntech.com].
- Backups: nightly SQLite snapshot into the existing Mimir backup rotation.

### Performance & Scalability
- Target: 6 concurrent users. Design headroom: one order of magnitude (60) with
  zero changes. Beyond that is success, and success gets a redesign conversation.

### Integration Points
- **Pathbuilder 2e JSON export** — the sole character source. *Risk: the export
  schema is unofficial and can drift. Mitigation: importer validates and reports
  unknown fields rather than dying; version pinned per import.*
- **Archives of Nethys** — outbound reference links in tooltips (read-only).
- **Authentik** — OIDC provider (existing).
- **Cloudflare Tunnel** — ingress (existing).

### Key Risks
- **Modifier-engine edge cases** (weird stacking, untyped penalties): mitigated by
  exhaustive unit tests against the core rulebook's worked examples.
- **Pathbuilder schema drift:** importer fails loud and human-readable; worst case
  is a manual re-export, never data loss.
- **Scope creep toward a combat tracker:** every roadmap conversation will want
  it. The PRD says no. Point at this line.

## Open Questions
- Hostname confirmation (hireling.flinntech.com?).
- GM read-only seat: ship in POC or cut? (Draft assumes ship — it's cheap and
  Bruce asked for nothing, which is exactly why we can afford to give him a
  zero-effort view.)
- Effect library: pre-seed common effects (Bless, Bane, Guidance, Inspire
  Courage, common conditions) vs. freeform-only at POC. (Draft: pre-seed the
  conditions that carry math; spells freeform with sensible modifier pickers.)
