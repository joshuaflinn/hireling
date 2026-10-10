/* global document */
import { test } from 'vitest';
import assert from 'node:assert/strict';

import { parseInert, scrub, adoptHTML, esc, linkifyConditions } from '../../src/lib/util/inert-html.js';

// A minimal stand-in for the browser's inert DOMParser: enough structure
// (node names, attributes, childNodes) for the util's contract to be pinned.
// The browser's inertness is the platform's property; the SCRUB is this
// module's work, and that is what these tests prove.

/** @returns {import('../../src/lib/util/inert-html.js').InertNode} */
function el(name, attributes = [], childNodes = []) {
  return {
    nodeName: name,
    attributes: attributes.map(([attrName, value]) => ({ name: attrName, value })),
    childNodes,
  };
}

/** A parserClass over a tiny HTML subset: <tag attrs>text</tag>, no nesting. */
function stubParser() {
  return class {
    constructor() {}
    parseFromString(html) {
      const childNodes = [];
      const pattern =
        /<script[\s\S]*?<\/script>|<([a-z]+)((?:\s+[a-z-]+="[^"]*")*)\s*>([^<]*)<\/\1>|([^<]+)/gi;
      for (const match of html.matchAll(pattern)) {
        if (match[1]) {
          const attributes = [...match[2].matchAll(/([a-z-]+)="([^"]*)"/g)].map(
            ([, name, value]) => [name, value],
          );
          childNodes.push(el(match[1].toUpperCase(), attributes));
        } else if (match[4] !== undefined && match[4].trim() !== '') {
          childNodes.push({ nodeName: '#text', childNodes: [] });
        }
      }
      return { body: { childNodes } };
    }
  };
}

const opts = () => ({ parserClass: stubParser() });

test('parseInert parses through the injected DOMParser and adopts body nodes', () => {
  const nodes = parseInert('<b>Bold</b> plain', opts());
  assert.equal(nodes.length, 2, 'the element and the text node');
  assert.equal(nodes[0].nodeName, 'B');
});

test('scrub drops script elements entirely', () => {
  const nodes = scrub(
    parseInert('<p>safe</p><script>alert(1)</script>', opts()),
  );
  assert.deepEqual(
    nodes.map((node) => node.nodeName),
    ['P'],
    'the script node never reaches the adopted tree',
  );
});

test('scrub strips on* handlers and javascript: URLs, keeps the rest', () => {
  const nodes = scrub(
    parseInert(
      '<a href="javascript:evil()" onclick="pwn()" title="keep me" data-x="ok">link</a>',
      opts(),
    ),
  );
  const kept = nodes[0].attributes.map((attribute) => attribute.name);
  assert.deepEqual(kept.sort(), ['data-x', 'title'], `got ${kept}`);
});

test('scrub recurses: a script nested under a kept element goes too', () => {
  const inner = el('DIV', [], [el('SCRIPT'), el('B')]);
  const [kept] = scrub([inner]);
  assert.deepEqual(
    kept.childNodes.map((node) => node.nodeName),
    ['B'],
  );
});

test('adoptHTML clears the host and appends the scrubbed nodes', () => {
  const adopted = [];
  const removed = [];
  const existing = { nodeName: '#old' };
  const host = {
    childNodes: [existing],
    removeChild: (node) => removed.push(node),
    appendChild: (node) => adopted.push(node),
  };
  adoptHTML(
    host,
    '<p onclick="pwn()">safe prose</p><script>bad()</script>',
    opts(),
  );
  assert.deepEqual(removed, [existing], 'old children go');
  assert.deepEqual(
    adopted.map((node) => node.nodeName),
    ['P'],
    'the script never lands',
  );
  assert.deepEqual(
    adopted[0].attributes,
    [],
    'the handler never lands either',
  );
});

test('adoptHTML prefers replaceChildren when the host has it', () => {
  const calls = [];
  const host = {
    replaceChildren: (...nodes) => calls.push(nodes),
    childNodes: [],
    removeChild: () => calls.push('removeChild'),
    appendChild: () => calls.push('appendChild'),
  };
  adoptHTML(host, '<i>x</i>', opts());
  assert.equal(calls.length, 1);
  assert.deepEqual(
    calls[0].map((node) => node.nodeName),
    ['I'],
  );
});

test('parseInert without DOMParser or injection names the fix', () => {
  const saved = globalThis.DOMParser;
  delete globalThis.DOMParser;
  try {
    assert.throws(() => parseInert('<b>x</b>'), /inject parserClass/);
  } finally {
    globalThis.DOMParser = saved;
  }
});

// ---- E9 extensions (contracts/inert-html.md §1/§2/§4) — new rows only ----

test('scrub drops every network-bearing element type, keeps the text around it', () => {
  for (const tag of ['IFRAME', 'OBJECT', 'EMBED', 'LINK', 'META', 'STYLE']) {
    const nodes = scrub([el(tag), { nodeName: '#text' }, el('B')]);
    assert.deepEqual(
      nodes.map((node) => node.nodeName),
      ['#text', 'B'],
      `${tag} never reaches the adopted tree`,
    );
  }
});

test('URL attributes survive only https: or fragment — data:, protocol-relative, relative dropped', () => {
  const nodes = scrub([
    el('A', [['href', 'https://2e.aonprd.com/Conditions.aspx?ID=76']]),
    el('A', [['href', '#second-layer']]),
    el('A', [['href', 'data:text/html;base64,AAAA']]),
    el('A', [['href', '//evil.example/x']]),
    el('A', [['href', 'relative/page.html']]),
    el('A', [['href', 'JAVASCRIPT:evil()']]),
    el('IMG', [['src', 'data:image/png;base64,AAAA']]),
  ]);
  const hrefs = nodes.map((node) => /** @type {any} */ (node).attributes?.[0]?.value ?? null);
  assert.deepEqual(hrefs, [
    'https://2e.aonprd.com/Conditions.aspx?ID=76',
    '#second-layer',
    null,
    null,
    null,
    null,
    null,
  ]);
});

test('srcset lists are judged candidate-by-candidate — one dirty candidate kills the attribute (MOR-124 F11)', () => {
  const nodes = scrub([
    el('IMG', [['srcset', 'https://ok.example/a.jpg 1x, //evil.example/b.jpg 2x']]),
    el('IMG', [['imagesrcset', 'https://ok.example/a.jpg 1x, javascript:bad() 2x']]),
    el('IMG', [['srcset', 'https://ok.example/a.jpg 1x, https://ok.example/b.jpg 2x']]),
  ]);
  assert.equal(nodes[0].attributes.length, 0, 'the protocol-relative candidate kills the list');
  assert.equal(nodes[1].attributes.length, 0, 'the script-scheme candidate kills the list');
  assert.equal(nodes[2].attributes[0]?.name, 'srcset', 'an all-https list survives whole');
});

test('poster, background and the style attribute get the same verdicts as href/src (MOR-124 F12)', () => {
  const nodes = scrub([
    el('VIDEO', [['poster', 'https://ok.example/f.jpg']]),
    el('VIDEO', [['poster', 'javascript:bad()']]),
    el('BODY', [['background', '//evil.example/beacon']]),
    el('P', [['style', 'background:url(//evil.example/beacon)']]),
  ]);
  assert.equal(nodes[0].attributes[0]?.name, 'poster', 'https poster survives');
  assert.equal(nodes[1].attributes.length, 0, 'script-scheme poster dies');
  assert.equal(nodes[2].attributes.length, 0, 'protocol-relative background dies');
  assert.equal(nodes[3].attributes.length, 0, 'the style attribute is dropped outright');
});

test('a hostile srcset list dies through the real DOMParser adoption path too (MOR-124 F11)', () => {
  const host = document.createElement('div');
  adoptHTML(host, '<img srcset="https://ok.example/a.jpg 1x, javascript:bad() 2x">');
  assert.equal(
    host.querySelector('img')?.getAttribute('srcset') ?? null,
    null,
    'the scrubbed image lands with no srcset at all',
  );
});

test('the same guarantees hold through the real DOMParser adoption path', () => {
  // jsdom ships the platform DOMParser — adopt the hostile prose the way the
  // components do and assert the live tree, not the double.
  const host = document.createElement('div');
  adoptHTML(
    host,
    '<p onclick="pwn()">hello <a href="javascript:bad()">js</a> ' +
      '<a href="https://2e.aonprd.com/Conditions.aspx?ID=76">aon</a></p>' +
      '<script>bad()</script><iframe src="https://evil.example"></iframe>',
  );
  assert.equal(host.querySelectorAll('script').length, 0, 'no script node');
  assert.equal(host.querySelectorAll('iframe').length, 0, 'no network-bearing node');
  assert.equal(host.querySelectorAll('p[onclick]').length, 0, 'no handler attribute');
  const hrefs = [...host.querySelectorAll('a')].map((a) => a.getAttribute('href'));
  assert.deepEqual(
    hrefs,
    [null, 'https://2e.aonprd.com/Conditions.aspx?ID=76'],
    'the javascript: href is stripped, the https href survives',
  );
  assert.equal(host.textContent, 'hello js aon', 'the text around the dropped nodes survives');
});

test('a NESTED payload is detached from the live tree before adoption (the removeChild branch)', () => {
  // The top-level row above drops script/iframe by simply not adopting them;
  // a nested one must be removed from its parent first, because adoption
  // moves the parent wholesale (MOR-115 F6 — this is the E6-fix branch).
  const host = document.createElement('div');
  adoptHTML(host, '<p>safe<script>bad()</script></p>');
  assert.equal(host.querySelectorAll('script').length, 0, 'the nested script never reaches the host');
  assert.equal(host.textContent, 'safe', 'the safe text survives');
});

test('esc entity-escapes markup characters, every time', () => {
  assert.equal(
    esc('Fish & "Chips" <b>are</b> \'food\''),
    'Fish &amp; &quot;Chips&quot; &lt;b&gt;are&lt;/b&gt; \'food\'',
  );
  assert.equal(esc(42), '42', 'non-strings stringified');
  assert.equal(esc('plain—text'), 'plain—text', 'unicode passes untouched');
});

test('linkifyConditions wraps condition names in text segments only', () => {
  const html = linkifyConditions('Frightened <b title="Off-Guard">Off-Guard</b> applies');
  // The <b> tag and its attribute pass through untouched; only text
  // segments are examined.
  assert.ok(
    html.includes('<b title="Off-Guard">'),
    `the tag segment is never rewritten: ${html}`,
  );
  assert.ok(html.includes('<a data-cond="frightened">Frightened</a>'), html);
  assert.ok(
    html.includes('<a data-cond="off-guard">Off-Guard</a></b>'),
    `the name inside the tag is linkified: ${html}`,
  );
  assert.ok(html.endsWith(' applies'), 'the plain text after the tag renders');
});

test('linkifyConditions honors skip (the popup subject) and link:false seeds', () => {
  // The popup's own subject is never self-linked — the match text survives.
  const self = linkifyConditions('Frightened drops by 1', { skip: 'frightened' });
  assert.ok(!self.includes('data-cond'), `skip must not self-link: ${self}`);
  // broken/controlled carry link:false in the seed — never match.
  const meta = linkifyConditions('The shield is Broken and you are Controlled');
  assert.ok(!meta.includes('data-cond'), `link:false entries never match: ${meta}`);
});

test('linkifyConditions matches through whitespace variants but not "actions"', () => {
  const spaced = linkifyConditions('persistent   damage rolls fresh');
  assert.ok(
    spaced.includes('<a data-cond="persistent damage">persistent   damage</a>'),
    `whitespace runs collapse into the match key: ${spaced}`,
  );
  const actions = linkifyConditions('Frightened actions are still yours');
  assert.ok(
    !actions.includes('data-cond'),
    `the prototype's actions-lookahead ports: ${actions}`,
  );
});
