// The prose boundary (E9, contracts/inert-html.md §3): Svelte `{@html}` is
// banned for prose repo-wide — every markup-carrying string must route
// through the one inert setter (`util/inert-html.js` `adoptHTML`) inside the
// sanctioned components. Same discipline as the justfile's `boundary` recipe
// (the engine's dependency edge): a grep-based gate that fails loudly, not a
// lint rule nobody configured. Sanctioned set is exact and closed: new
// members require a spec change, not a quiet edit here.
/* global console, process */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, sep } from 'node:path';

const ROOT = join(import.meta.dirname, '..', 'src');
/** Components that may own `{@html}`: the prose renderers. Paths are
 * repo-relative to `src` — a basename match would exempt any future file
 * that happens to share the name (MOR-115 finding 8). */
const SANCTIONED = new Set([
  'lib/sheet/components/ConditionTip.svelte',
  'lib/sheet/components/AboutView.svelte',
]);

/** @param {string} dir @returns {string[]} */
function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...walk(path));
    else if (path.endsWith('.svelte')) out.push(path);
  }
  return out;
}

const offenders = [];
for (const path of walk(ROOT)) {
  const relative = path.slice(ROOT.length + 1).split(sep).join('/');
  if (SANCTIONED.has(relative)) continue;
  const lines = readFileSync(path, 'utf8').split('\n');
  lines.forEach((line, index) => {
    if (line.includes('{@html')) {
      offenders.push(`${path}:${index + 1}: ${line.trim()}`);
    }
  });
}

if (offenders.length > 0) {
  console.error(
    [
      'PROSE BOUNDARY VIOLATION: `{@html}` outside the sanctioned components',
      `(allowed: ${[...SANCTIONED].sort().join(', ')}). Prose must render through`,
      'util/inert-html.js `adoptHTML` — see specs/010-rules-tooltips-custom-content/contracts/inert-html.md:',
      ...offenders.map((line) => `  ${line}`),
    ].join('\n'),
  );
  process.exit(1);
}
console.log(
  `html-boundary ok: no {@html} outside ${[...SANCTIONED].sort().join(', ')}` /* nosemgrep: missing-template-string-indicator — the {@html} literal is prose, not a template hole */,
);
