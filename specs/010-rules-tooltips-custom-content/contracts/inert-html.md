# Contract: Inert HTML Rendering (E9)

**Status**: binding for E9 implementation · the security boundary for all
prose · extends the prototype's `setHTML` discipline (frozen reference
`docs/reference/lorum_ipsum_dashboard.html`, merge `cb0f397`).

Every string that reaches the DOM as anything other than plain text —
curated condition prose, custom-row descriptions, the license notice —
passes through this module. Nothing else may construct prose markup.

## 1. The setter

`setInertHTML(el, html)` — port of the prototype's `setHTML`:

1. `DOMParser().parseFromString(html, "text/html")`
2. `el.replaceChildren(...parsed.body.childNodes)`

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
- No network-bearing element types are moved at all: `script`, `iframe`,
  `object`, `embed`, `link`, `meta`, `style` are discarded.

## 2. Escaping and linkification

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
  enforces it (plan Task 1): prose must route through `setInertHTML`
  inside `ConditionTip`/`AboutView`.
- Plain-text interpolation of names/descriptions (no markup expected)
  stays ordinary Svelte interpolation — the ban is on markup-carrying
  strings only.

## 4. The hostile-fixture matrix (unit + production-path)

| Fixture | Asserted |
|---|---|
| `"<script>window.__pwn=1</script>text"` | no script node in `el`; `window.__pwn` undefined; `text` present |
| `"<img src=x onerror=window.__pwn=2>"` | img may render (broken), `onerror` attribute absent, handler never fires |
| `"<a href=\"javascript:window.__pwn=3\">x</a>"` | anchor present, href dropped or neutralized to fragment |
| `"Frightened <b>Off-Guard</b> applies"` | `Off-Guard` and `Frightened` linkified in *text* segments; the `<b>` tag passes through unchanged; attributes inside tags are never rewritten |
| through mounted `ConditionTip` with the same prose | same assertions against the rendered DOM (the component is the production path; the unit suite alone proves the module, not the wiring) |

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
