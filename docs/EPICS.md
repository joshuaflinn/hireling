# Hireling — Epic Decomposition

Generated 2026-09-18 from PRD v3.6 (`docs/PRD.md`). Each epic carries **one
specify prompt**, with the PRD's settled tech constraints folded in as a
**Constraints** block. AI Guardrails carry the implicit engineering
requirements the PRD doesn't spell out. Checker reports (rounds 8–9,
`docs/PRD-CHECK.md`) flagged builder-level detail that intentionally lives
HERE and downstream, not in the PRD: metric endpoints, stash/bank
reconciliation granularity, key-skills tie-break.

**The feature pipeline (Josh, 2026-09-18):** every epic runs
**specify → clarify → brainstorm → plan → subagent development**, with a
human approval gate at each arrow. The specify prompt in each issue feeds
step one; the spec is not a build order. (E1 was grandfathered in as
scaffolding before this ruling.)

**Build order:** Phase 0 → 1 → 2 (go-live) → 3 (P1) → 4 (P2 placeholders).

## Build Sequence Overview

| Phase | Lane | Epic | Priority | Dependencies | Summary |
|-------|------|------|----------|-------------|---------|
| 0 | — | E1 App scaffold & dev stack | P0 | none | axum binary + Svelte shell + compose postgres + /healthz |
| 0 | — | E2 Database schema | P0 | E1 | Asgard hall, sqlx migrations, multi-party from day one |
| 0 | — | E3 Authentik OIDC auth | P0 | E1 | six accounts, sessions, read/write gating |
| 0 | — | E4 Rules corpus importer | P0 | E1 | Foundry pf2e ETL: conditions + items, source lanes, license gate |
| 1 | A | E5 Pathbuilder import pipeline | P0 | E2, E3 | parser, failure classes, re-import anchoring, ownership claim |
| 1 | A | E6 Live sheet UI (desktop) | P0 | E5 | full sheet, design language, level adjust, persistence |
| 1 | B | E7 Party sync & realtime | P0 | E2, E3 | WS broadcast, offline queue, per-field versioning, degraded mode |
| 1 | sync | E8 Buff/effect engine | P0 | E4, E5, E7 | vocabulary, stacking math, recomputation, provenance breakdown |
| 1 | A | E9 Rules tooltips + custom entry | P0 | E4, E6 | hover popups, curated prose, custom-lane forms at point of use |
| 1 | B | E10 Party view, GM seat, PWA | P0 | E6, E7, E8 | roster cards, read-only everywhere, installable PWA |
| 2 | — | E11 Go-live deploy | P0 | E6–E10 | Mimir container, tunnel, hireling.flinntech.com, Heimdall probe |
| 3 | — | E12 Shared inventory (FG4) | P1 | E10, E4 | stash, transfers, claim history, sell, bank, quartermaster |
| 3 | — | E13 Seeded spell library + conflict pre-warn (FG3 P1) | P1 | E8 | outcome templates, degrees-of-success tapper, pick-time warnings |
| 3 | — | E14 GM stat density (FG5 P1) | P1 | E10 | full stat-block cards, key skills, initiative modifier |
| 3 | — | E15 Curation editing (FG1 P1) | P1 | E9 | prose editing on imported rows |
| 4 | — | P2 parking lot | P2 | — | phone/tablet layout · dice roller · multi-party UI · GM encounter view · scenario-aware sheet views (issue #19) |

## Parallel Execution Map

```
Phase 0:  E1 ──► E2 ─┬─► E5 ──► E6 ──► E9          (Lane A: character)
                    ├─► E7 ──► E10                  (Lane B: party)
                    ├─► E3 ─┘       │
                    └─► E4 ──► E8 ◄─┴── sync point (needs E4+E5+E7)
                            E8 ──► E10
Phase 2:  E11 (all of Phase 1)
Phase 3:  E12 ∥ E13 ∥ E14 ∥ E15 (independent of each other)
```

⚠️ **Conflict risks:** E6 and E10 both render character data — E10's roster
cards should consume E6's sheet components, not re-implement them. E9 and
E13 both touch the effect composer (E9 adds the custom-spell entry point,
E13 adds the library tapper) — sequence E13 after E9.

---

## Phase 0: Foundations

### Epic E1 — App scaffold & dev stack
**Dependencies:** none · **Priority:** P0 · **Parallel:** blocks everything

**Specify prompt:**
> Build the Hireling app scaffold: a single Rust (axum) backend binary that serves both the JSON API and the static frontend bundle, plus a Svelte (Vite) frontend shell with a placeholder route. Local development runs a throwaway Postgres container via docker compose. The backend exposes `GET /healthz` returning 200 with a version string. Include `just` recipes for dev, test, and a local gate run. This is the skeleton every later epic builds on — no product features.

**Constraints (settled, not negotiable):**
> Rust + axum, single binary, backend owns routing (no SvelteKit — frontend is a static bundle). Existing repo root already vendors the rust-toolkit crate `hireling` and the grizzly-gate CI (gate-config.json, pinned image) — extend, don't replace. Repo: github.com/joshuaflinn/hireling.

**AI Guardrails:**
> Structured logging (JSON, correlation IDs). Env-var config for dev/prod (DB URL, port). Graceful shutdown that drains in-flight requests. Keep `/healthz` dependency-free (no DB call — it answers even when Postgres is down).

---

### Epic E2 — Database schema
**Dependencies:** E1 · **Priority:** P0 · **Parallel:** with E3, E4

**Specify prompt:**
> Design and migrate the Hireling database: a dedicated Postgres hall on Asgard (database `hireling`, role `hireling`, connection limit 20) reached only over the `asgard-net` bridge, with sqlx migrations checked into the repo. Schema covers parties, characters (Pathbuilder import payload + derived state), active effects, inventory, the P1 stash/bank tables, rules-corpus rows with `source: core|imported|custom` lanes, and per-field monotonic version bookkeeping for offline reconciliation. Multi-party from day one: `party_id` on everything relevant — the second campaign must be config, not migration. Local dev uses the compose throwaway Postgres, never Asgard.

**Constraints (settled):**
> Postgres 16 on Asgard (house instance). sqlx migrate. The reference Pathbuilder export (`#pbExport` JSON block in docs/reference/lorum_ipsum_dashboard.html) is the shape of imported character data. Character identity anchor: Authentik account → character.

**AI Guardrails:**
> Migrations reversible (up/down). FK constraints + indexes on every party_id/character_id join path. created_at/updated_at on all entities. Decide soft- vs hard-delete once and apply consistently (claim history is append-only regardless). Concurrent writes resolved by server-receipt order — schema must support per-field versions without locking UI flows.

---

### Epic E3 — Authentik OIDC auth
**Dependencies:** E1 · **Priority:** P0 · **Parallel:** with E2, E4

**Specify prompt:**
> Build authentication: Authentik OIDC login for six pre-provisioned accounts (Josh, Bear, Dave, Becky, Jake, Bruce). No self-serve signup, no password handling in the app. The session identifies the account everywhere; every write path enforces Hireling's ownership semantics server-side: a character's owner is its sole writer, the creator of an effect is its sole writer (even on other sheets), reads are unrestricted within the party, and the GM account writes nothing anywhere — the server rejects GM writes even if the UI never offers them.

**Constraints (settled):**
> Existing house Authentik instance (OIDC only). Ownership claimed at import: the importing account owns the character. One character per account at POC; reassignment is a DB operation, not UI.

**AI Guardrails:**
> Authorization check on EVERY endpoint (not just the ones features mention) — deny by default. Session timeout + invalidation. Audit-log logins and ownership-relevant rejections. Consistent 403 payload shape.

---

### Epic E4 — Rules corpus importer
**Dependencies:** E1 · **Priority:** P0 · **Parallel:** with E2, E3

**Specify prompt:**
> Build the rules-corpus importer: a one-time ETL from the Foundry VTT pf2e system packs (foundryvtt/pf2e on GitHub, JSON) into the corpus tables. POC scope imports CONDITIONS and ITEMS only; spells/feats/bestiary are importer-capable but deferred (no consumer). Every row lands in a source lane (`core|imported|custom`); importer re-runs are idempotent and never touch `custom` rows. Conditions the engine's stat vocabulary cannot express import as display-only rows (badged tracked-manually downstream) — the importer never fabricates modifier rows for them. Includes the license gate: ORC/OGL notice file in-repo, the upstream pack license text archived, and a green/red verdict that gates anything public (POC deploys private regardless).

**Constraints (settled):**
> No runtime dependency on AoN (no API, CORS-fragile) — AoN is for citation links only. Pack source is pinned by release tag; schema drift between Foundry releases is handled by importer versioning, not runtime scraping.

**AI Guardrails:**
> Pin the pack release; record import date + pack version per row (data-freshness indicator). Retry with backoff on fetch failures; a partial import must never half-write a pack (transactional per pack). Log per-table counts per run; a run that imports 0 conditions when conditions existed before is an error, not a no-op.

---

## Phase 1: Core (P0)

### Epic E5 — Pathbuilder import pipeline
**Dependencies:** E2, E3 · **Priority:** P0 · **Lane:** A · **Blocks:** E6

**Specify prompt:**
> Build the Pathbuilder 2e import pipeline: paste or upload a Pathbuilder JSON export; the full character derives from it and joins the party roster, owned by the importing account. The import contract is the reference export embedded in docs/reference/lorum_ipsum_dashboard.html (the `#pbExport` JSON block) — that shape is the spec. Three failure classes with human-readable messages: invalid JSON; valid JSON that isn't a Pathbuilder export (missing top-level keys); unknown fields (logged, import continues). Robustness comes from class (c), not a version pin — the export carries no version. Re-import replaces the base sheet while preserving live state (anchoring: HP/temp-HP by character identity, slot usage and prep by rank+index, inventory deltas by item name); unmatched entities are kept and surfaced in a post-import diff, never dropped. Class features that reshape slot layouts (Staff Nexus, school spells, flexible spellcasting) are handled per-feature as real exports surface them — every quirk lands as an importer test case.

**Constraints (settled):**
> The export schema is unofficial and can drift; worst case is a manual re-export, never data loss. Active effects live server-side and are untouched by re-import.

**AI Guardrails:**
> Validate + sanitize the JSON server-side (size cap, depth cap). Re-import must be idempotent per the anchoring rules. Every failure-class message must be human-readable and tested. Log import attempts with account + outcome.

---

### Epic E6 — Live sheet UI (desktop)
**Dependencies:** E5 · **Priority:** P0 · **Lane:** A · **Blocks:** E9, E10

**Specify prompt:**
> Build the desktop live sheet: the player's full character — HP/temp-HP (clamped 0–max, temp absorbs first as a distinct bar segment), AC, saves, Perception, skills with proficiency ranks, strikes/actions, spells (slots, focus, innate, cantrips) with prep and expenditure tracking, feats, inventory with containers (extradimensional contents excluded from Bulk), and the companions/minions panel. Include the manual level adjust control in the sheet header (bounds 1–20, confirm on level-down, all derived stats visibly re-derive — math rescale only; boosts/feats still come from Pathbuilder re-export). The visual and interaction baseline is Dave's frozen prototype (docs/reference/lorum_ipsum_dashboard.html): dark fantasy, Palatino headers, gold accents, three-column desktop layout. All live state persists server-side (survives refresh, reinstall, device switch). POC build target is DESKTOP ONLY — the phone/tablet layout is P2 and out of scope here.

**Constraints (settled):**
> Svelte via Vite, no SvelteKit — static bundle served by the axum binary. Hand-rolled CSS following the prototype's design language, no component library. Local state mirrors server state optimistically; the server is authoritative.

**AI Guardrails:**
> Loading/empty/error states for every async surface (a first-run sheet is EMPTY until import — design that state deliberately). Keyboard accessibility for interactive elements. Optimistic updates with rollback on rejection. Client-side validation before writes (HP bounds).

---

### Epic E7 — Party sync & realtime
**Dependencies:** E2, E3 · **Priority:** P0 · **Lane:** B · **Blocks:** E8, E10

**Specify prompt:**
> Build party sync: one WebSocket per party; any state change propagates to all connected members as server-broadcast diffs, hitting p95 broadcast dispatch under 1s on home wifi and under 3s on cellular (define and instrument the exact interval — e.g. API receipt → broadcast dispatch — during the spec; the PRD deliberately left this to the build stage). Offline tolerance: the PWA caches last-known state read-only; offline writes queue and reconcile on reconnect by server-receipt order — the server assigns a monotonic version per field (granularity: HP, temp-HP, each spell slot, each inventory quantity, each effect whole; extend to stash/bank fields when FG4 builds). A queued write that loses is silently superseded and the view re-renders to synced state — a subtle "syncing…" indicator while the queue is non-empty, no error theatre. Backend unreachable = same degraded mode: serve last-known state read-only, queue writes.

**Constraints (settled):**
> Client clocks are untrusted; server-receipt order always wins. Two devices, one owner: no locking UI — versioning handles it.

**AI Guardrails:**
> WebSocket reconnect is silent and automatic with backoff; queued writes are idempotent (safe to replay). Superseded writes are logged server-side even though the UI is silent. Broadcast failure for one client must not block others.

---

### Epic E8 — Buff/effect engine ⚠️ sync point
**Dependencies:** E4, E5, E7 · **Priority:** P0 · **Blocks:** E10, E13

**Specify prompt:**
> Build the buff/effect engine — the product's heart. Effect model `{ name, source_character, targets[], modifiers[], duration_note, active }`; targets are roster characters only. A modifier is `{ type, stat, value }` with type `circumstance|status|item|untyped` and a closed stat vocabulary: single stats (ac, fort, ref, will, perception, speed, attack, damage, spell_attack, spell_dc, class_dc, skill:<name>) plus blanket targets with rules-exact expansion sets (all_checks = every d20 roll, NOT damage/speed; all_dcs = ac + spell_dc + class_dc; all_checks_and_dcs = the union — frightened's footprint). Stacking math: highest bonus applies once per type, penalties take the worst per type, untyped stacks fully; blanket targets expand before evaluation; every derived stat on every affected sheet recomputes on any effect change. Every derived number renders its provenance on hover, including suppressed sources (`+1 status (Bless) — Inspire Courage +1 also active, not stacked`). Manual lifecycle only: the creator adds/removes targets and ends effects; nothing auto-expires; duration is a displayed note. The caster sees all their active effects and current targets; each sheet shows effects affecting it with sources. Valued conditions seed the picker from the imported corpus — which conditions carry engine math vs tracked-manually badges is corpus data from the importer, not hardcoded. Detrimental conditions ride the same mechanism with negative values.

**Constraints (settled):**
> The engine recomputes; it never stores derived numbers as editable state. Keep the engine module clean of UI and transport — a future RPGMastermind harvest must be a port, not a rewrite (explicit business goal). Engine bugs are stop-the-line (Constitution Article IV in repo house rules).

**AI Guardrails:**
> Exhaustive unit tests against the Player Core's worked examples for every rule the stacking math encodes (this is the PRD's stated mitigation for the modifier-engine risk). Property-based tests for expansion sets and stacking permutations. Zero special-cases for named conditions in engine code — everything flows through the vocabulary.

---

### Epic E9 — Rules tooltips + custom content entry
**Dependencies:** E4, E6 · **Priority:** P0 · **Lane:** A

**Specify prompt:**
> Build rules tooltips and the custom content lane. Tooltips: conditions and game terms render as hoverable/pinnable popups with paraphrased rules text and Archives of Nethys links; POC covers every Player Core condition; the prose is a curated layer seeded from Dave's frozen prototype (its 42-condition map — paraphrased, AoN-linked, page-cited), structure comes from the corpus import. Custom entry: "Add custom" affordances at the point of use — item context (inventory), the caster's spell composer, and the condition picker — opening a minimal-fields form (name, level/value where applicable, one-line description) that creates a `custom`-lane row surfacing immediately where created and joining the relevant picker party-wide, badged `custom`. Custom rows are creator-owned (any character owner may create; the creator is the row's sole writer; Dave is curation lead, not gate) and are display/tracking entries — homebrew engine math goes through the freeform effect composer.

**Constraints (settled):**
> Paraphrased rules text ships under the Paizo Community Use Policy / ORC notice in-repo (notice file included in this epic). The frozen prototype is display-prose seed only — never a structured data source.

**AI Guardrails:**
> All prose renders through an inert HTML setter (the prototype's setHTML discipline — DOMParser, no script execution). Input validation + length caps on custom forms. Tooltip content is read-only for non-creators at P0 (curation editing is E15).

---

### Epic E10 — Party view, GM seat, PWA
**Dependencies:** E6, E7, E8 · **Priority:** P0 · **Lane:** B

**Specify prompt:**
> Build the party screen and the installable PWA. Party screen (all accounts): roster of character cards — name, portrait initial, HP bar with down/max state, active effect chips with sources; tapping a card opens that character's full sheet, read-only unless you're the owner (cross-member viewing: ownership gates writes, nothing gates reads). The GM account lands here and never sees an edit affordance, server-enforced (E3) as well as UI-hidden. PWA: installable on desktop (and wherever the browser allows) — manifest, icon, splash, standalone display, service-worker cache per E7's offline contract. First-run state: empty party screen with the "Import your character" CTA.

**Constraints (settled):**
> Standard service worker + manifest (no PWA framework). Desktop layout only at POC.

**AI Guardrails:**
> Reuse E6's sheet components for card drill-ins — do not re-implement. Service-worker cache versioning/invalidation strategy explicit (stale sheets at the table are the product failing, silently). Effect chips must render from the same engine state as sheet chips — one source of truth.

---

## Phase 2: Go-Live

### Epic E11 — Go-live deploy
**Dependencies:** E6–E10 · **Priority:** P0

**Specify prompt:**
> Deploy Hireling to the lab: a stateless container on Mimir (existing Docker host, rebuild-from-git), behind the existing cloudflared tunnel pattern, serving https://hireling.flinntech.com. The Asgard `hireling` hall is the only persistent state and rides Asgard's existing backup rotation — verify a restore once before first session. Heimdall monitors uptime against /healthz. Session-night readiness (Mimir, Asgard, tunnel) is a P0 operational dependency owned by Josh; this epic ships the readiness check as a documented procedure.

**Constraints (settled):**
> Zero new infrastructure — existing Mimir, Cloudflare, Authentik, Asgard assets only. Private deployment: no public exposure, so E4's license gate stays yellow-green (gate matters only for public features later).

**AI Guardrails:**
> Container healthcheck wired to /healthz; graceful shutdown drains WebSocket connections. Structured logs shipped wherever Mimir containers log today. One verified backup-restore rehearsal before the first game session — an unverified backup is a hope, not a backup.

---

## Phase 3: Secondary (P1)

### Epic E12 — Shared inventory (FG4)
**Dependencies:** E10, E4 · **Priority:** P1 (cuttable table-convenience)

**Specify prompt:**
> Build the shared stash: a party loot list (item, quantity, Bulk, notes) with live transfers between characters and the stash; an append-only claim history `{item, quantity, from, to, actor, timestamp}`; quartermaster-only sell (sale value prompt defaulting to book value — resolved from the Foundry items corpus by case-insensitive exact-name match, unmatched items flagged "no book value" with manual entry); proceeds land in the party bank (gp/sp/cp ledger, quartermaster-only manual adjustments, actor-logged). The quartermaster toggle lives in party settings, flippable by the designated character's owner; party settings are writable by any character owner. Reconciliation granularity for stash/bank fields joins E7's per-field versioning — define it in this epic's spec.

**AI Guardrails:**
> Money and item mutations are append-logged before state changes (argument-settling is the feature). Sell/bank operations idempotent-safe against replayed offline writes.

---

### Epic E13 — Seeded spell library + conflict pre-warn (FG3 P1)
**Dependencies:** E8, E9 · **Priority:** P1 · **Human dependencies:** Josh (seed list config), Dave (templates)

**Specify prompt:**
> Build the seeded spell library: the party's commonly-cast top ~20 spells (definitive list = importer seed config owned by Josh, chosen from observed table usage) carry structured outcome templates authored by Dave in the prototype's paraphrase style, reviewed by Josh — never model-generated. Casting a library spell offers its degrees of success (crit success/success/failure/crit failure); the caster taps what happened; defined effects apply to chosen targets automatically — no manual modifier definition. Library covers party-targeted effects only; enemy effects stay player-managed (non-goal). Spells outside the library stay freeform exactly as P0. Plus conflict pre-warning: the composer's target picker flags stacking conflicts before assignment ("Becky: +1 status active — Bless would be suppressed") so the caster decides with the math done.

**AI Guardrails:**
> Template data validates against the engine's vocabulary at seed time (a template referencing an invalid stat fails loud at config load, not at the table). Conflict warnings computed from live engine state, not cached chips.

---

### Epic E14 — GM stat density (FG5 P1)
**Dependencies:** E10 · **Priority:** P1

**Specify prompt:**
> Extend the party view's per-character cards into full glanceable stat blocks, all read-only: current/max HP, AC, saves, Perception, spell/class DCs, initiative modifier, and the character's three highest skill modifiers ("key skills", auto-selected — define the selection set and tie-break in this epic's spec; the PRD deferred it here deliberately). Initiative ORDER and turn tracking remain non-goals — modifier display is a stat, not tracking.

**AI Guardrails:**
> Stat blocks render from the same engine-recomputed values as sheets — no second computation path.

---

### Epic E15 — Curation editing (FG1 P1)
**Dependencies:** E9 · **Priority:** P1 · **Dave-owned content lane**

**Specify prompt:**
> Build prose curation on imported rows: editing the display prose (paraphrase, AoN link, page cite) of corpus-imported conditions/items in-app, in the prototype's authorship style. Edits are curation-layer only — structured rules data stays importer-owned and is never editable here. Authorship/review flow per the PRD: Dave authors, Josh reviews before changes ship to the party.

**AI Guardrails:**
> Edited prose versions tracked (who/when) — curation is auditable. Rendering keeps the inert-setter discipline from E9.

---

## Phase 4: Future (P2 — placeholders, not built)

- **Phone/tablet layout** — dedicated one-handed design; floor = prototype's 780px single-column reflow; design pass reviewed by Dave. (Moved from P0 2026-09-18.)
- **Dice roller** — public-release feature; table rolls physical dice.
- **Multi-party UI** — schema is ready (E2); Dave's second campaign forces it.
- **GM encounter view** — only if Bruce asks; zero-GM-work default stands.
