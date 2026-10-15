// PF2e action symbols as inline SVG — Dave's actionSVG, ported verbatim
// (docs/reference/lorum_ipsum_dashboard.html, actionSVG/glyph). Presentation,
// not game data: the glyph is how the books print an action cost.

export const GLYPH_TITLE = /** @type {Record<string, string>} */ ({
  1: 'Single action',
  2: 'Two actions',
  3: 'Three actions',
  R: 'Reaction',
  F: 'Free action',
});

/** @param {string} a */
export function actionSVG(a) {
  if (a === 'R')
    return '<svg viewBox="0 0 20 16" aria-hidden="true"><path d="M0.4,4.3 C3.5,0.2 10,-0.6 14.8,1.1 C19.6,2.9 20.9,8.6 17.4,11.4 C16.4,12.2 15.2,12.8 13.9,13.2 L12.1,15.9 L3.3,14 L10.3,10.2 L12.3,11.7 C15.3,10.4 16,6.2 13.4,4.3 C10.4,2.1 4.6,2 1.3,3.3 Z" fill="currentColor"/></svg>';
  if (a === 'F')
    return '<svg viewBox="0 0 16 16" aria-hidden="true"><polygon points="8,0.9 15.1,8 8,15.1 0.9,8" fill="none" stroke="currentColor" stroke-width="1.6"/><polygon points="6.1,4.2 9.9,8 6.1,11.8 4.2,9.9 6.1,8 4.2,6.1" fill="currentColor"/></svg>';
  const n = Math.min(3, Math.max(1, Number(a) || 1));
  const CX = [7.9, 18.2, 27.6];
  const SC = [1, 0.9, 0.69];
  const W = [0, 15.9, 25.4, 33.2];
  const parts = ['2.8,5.2 5.6,8 2.8,10.8 0,8'];
  for (let k = 0; k < n; k += 1) {
    const cx = CX[k];
    const h = 8 * SC[k];
    parts.push(
      [[cx, 8 - h], [cx + h, 8], [cx, 8 + h], [cx - 0.48 * h, 8 + 0.52 * h], [cx + 0.04 * h, 8], [cx - 0.48 * h, 8 - 0.52 * h]]
        .map((p) => p.map((v) => +v.toFixed(2)).join(','))
        .join(' '),
    );
  }
  return `<svg viewBox="0 0 ${W[n]} 16" aria-hidden="true">${parts.map((p) => `<polygon points="${p}" fill="currentColor"/>`).join('')}</svg>`;
}

/** Glyph HTML for an action cost: 1|2|3|R|F, or "1-3" ranges. */
export function glyphHTML(/** @type {string | number | null | undefined} */ a) {
  if (a == null || a === '') return '';
  const key = String(a);
  if (GLYPH_TITLE[key]) return `<span class="glyph" title="${GLYPH_TITLE[key]}">${actionSVG(key)}</span>`;
  const range = key.match(/^([123])\s*(?:[–-]|to)\s*([123])$/);
  if (range)
    return `<span class="glyph" title="${range[1]} to ${range[2]} actions">${actionSVG(range[1])}<span class="dash">–</span>${actionSVG(range[2])}</span>`;
  return '';
}

/** Map a Foundry {action_type, actions} / spell time to a glyph key. */
export function costKey(/** @type {any} */ d) {
  if (!d) return null;
  if (d.action_type === 'reaction') return 'R';
  if (d.action_type === 'free') return 'F';
  if (d.action_type === 'action' && d.actions) return String(d.actions);
  const t = String(d.time ?? '');
  if (/^[123]$/.test(t)) return t;
  if (t === 'reaction') return 'R';
  if (t === 'free') return 'F';
  if (/^[123] (to|-) [123]$/.test(t)) return t;
  return null;
}

/** Back-compat for DetailTip: key → bare glyph HTML. */
export const ACTION_GLYPH = /** @type {Record<string, string>} */ ({
  1: actionSVG('1'),
  2: actionSVG('2'),
  3: actionSVG('3'),
  R: actionSVG('R'),
  F: actionSVG('F'),
});
