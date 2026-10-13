# Contract: Inert HTML Rendering (E9)

**Status**: binding for E9 implementation · the security boundary for all
prose · extends the prototype's `setHTML` discipline (frozen reference
`docs/reference/lorum_ipsum_dashboard.html`, merge `cb0f397`).

Every string that reaches the DOM as anything other than plain text —
curated condition prose, custom-row descriptions, the license notice —
passes through this module. Nothing else may construct prose markup.

**The module already ships (E6).** `web/src/lib/util/inert-html.js` —
E6's spec (`specs/006-live-sheet-ui/spec.md`, tooltip-infrastructure row)
names E6 the shipper and E9 the consumer. It exports `parseInert`,
`scrub`, `adoptHTML`, with 7 unit tests (`web/tests/util/inert-html.test.js`).
It has **zero production callers** today — by AGENTS.md's reachability
rule that requirement is unbuilt, and E9 mounting `ConditionTip` /
`AboutView` on it is what builds it. E9 **modifies** the module; it does
not create a second one.

## 1. The setter

**`adoptHTML(el, html)` is the setter** — the prototype's `setHTML` under
its shipped name. E9 does not rename it and does not add a `setInertHTML`
wrapper: one setter, the module's existing export.

1. `DOMParser().parseFromString(html, "text/html")` — shipped, unchanged
2. scrub, then `el.replaceChildren(...nodes)` — shipped shape, unchanged

E9's extensions to the scrub (strictly stronger than what ships —
the shipped `scrub` discards `script` only, and the shipped attribute
filter drops `javascript:` only):

- `scrub`'s discard list grows to every network-bearing element type:
  `script`, `iframe`, `object`, `embed`, `link`, `meta`, `style`.
- The shipped attribute filter drops only `javascript:` URLs; E9
  upgrades it to the https-or-fragment guarantee below (also kills
  `data:` and protocol-relative forms).

Guarantees (test-enforced, §4):

- `<script>` elements never execute — DOMParser inserts nothing, and the
  parsed nodes are moved without evaluation.
- Event-handler attributes (`onerror`, `onclick`, …) do not survive into
  the live tree: the setter **strips them** from every element it moves
  (defense beyond DOMParser, because `replaceChildren` alone would carry
  such attributes into the document if a future engine changes parse
  semantics; the strip makes the invariant ours, not the platform's).
- URL-bearing attributes (`href`) survive only with `https:` (or
  fragment) schemes; `javascript:`, `data:`, and relative-protocol forms
  are dropped.
- No network-bearing element types are moved at all (the extended
  discard list above).

## 2. Escaping and linkification — new exports

Both are additions to the shipped module (it exports neither today):

- `esc(s)` — `& < > "` to entities, as the prototype. Used on every
  dynamic string *before* it enters a template (names, descriptions,
  cites, URLs).
- `linkifyConditions(text, { skip })` — port of `linkConds`:
  - Splits on tag segments (`/^(<[^>]+>)/`); **only text segments are
    examined** — tags and attributes are passed through untouched by
    construction.
  - Wraps condition-name matches in `<a data-cond="name">…</a>` —
    plain anchors, no href (the tip layer handles activation);
    longest-match-first ordering, the prototype's regex discipline,
    word-boundary anchored.
  - `link: false` entries in the seed (objects-only / meta conditions)
    are excluded from matching — the prototype's `nl` flag.
  - The `skip` name (the popup's own subject) is not self-linked.

## 3. Svelte discipline

- `{@html …}` is **banned for prose** repo-wide; the boundary check
  enforces it (plan Task 1): prose must route through `adoptHTML`
  inside `ConditionTip`/`AboutView`.
- Plain-text interpolation of names/descriptions (no markup expected)
  stays ordinary Svelte interpolation — the ban is on markup-carrying
  strings only.

## 4. The hostile-fixture matrix

The 7 shipped tests already pin: parse-through-DOMParser, `script`
discard (including nested), `on*`/`javascript:` attribute strip, host
clear + `replaceChildren` adoption, and the no-DOMParser error path.
**E9 extends the suite with only what those do not cover** — do not
re-test what ships:

| Fixture | Asserted | Why new |
|---|---|---|
| fixtures containing each of `iframe`, `object`, `embed`, `link`, `meta`, `style` | none of the discard set survives into `el`; surrounding text does | shipped `scrub` drops `script` only |
| a `data:` href, a protocol-relative href (`//evil.example/x`), a plain relative href | dropped; `https://…` and `#frag` hrefs survive | shipped filter drops `javascript:` only |
| `esc` outputs on inputs carrying `& < "` | entity-escaped, every time | new export |
| `"Frightened <b>Off-Guard</b> applies"` (+ `skip` / `link:false` variants) | condition names linkified in *text* segments only; the `<b>` tag and its attributes pass through untouched; the `skip` name is not self-linked; `link:false` entries never match | new export (`linkifyConditions`) |
| the same hostile prose through a mounted `ConditionTip` | same assertions against the rendered DOM | production-path rule: the unit suite proves the module, not the wiring |

## 5. Sources and their trust

| Source | Trust | Treatment |
|---|---|---|
| Curated seed (`condition-prose.json`) | repo data (PR-reviewed) | linkified, rendered inert |
| Custom-row description | party user input (capped at 280 chars server-side) | `esc`ed — **not** linkified, **not** markup — rendered as text with the setter |
| `NOTICE.md` at build time | repo data | rendered inert, not linkified |
| Effect/condition names | mixed (freeform names are user input) | `esc`ed as text everywhere |

The distinction that matters: curated prose may carry `<b>`/`<i>`/anchors
(because a PR reviewed it) and is linkified; user descriptions render as
plain text only. No user-input string is ever linkified or allowed to
carry markup.
