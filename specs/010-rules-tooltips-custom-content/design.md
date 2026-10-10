# Design: Rules Tooltips + Custom Content Entry (E9)

**Status**: signed by Korrin (PM seat) 2026-10-20 — decisions internal to
E9's P0 scope; nothing here moves the P0/P1/P2 line (Q2's composer gap is
escalated, not decided). Input chain: `spec.md` header.

The shape in one paragraph: curated prose ships as a build-time static
module; a shared inert HTML setter renders it; a `Tooltip` primitive
(e9-built, listed absent in E6's components contract) mounts on every
condition-name surface; custom rows write through two party-scoped REST
routes into the existing `corpus_entries` table (zero migrations); the
condition picker surface is new UI over E8's existing REST + write
frames; NOTICE.md gains a lane and an about view renders it.

## D1 — Curated prose is a build-time static module, not server data

`web/src/lib/rules/condition-prose.json` — the prototype's 42-entry map
ported verbatim (`{ "<lowercase name>": { "page": N, "aonId": N, "text":
"…", "link": false? } }`), imported by the bundle. Join: case-insensitive
exact match against corpus/effect names, client-side.

- **Why**: tooltips must work offline at the table (FR-10, E7/E10
  contract — the sheet stays fully readable; US-3 tooltips are part of
  the readable sheet). A static module is SW-cached with the shell by
  construction, needs no route, no cache invalidation, and no fallback
  path when the socket is down. Curation at P0 is by PR — the same
  human-reviewed-checked-in-data pattern as E4's
  `data/seed/condition-tiers.json` (its `_readme` is the precedent
  text).
- **Rejected — server-joined prose on the picker REST** (`description`,
  `page`, `aon_id` fields added to `GET /conditions`): one source for
  corpus-ish data, and E15-friendly — but it makes every offline tooltip
  a miss and couples the tooltip's render path to a fetch's success.
  E15 (curation editing, auditable versions) will move prose to DB state
  *then* — that migration is E15's spec problem, and moving from a
  checked-in seed to a DB row is a smaller step than moving from a
  served-join to an editable one.
- **Rejected — prose columns on `corpus_entries`**: E4's spec explicitly
  reserved prose as "E9/E15's layer" and the importer rewrites `data`
  from upstream verbatim — a prose field inside the import's blast
  radius is a clobber bug waiting to happen.

## D2 — Custom rows write via party-scoped REST, not WS frames

`POST /api/parties/{party_id}/custom` and
`PATCH /api/parties/{party_id}/custom/{corpus_entry_id}` (contract:
`contracts/custom-rows-rest.md`). Session-gated; authz = character owner
in that party (create) / row creator (edit); GM read-only.

- **Why**: the WS protocol is per-field CAS sync over character-owned
  rows (`base_version`, `op_id`, snapshot/diff). A corpus row is none of
  that — it's an identity-keyed entity whose write gate is ownership,
  not version racing. REST matches E3's existing declarative matrix rows
  and keeps the wire protocol untouched (a wire change would ripple
  through E7's contract and E10's SW for zero P0 benefit).
- **Rejected — a `custom_new` frame on the wire**: consistent with
  `effect_new`, but it drags snapshot semantics, fan-out, and the
  degraded-mode queue into a write that happens a handful of times per
  campaign.

## D3 — Visibility: creator-local immediate; party-wide on next fetch

The create response returns the row. The creating client renders it
immediately in the surface it was created from (optimistic insert for
the inventory row; store insert for spell/condition lists). Other
members' clients pick custom rows up when they next fetch: picker open,
sheet boot, composer open — all already on-demand fetches.

- **Why**: pickers and composers are open-on-demand surfaces; the
  corpus query already includes custom rows (no lane filter), so
  "joins the relevant picker party-wide" is true of the data the moment
  the row commits — what's deferred is other clients' *display* until
  they look. At POC scale (one party, six users, session nights) that is
  the honest 80/20 (Constitution Art. II).
- **Rejected — broadcast a custom-row diff on create**: live push for a
  list nobody has open; costs a wire-contract amendment and fan-out
  work in the sync core.
- **Consequence accepted**: a member with a picker dialog *already open*
  won't see a just-created custom row until they reopen it. Known,
  documented, POC-tolerable; revisited if the table complains.

## D4 — E9 builds the condition picker surface; it does NOT build the freeform composer

FR-7 ships a picker dialog over `GET /api/parties/{id}/conditions`:
searchable list, tier/lane/valued badges, value input for valued
conditions, apply via the existing effect-create write
(`corpus_entry_id` + `condition_value`), plus the "Add custom condition"
affordance. Owner-gated.

- **Why**: gh#11 names the picker as a point of use for "Add custom"
  and for tooltip triggers; the picker UI exists nowhere in the tree
  (E8's spec was API-first — its stories apply conditions "via the
  API"; E10 renders chips only). Three of this epic's requirements have
  no home without it.
- **The freeform effect composer (PRD FG3 Step 3, P0 element) is also
  unbuilt** — Bear's "new effect → +1 status to attack → targets"
  flow has no UI today, only the wire/API support. That is an E8
  residual, not E9 scope: absorbing it would fold a second epic's UI
  surface (modifier picker over the stat vocabulary, target picker,
  duration note) into the one epic that explicitly routes homebrew math
  *around* itself (FR-6). Escalated to Thrane with the sizing read —
  options: fold into E9's plan as added tasks (≈ +1–2 heats), or a
  standalone backfill task ahead of E13. Korrin's recommendation:
  standalone task — it is E8-shaped (engine seam, wire frames) and E13
  builds directly on it. **Answered (post-escalation): standalone —
  filed as gh#74, sequenced ahead of E13 (gh#15); does not block E9.**
- **Rejected — spec E9 around a picker that might exist**: an affordance
  pointed at a surface nobody ships is a spec bug, not scope discipline.

## D5 — Custom rows ride existing machinery; zero migrations

- Rows land in `corpus_entries` exactly as E2 built it: `kind`
  (`condition` | `item` | `spell`), `lane='custom'`, `created_by_sub` =
  session account, `modifiers = NULL`, `source_id = NULL` (⇒ importer
  re-runs structurally cannot touch them, FR-8). `data` carries
  `{ "custom": { "description": str, "value_or_rank": int? } }` — no
  `import` block (the picker's NULL-tier default yields
  `display_only`; NULL modifiers yield `valued: false`; both are E8's
  existing read logic, unchanged — spec Clarify Q8).
- **Applying a custom condition**: existing `effect_new` frame with
  `corpus_entry_id` set; E8's apply already resolves NULL modifiers to
  `tracked_manually=true`, zero math (`src/engine_host/apply.rs`). No
  new apply code. FR-6 by construction.
- **Preparing a custom spell**: existing slot write with
  `prepared: "<name>"`. The composer lists custom rows from the custom
  fetch; the write is unchanged.
- **Custom item**: create inserts the corpus row AND one
  `character_inventory_live` row for the creating character in
  the same transaction — **`qty_delta = 1` explicitly, not the column
  default 0**: a custom item has no anchor base (base 0), and quantity
  renders as `base_qty + delta`, so the default would render qty 0 and
  fail US-3 AC-1. Qty changes thereafter are ordinary `inv`
  writes by exact name.
- **Audit**: `forbidden_custom_write` already exists in the enum — no
  migration. Successful creates are not audit events (the table's
  charter is "logins and ownership-relevant rejections").
- **Why zero-migration matters**: E9 was sequenced against E4+E6; every
  schema touch re-opens E2's ownership law for no gain. The schema
  already anticipates this epic (`created_by_sub`'s comment cites it).

## D6 — Tooltip triggering: name-match, corpus-enriched; one primitive

A single `ConditionTip` wrapper component + `inert-html.js` module
(contracts: `contracts/inert-html.md`). Triggers: sheet `EffectsStrip`
chips, roster chips (same component — E10 reuse), picker rows,
provenance breakdown entries, and linkified names inside tooltip prose
(second layer). Matching is case-insensitive exact name against the
prose map; where the context carries a corpus link
(`corpus_entry_id`/lane), badges render (tier, `custom`); custom rows'
descriptions render as the prose body.

- **Why name-match**: chips and provenance rows carry effect *names*;
  freeform effects named "Frightened" deserve the tooltip exactly as
  corpus-applied ones do — that is the prototype's discipline
  (`linkConds` matches text, not ids). The corpus link enriches what it
  can; it is not the gate.
- **Rejected — corpus_entry_id-keyed tooltips only**: freeform-named
  conditions go dark; provenance rows and nested prose links (which
  carry only names) would need a separate lookup path anyway.

## D7 — Inert rendering: one shared module (E6's), extended in place, boundary-checked

`web/src/lib/util/inert-html.js` **already ships** — E6 built it
(`parseInert`/`scrub`/`adoptHTML`, 7 unit tests;
`specs/006-live-sheet-ui/spec.md` names E6 the shipper and E9 the
consumer). **`adoptHTML(el, html)` is the setter** — the prototype's
`setHTML` under its shipped name. E9 does not create a second setter
(a second setter is a second chance to get it wrong — the exact failure
this decision exists to prevent); it **extends the module in place**:
`scrub`'s discard list grows to the contract's network-bearing set
(`iframe`, `object`, `embed`, `link`, `meta`, `style` beyond `script`),
the attribute filter upgrades from `javascript:`-dropping to
https-or-fragment-only (kills `data:` and protocol-relative), and two
new exports land — `esc` and `linkifyConditions` (the prototype's
`linkConds`: text-node-only linkification, tag segments passed through
untouched). All prose — curated, custom descriptions, NOTICE — renders
through it; Svelte `{@html}` is banned for prose (the boundary check
gains this rule; see plan Task 1).

- **Why one module**: the guardrail is a property of the *path*, not of
  each call site — a second setter is a second chance to get it wrong.
- **Reachability**: the shipped module has **zero production callers**
  today — by AGENTS.md's reachability rule E6's requirement is unbuilt
  until E9 mounts `ConditionTip`/`AboutView` on it. E9 is the epic that
  closes that.
- **Rejected — sanitize-then-`{@html}`** (DOMPurify-style allowlist):
  a new dependency (banned) or a hand-rolled allowlist (more code than
  DOMParser + text-only construction) for the same guarantee.

## D8 — Licensing: NOTICE lane + about view

`NOTICE.md` gains the curated-prose lane: paraphrased condition text
seeded from the frozen prototype ships under the Paizo Community Use
Policy, consistent with the archived ORC/OGL lanes; the seed file's
provenance (prototype merge `cb0f397`) is cited. The in-app about view
renders `NOTICE.md` through the inert setter (packaged into the bundle
at build time like the prose seed — same offline story). This closes
the PRD's green-gate "in-app about/license view" item, which E4's
shipped scope (file + archive + verdict) left open.

## D9 — Settled interpretations (recorded with their rejected twins)

1. "Caster's spell composer" = spellbook/prepare surface (spec Clarify
   Q1). *Rejected*: the FG3 effect-composer reading — custom rows carry
   no math (FR-6), so an effect-composer entry point for them is
   incoherent; EPICS.md's wording is shorthand.
2. Fallback prose for un-frozen corpus conditions: minimal tooltip, no
   invention. *Rejected*: render upstream pack `description.value` —
   license-fine but scope-heavy (would ship pack text to the client for
   a case the PRD explicitly defers), and it would put un-curated text
   in a tooltip whose promise is "paraphrased".
3. Custom-row deletion: absent (non-goal). *Rejected*: soft-delete flag
   — live references (prepared names, inventory rows, applied effects)
   key by name/id; dangling badges are worse than clutter.

## Component plan (E6 contract additions — additive)

| Unit | New/Changed | Notes |
|---|---|---|
| `util/inert-html.js` | **changed (extend)** | E6 shipped `parseInert`/`scrub`/`adoptHTML` (7 tests, zero callers); E9 extends scrub + attrs (contract §1) and adds `esc`/linkify (contract §2) |
| `rules/condition-prose.json` | new | 42-entry seed, build-time import |
| `rules/prose.js` | new | lookup: name → entry (ci-exact), fallback shape |
| `ConditionTip` | new | hover/focus + pin + second layer; renders badges, cite, AoN, custom description |
| `EffectsStrip` | changed (additive) | chips become `ConditionTip` triggers (no prop break; E10 unaffected) |
| `Provenance` | changed (additive) | breakdown entries become triggers |
| `ConditionPicker` | new | FR-7 dialog; fetches E8 REST; apply via existing write path; hosts "Add custom condition" |
| `AddCustomForm` | new | shared minimal-fields form (kind-parameterized), caps + inline errors |
| `InventoryPanel` | changed (additive) | `onaddcustom` callback + affordance in the item context (owner-gated by `editable`) |
| `CasterPanel` | changed (additive) | custom-spell section in spellbook/prepare (badged), `onaddcustom` affordance |
| `AboutView` | new | NOTICE.md rendered through the setter |

`editable` discipline unchanged: every new affordance renders only when
`editable` (owner) — GM and cross-member views see none of it (E6
components contract).

## Risks

- **Name-join drift** (upstream rename orphans prose): coverage test
  makes it a CI failure, not a silent empty tooltip.
- **Second-layer state complexity** (pin + swap + Esc): bounded to one
  layer by design (spec edge); prototype's exact behavior is the
  baseline — port, don't redesign.
- **The composer gap (Q2) lands late**: escalated with the sizing read;
  E9's plan does not depend on it (custom rows route display/tracking,
  not math).
