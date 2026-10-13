// The one inert-HTML primitive (E6 spec §7): every prose/HTML injection in
// the sheet goes through here, porting the prototype's `setHTML` discipline
// (docs/reference/lorum_ipsum_dashboard.html line 966).
//
// The platform property this relies on is the HTML spec's own: a document
// parsed by DOMParser is *inert* — its scripts do not run — and nodes
// adopted out of it do not suddenly become executable. On top of that
// platform guarantee this module adds a defensive scrub before adoption:
// network-bearing elements are dropped and `on*`/non-https URL attributes
// are stripped, so a future caller that bypasses the parser (or a platform
// change) still cannot arm a payload through this util. No component may
// innerHTML-assign; there is exactly one shared primitive.
//
// esc + linkifyConditions live here too (contracts/inert-html.md — binding):
// the scrub's discard list is the network-bearing element set and the URL
// filter is https-or-fragment-only. Svelte `{@html}` is banned for prose
// repo-wide — the boundary check (web/scripts/check-html-boundary.mjs)
// enforces it.

/**
 * A parsed, inert node: the util only touches `nodeName`, `attributes`,
 * `childNodes`, and the append target's DOM methods, so tests can drive it
 * with minimal doubles. Real DOMParser output works too: `attributes` may
 * be a NamedNodeMap and `childNodes` a NodeList (array-likes), and live
 * nodes may carry `removeAttribute`/`removeChild` — the scrub normalizes
 * both shapes.
 * @typedef {Object} InertNode
 * @property {string} nodeName
 * @property {InertNode[]} [childNodes]
 * @property {Array<{name: string, value: string}> | ArrayLike<{name: string, value: string}>} [attributes]
 * @property {(name: string) => void} [removeAttribute]
 * @property {(child: InertNode) => InertNode} [removeChild]
 * @property {() => void} [remove]
 */

/**
 * Parse `html` inertly and return the top-level nodes of its body.
 *
 * In a browser this is DOMParser (the prototype's discipline, verbatim).
 * `parserClass` is injectable so tests can supply a minimal stand-in — the
 * util's own safety work is the scrub, and that is what the tests pin.
 *
 * @param {string} html
 * @param {{parserClass?: new () => { parseFromString: (html: string, type: string) => { body: { childNodes: InertNode[] } } }}} [options]
 * @returns {InertNode[]}
 */
export function parseInert(html, { parserClass } = {}) {
  const Parser = parserClass ?? globalThis.DOMParser;
  if (typeof Parser !== 'function') {
    throw new Error('no DOMParser in this environment; inject parserClass');
  }
  const doc = new Parser().parseFromString(html, 'text/html');
  return Array.from(/** @type {InertNode[]} */ (doc.body.childNodes));
}

/**
 * Drop network-bearing elements and event-handler/non-https URL attributes
 * from a node tree, in place. Returns the same top-level nodes, scrubbed.
 *
 * Discard set (E9, contract §1): every element type whose mere presence can
 * fetch, embed, or execute — script, iframe, object, embed, link, meta,
 * style.
 *
 * @param {InertNode[]} nodes
 * @returns {InertNode[]}
 */
export function scrub(nodes) {
  const kept = [];
  for (const node of nodes) {
    if (isDiscarded(node)) continue;
    scrubAttributes(node);
    const children = rowsOf(node.childNodes);
    if (children) {
      const keptChildren = scrub(children);
      if (Array.isArray(node.childNodes)) {
        // A double: the array IS the tree — replace it with the survivors.
        node.childNodes = keptChildren;
      } else {
        // A live node: detach the discarded children from the tree itself —
        // adoption moves the parent wholesale, so a nested payload must be
        // gone from the parent before it moves.
        const survivors = new Set(keptChildren);
        for (const child of children) {
          if (survivors.has(child)) continue;
          if (typeof node.removeChild === 'function') node.removeChild(child);
          else if (typeof child.remove === 'function') child.remove();
        }
      }
    }
    kept.push(node);
  }
  return kept;
}

/**
 * Array-likes (NodeList) to arrays; plain arrays pass through; anything
 * else is "no children". @param {InertNode[] | undefined} rows
 * @returns {InertNode[] | null}
 */
function rowsOf(rows) {
  if (Array.isArray(rows)) return rows;
  if (rows && typeof /** @type {any} */ (rows).length === 'number') {
    return Array.from(/** @type {any} */ (rows));
  }
  return null;
}

/** Element types the scrub never moves into a live tree (contract §1). */
const DISCARDED_ELEMENTS = new Set([
  'script',
  'iframe',
  'object',
  'embed',
  'link',
  'meta',
  'style',
]);

/** @param {InertNode} node */
function isDiscarded(node) {
  return DISCARDED_ELEMENTS.has(node.nodeName.toLowerCase());
}

/** URL-bearing attributes — the ones a scrubbed tree may still carry. */
const URL_ATTRIBUTES = new Set([
  'href',
  'src',
  'srcset',
  'imagesrcset',
  'xlink:href',
  'action',
  'formaction',
  'poster',
  'background',
]);

/**
 * The https-or-fragment guarantee (contract §1): a URL attribute survives
 * only with an explicit `https:` scheme or as a same-document fragment.
 * `javascript:`, `data:`, protocol-relative (`//…`), and relative forms are
 * all dropped (contract §1). Single-URL attributes only — srcset-style
 * candidate lists are judged per candidate by `srcsetAllowed`.
 * @param {string} value
 */
function urlAllowed(value) {
  const candidate = value.trim().toLowerCase();
  return candidate.startsWith('https:') || candidate.startsWith('#');
}

// Spelled from parts: a literal `javascript:` string is itself a script
// URL (eslint no-script-url), and the scrubber must name the scheme
// without carrying one.
const scriptScheme = ['java', 'script:'].join('');

/**
 * A `srcset`/`imagesrcset` value is a comma-separated candidate LIST —
 * `url 1x`, `url 640w`, … — so it is judged candidate-by-candidate: every
 * candidate's URL token (the first whitespace-delimited field) must pass
 * `urlAllowed`. One dirty candidate kills the whole attribute (MOR-124
 * F11: the single-URL predicate was judging only the first token, so a
 * hostile second candidate rode through).
 * @param {string} value
 */
function srcsetAllowed(value) {
  return value
    .split(',')
    .map((candidate) => candidate.trim().split(/\s+/)[0] ?? '')
    .filter((candidate) => candidate !== '')
    .every(
      (candidate) =>
        !candidate.toLowerCase().startsWith(scriptScheme) && urlAllowed(candidate),
    );
}

/** @param {InertNode} node */
function scrubAttributes(node) {
  const live = node.attributes;
  if (!live) return;
  const isArray = Array.isArray(live);
  // A snapshot to iterate: splicing the array (doubles) or calling
  // removeAttribute (live nodes) must not fight the loop's index.
  const rows = isArray ? live : Array.from(/** @type {any} */ (live));
  /** @param {{name: string, value: string}} attribute */
  const dangerous = (attribute) => {
    const name = attribute.name.toLowerCase();
    if (name.startsWith('on')) return true;
    // `style` as an ATTRIBUTE is dropped outright: `background:url(…)` is a
    // network fetch the element discard list cannot see (contract §1).
    if (name === 'style') return true;
    if (!URL_ATTRIBUTES.has(name)) return false;
    if (name === 'srcset' || name === 'imagesrcset') return !srcsetAllowed(attribute.value);
    const value = attribute.value.trim().toLowerCase();
    return value.startsWith(scriptScheme) || !urlAllowed(attribute.value);
  };
  for (let index = rows.length - 1; index >= 0; index -= 1) {
    const attribute = /** @type {{name: string, value: string}} */ (rows[index]);
    if (!dangerous(attribute)) continue;
    if (isArray) live.splice(index, 1);
    else if (typeof node.removeAttribute === 'function') node.removeAttribute(attribute.name);
    else /** @type {any} */ (live).removeNamedItem?.(attribute.name);
  }
}

/**
 * The prototype's `setHTML`, as one call: parse inertly, scrub, adopt.
 * `replace` must expose `replaceChildren(...nodes)`; when it exposes
 * `removeChild`/`appendChild` instead, adoption falls back to them. The
 * host type is deliberately wide — real DOM elements and the tests'
 * doubles both fit.
 *
 * @param {{replaceChildren?: (...nodes: any[]) => void, appendChild?: (node: any) => void, removeChild?: (node: any) => void, childNodes?: ArrayLike<any>}} replace
 * @param {string} html
 * @param {{parserClass?: new () => { parseFromString: (html: string, type: string) => { body: { childNodes: InertNode[] } } }}} [options]
 */
export function adoptHTML(replace, html, options = {}) {
  const nodes = scrub(parseInert(html, options));
  if (typeof replace.replaceChildren === 'function') {
    replace.replaceChildren(...nodes);
    return;
  }
  for (const child of Array.from(replace.childNodes ?? [])) {
    replace.removeChild?.(child);
  }
  for (const node of nodes) replace.appendChild?.(node);
}

/**
 * Escape a dynamic string for interpolation into an HTML template — the
 * prototype's `esc`, verbatim (docs/reference line 971). Every dynamic
 * string goes through this BEFORE it enters a template (names,
 * descriptions, cites, URLs); templates are then parsed inertly by
 * `adoptHTML`.
 *
 * @param {unknown} value
 * @returns {string}
 */
export function esc(value) {
  return String(value).replace(
    /[&<>"]/g,
    /** @param {string} char */ (char) =>
      /** @type {Record<string, string>} */ ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
      })[char],
  );
}

// ---- linkification (contract §2) ------------------------------------------
//
// The prototype's `linkConds` (line 904), fed by the curated seed instead of
// the prototype's own map: split on tag segments, examine TEXT segments
// only, wrap longest-match-first condition names in plain
// `<a data-cond="key">` anchors (no href — the tip layer owns activation).

import seed from '../rules/condition-prose.json';

/** Built once from the seed: linkable keys, longest first, regex-escaped. */
const matcher = /** @returns {RegExp} */ (() => {
  let compiled = /** @type {RegExp | null} */ (null);
  return () => {
    if (!compiled) {
      const keys = Object.keys(seed)
        .filter((key) => {
          const entry = /** @type {any} */ (seed)[key];
          return key !== '_readme' && entry && entry.link !== false;
        })
        .sort((a, b) => b.length - a.length);
      const sources = keys.map((key) =>
        key
          .replace(/[-]/g, '\\-')
          .replace(/\s+/g, '\\s+'),
      );
      // Word-boundary anchored; the prototype's `[- ]actions?` lookahead
      // keeps "frightened actions" (the phrase) from matching the condition.
      compiled = new RegExp(`\\b(${sources.join('|')})\\b(?![- ]actions?\\b)`, 'gi');
    }
    return compiled;
  };
})();

/**
 * Wrap condition-name matches in `<a data-cond="key">…</a>` — text segments
 * only; tags and their attributes pass through untouched by construction.
 * The `skip` name (the popup's own subject) is not self-linked.
 *
 * @param {string} text esc'ed prose (callers escape BEFORE linkifying)
 * @param {{skip?: string}} [options]
 * @returns {string}
 */
export function linkifyConditions(text, { skip } = {}) {
  const pattern = matcher();
  return String(text)
    .split(/(<[^>]+>)/)
    .map((segment) => {
      if (segment.startsWith('<')) return segment;
      return segment.replace(pattern, (match) => {
        const key = match.toLowerCase().replace(/\s+/g, ' ');
        if (key === skip) return match;
        // Named suppression, same precedent as 7df77cc/a071ec2: the rule is
        // shape-based (HTML + interpolation), but this anchor's inputs are
        // `match` — esc'ed-by-contract prose (callers escape BEFORE
        // linkifying; the escape contract has teeth per MOR-124 F5) — and
        // `key`, in-repo seed data. The consumer is adoptHTML's scrubber,
        // proven through the real DOMParser in tests/util/inert-html.test.js,
        // not a raw innerHTML sink. Not a template hole.
        return `<a data-cond="${key}">${match}</a>`; /* nosemgrep: html-in-template-string */
      });
    })
    .join('');
}
