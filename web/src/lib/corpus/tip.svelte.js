/* global $state, window */
// The page-level detail popup state (Dave's #tip): one floating card for
// spells, feats, features, actions, items, creatures. Rows call showTip on
// hover/focus and pinTip on click; the card fetches the corpus entry.

export const tip = $state({
  open: false,
  pinned: false,
  /** @type {{kind: string, name?: string, id?: string, sub?: string} | null} */
  subject: null,
  /** @type {{x: number, y: number}} */
  at: { x: 0, y: 0 },
  /** stack of previous subjects for back-navigation inside a pinned card */
  /** @type {Array<{kind: string, name?: string, id?: string}>} */
  trail: [],
});

/**
 * @param {{kind: string, name?: string, id?: string, sub?: string}} subject
 * @param {Element | null} anchor
 */
export function showTip(subject, anchor) {
  if (tip.pinned) return;
  place(anchor);
  tip.subject = subject;
  tip.trail = [];
  tip.open = true;
}

/**
 * @param {{kind: string, name?: string, id?: string, sub?: string}} subject
 * @param {Element | null} anchor
 */
export function pinTip(subject, anchor) {
  if (tip.pinned && tip.subject && same(tip.subject, subject)) {
    closeTip();
    return;
  }
  place(anchor);
  tip.subject = subject;
  tip.trail = [];
  tip.open = true;
  tip.pinned = true;
}

/** Follow a cross-reference inside the card (keeps it pinned). */
export function followTip(/** @type {{kind: string, name?: string, id?: string}} */ subject) {
  if (tip.subject) tip.trail = [...tip.trail, tip.subject];
  tip.subject = subject;
  tip.pinned = true;
  tip.open = true;
}

export function backTip() {
  const prev = tip.trail.at(-1);
  if (!prev) return;
  tip.trail = tip.trail.slice(0, -1);
  tip.subject = prev;
}

export function hideTip() {
  if (!tip.pinned) {
    tip.open = false;
    tip.subject = null;
  }
}

export function closeTip() {
  tip.open = false;
  tip.pinned = false;
  tip.subject = null;
  tip.trail = [];
}

/**
 * @param {{kind: string, name?: string, id?: string}} a
 * @param {{kind: string, name?: string, id?: string}} b
 */
function same(a, b) {
  return a.kind === b.kind && (a.name ?? '') === (b.name ?? '') && (a.id ?? '') === (b.id ?? '');
}

/** Position beside the anchor, clamped to the viewport (Dave's placement). */
function place(/** @type {Element | null} */ anchor) {
  if (!anchor || typeof window === 'undefined') return;
  const r = anchor.getBoundingClientRect();
  const w = Math.min(440, window.innerWidth - 16);
  let x = r.right + 10;
  if (x + w > window.innerWidth - 8) x = Math.max(8, r.left - w - 10);
  const y = Math.max(8, Math.min(r.top, window.innerHeight - 200));
  tip.at = { x, y };
}

/**
 * Svelte action: make an element a tip trigger.
 * @param {HTMLElement} node
 * @param {{kind: string, name?: string, id?: string, sub?: string} | null} subject
 */
export function tipTrigger(node, subject) {
  let current = subject;
  const enter = () => current && showTip(current, node);
  const leave = () => hideTip();
  const click = (/** @type {MouseEvent} */ e) => {
    if (!current) return;
    if (/** @type {HTMLElement} */ (e.target).closest('button,input,select,a[href]')) return;
    pinTip(current, node);
  };
  const key = (/** @type {KeyboardEvent} */ e) => {
    if (e.key === 'Enter' && current && e.target === node) {
      e.preventDefault();
      pinTip(current, node);
    }
  };
  node.addEventListener('mouseenter', enter);
  node.addEventListener('mouseleave', leave);
  node.addEventListener('click', click);
  node.addEventListener('keydown', key);
  if (!node.hasAttribute('tabindex')) node.setAttribute('tabindex', '0');
  node.classList.add('has-tip');
  return {
    update(/** @type {typeof subject} */ next) {
      current = next;
    },
    destroy() {
      node.removeEventListener('mouseenter', enter);
      node.removeEventListener('mouseleave', leave);
      node.removeEventListener('click', click);
      node.removeEventListener('keydown', key);
    },
  };
}
