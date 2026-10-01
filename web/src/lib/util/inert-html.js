// The one inert-HTML primitive (E6 spec §7): every prose/HTML injection in
// the sheet goes through here, porting the prototype's `setHTML` discipline
// (docs/reference/lorum_ipsum_dashboard.html line 966).
//
// The platform property this relies on is the HTML spec's own: a document
// parsed by DOMParser is *inert* — its scripts do not run — and nodes
// adopted out of it do not suddenly become executable. On top of that
// platform guarantee this module adds a defensive scrub before adoption:
// script elements are dropped and `on*`/`javascript:`-carrying attributes
// are stripped, so a future caller that bypasses the parser (or a platform
// change) still cannot arm a payload through this util. No component may
// innerHTML-assign; there is exactly one shared primitive.

/* global DOMParser */

/**
 * A parsed, inert node: the util only touches `nodeName`, `attributes`,
 * `childNodes`, and the append target's DOM methods, so tests can drive it
 * with minimal doubles.
 * @typedef {Object} InertNode
 * @property {string} nodeName
 * @property {InertNode[]} [childNodes]
 * @property {Array<{name: string, value: string}>} [attributes]
 */

/**
 * Parse `html` inertly and return the top-level nodes of its body.
 *
 * In a browser this is DOMParser (the prototype's discipline, verbatim).
 * `parserClass` is injectable so tests can supply a minimal stand-in — the
 * util's own safety work is the scrub, and that is what the tests pin.
 *
 * @param {string} html
 * @param {{parserClass?: Function}} [options]
 * @returns {InertNode[]}
 */
export function parseInert(html, { parserClass } = {}) {
  const Parser = parserClass ?? globalThis.DOMParser;
  if (typeof Parser !== 'function') {
    throw new Error('no DOMParser in this environment; inject parserClass');
  }
  const doc = new Parser().parseFromString(html, 'text/html');
  return Array.from(doc.body.childNodes);
}

/**
 * Drop script nodes and event-handler/`javascript:` attributes from a node
 * tree, in place. Returns the same top-level nodes, scrubbed.
 *
 * @param {InertNode[]} nodes
 * @returns {InertNode[]}
 */
export function scrub(nodes) {
  const kept = [];
  for (const node of nodes) {
    if (isScript(node)) continue;
    scrubAttributes(node);
    if (Array.isArray(node.childNodes)) {
      node.childNodes = scrub(node.childNodes);
    }
    kept.push(node);
  }
  return kept;
}

/** @param {InertNode} node */
function isScript(node) {
  return node.nodeName.toLowerCase() === 'script';
}

/** @param {InertNode} node */
function scrubAttributes(node) {
  const attributes = node.attributes;
  if (!Array.isArray(attributes)) return;
  const dangerous = (attribute) =>
    attribute.name.toLowerCase().startsWith('on') ||
    (['href', 'src', 'xlink:href', 'action', 'formaction'].includes(
      attribute.name.toLowerCase(),
    ) &&
      attribute.value.trim().toLowerCase().startsWith('javascript:'));
  for (let index = attributes.length - 1; index >= 0; index -= 1) {
    if (dangerous(attributes[index])) attributes.splice(index, 1);
  }
}

/**
 * The prototype's `setHTML`, as one call: parse inertly, scrub, adopt.
 * `replace` must expose `replaceChildren(...nodes)`; when it exposes
 * `removeChild`/`appendChild` instead, adoption falls back to them.
 *
 * @param {{replaceChildren?: Function, appendChild?: Function, removeChild?: Function}} replace
 * @param {string} html
 * @param {{parserClass?: Function}} [options]
 */
export function adoptHTML(replace, html, options = {}) {
  const nodes = scrub(parseInert(html, options));
  if (typeof replace.replaceChildren === 'function') {
    replace.replaceChildren(...nodes);
    return;
  }
  for (const child of [...(replace.childNodes ?? [])]) {
    replace.removeChild(child);
  }
  for (const node of nodes) replace.appendChild(node);
}
