import { test } from 'vitest';
import assert from 'node:assert/strict';

import { parseInert, scrub, adoptHTML } from '../../src/lib/util/inert-html.js';

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
