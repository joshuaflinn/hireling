// Foundry pf2e description markup → plain display HTML (specs/015).
//
// Pack descriptions carry Foundry "enrichers": @UUID[...]{Label},
// @Damage[2d6[fire]], @Check[will|against:class-spell|basic],
// @Template[cone|distance:30], [[/r 1d4 #rounds]]{1d4 rounds},
// [[/act seek]], @Embed[...], @Localize[...]. Rendered as readable text the
// way the books print it. The OUTPUT is HTML that still goes through
// adoptHTML (parse inert + scrub) — this function never touches the DOM.
//
// UUID links keep their target as data-* so the popup layer can open the
// linked entry (data-uuid-kind + data-uuid-id = upstream _id).

/** Compendium pack name → our corpus kind. */
const PACK_KIND = /** @type {Record<string, string>} */ ({
  'spells-srd': 'spell',
  'feats-srd': 'feat',
  classfeatures: 'class_feature',
  'actionspf2e': 'action',
  conditionitems: 'condition',
  'equipment-srd': 'item',
});

/** @param {string} raw */
function attr(raw) {
  return String(raw).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);
}

/** "class-spell" → "your spell DC"; "dc:15" → "DC 15". @param {string} spec */
function checkLabel(spec) {
  const parts = spec.split('|');
  const stat = parts[0];
  const opts = Object.fromEntries(parts.slice(1).map((/** @type {string} */ p) => {
    const [k, ...v] = p.split(':');
    return [k.trim(), v.join(':').trim() || true];
  }));
  const name = stat === 'flat' ? 'flat check' : stat.charAt(0).toUpperCase() + stat.slice(1);
  let out = opts.basic ? `basic ${name}` : name;
  if (opts.dc && /^\d+$/.test(String(opts.dc))) out = `DC ${opts.dc} ${out}`;
  else if (opts.against === 'class-spell' || opts.against === 'spell') out += ' (vs spell DC)';
  else if (opts.against === 'class' || opts.against === 'class-dc') out += ' (vs class DC)';
  return out;
}

/** @Damage formula → "2d6 fire" (nested brackets supported). @param {string} inner */
function damageLabel(inner) {
  return inner
    .split(',')
    .map((/** @type {string} */ part) => {
      const m = part.trim().match(/^(.*?)\[([^\]]*)\]$/);
      let formula = (m ? m[1] : part).trim();
      const type = m ? m[2].replace(/,/g, ' ') : '';
      formula = formula.replace(/^\((.*)\)$/, '$1').replace(/@actor\.level/g, 'level').replace(/@item\.level/g, 'level');
      return `${formula}${type ? ` ${type}` : ''}`;
    })
    .join(' plus ');
}

/** Read a balanced [...] starting at `start` (which must point at '[').
 * @param {string} text @param {number} start */
function balanced(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    if (text[i] === '[') depth += 1;
    else if (text[i] === ']') {
      depth -= 1;
      if (depth === 0) return { inner: text.slice(start + 1, i), end: i + 1 };
    }
  }
  return null;
}

/** Optional {Label} right after `pos`. @param {string} text @param {number} pos */
function label(text, pos) {
  if (text[pos] !== '{') return { label: null, end: pos };
  const close = text.indexOf('}', pos);
  if (close < 0) return { label: null, end: pos };
  return { label: text.slice(pos + 1, close), end: close + 1 };
}

/**
 * Enrich one description to display HTML.
 * @param {string} html Foundry description (already HTML)
 * @returns {string}
 */
export function enrich(html) {
  const text = String(html ?? '');
  let out = '';
  let i = 0;
  while (i < text.length) {
    if (text.startsWith('@', i)) {
      const m = /^@(UUID|Damage|Check|Template|Embed|Localize)\[/.exec(text.slice(i, i + 12));
      if (m) {
        const tag = m[1];
        const br = balanced(text, i + tag.length + 1);
        if (br) {
          const lb = label(text, br.end);
          out += render(tag, br.inner, lb.label);
          i = lb.end;
          continue;
        }
      }
    }
    if (text.startsWith('[[/', i)) {
      const close = text.indexOf(']]', i);
      if (close > 0) {
        const body = text.slice(i + 3, close);
        const lb = label(text, close + 2);
        const [cmd, ...rest] = body.split(' ');
        const arg = rest.join(' ');
        if (cmd === 'act') {
          const slug = arg.split(' ')[0];
          const name = lb.label ?? slug.split('-').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
          out += `<a class="xref" data-act="${attr(slug)}">${attr(name)}</a>`;
        } else {
          const shown = lb.label ?? arg.replace(/#.*$/, '').trim();
          out += `<b class="roll">${attr(shown)}</b>`;
        }
        i = lb.end;
        continue;
      }
    }
    out += text[i];
    i += 1;
  }
  return out;
}

/** @param {string} tag @param {string} inner @param {string | null} lbl */
function render(tag, inner, lbl) {
  switch (tag) {
    case 'UUID': {
      const parts = inner.split('.');
      const id = parts[parts.length - 1];
      const pack = parts[0] === 'Compendium' ? parts[2] : null;
      const kind = pack ? PACK_KIND[pack] : null;
      const shown = lbl ?? id;
      return kind
        ? `<a class="xref" data-uuid-kind="${attr(kind)}" data-uuid-id="${attr(id)}">${attr(shown)}</a>`
        : `<span class="xref-dead">${attr(shown)}</span>`;
    }
    case 'Damage':
      return `<b class="roll">${attr(lbl ?? damageLabel(inner))}</b>`;
    case 'Check':
      return `<b class="check">${attr(lbl ?? checkLabel(inner))}</b>`;
    case 'Template': {
      const opts = Object.fromEntries(inner.split('|').map((/** @type {string} */ p) => {
        const [k, ...v] = p.split(':');
        return v.length ? [k, v.join(':')] : ['type', k];
      }));
      return `<b class="area">${attr(lbl ?? `${opts.distance ?? ''}-foot ${opts.type ?? ''}`.trim())}</b>`;
    }
    case 'Embed':
    case 'Localize':
      return lbl ? attr(lbl) : '';
    default:
      return '';
  }
}

/**
 * Upstream ids linked from a description (curriculum lists, cross-refs).
 * @param {string} html
 * @param {string} [kind] filter to one corpus kind
 * @returns {Array<{kind: string, id: string, label: string}>}
 */
export function linkedIds(html, kind) {
  const out = [];
  const re = /@UUID\[Compendium\.pf2e\.([^.\]]+)\.Item\.([A-Za-z0-9]+)\](?:\{([^}]*)\})?/g;
  let m;
  while ((m = re.exec(String(html ?? ''))) !== null) {
    const k = PACK_KIND[m[1]];
    if (!k || (kind && k !== kind)) continue;
    out.push({ kind: k, id: m[2], label: m[3] ?? m[2] });
  }
  return out;
}
