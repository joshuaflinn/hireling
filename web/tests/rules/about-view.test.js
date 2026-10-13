import { afterEach, test } from 'vitest';
import assert from 'node:assert/strict';

import { render, cleanup } from '@testing-library/svelte';

import AboutView from '../../src/lib/sheet/components/AboutView.svelte';

// The about view (E9 Task 10, spec US-5 AC-3): the repo's NOTICE.md content,
// including the curated-prose lane, rendered through the inert setter —
// asserted through the mounted component, never the helper alone.

afterEach(cleanup);

const noop = () => {};

function noticeBody() {
  const rendered = render(AboutView, { props: { onback: noop } });
  return rendered.container.querySelector('.notice');
}

test('the about view renders the license notice with the curated-prose lane', () => {
  const body = noticeBody();
  assert.ok(body, 'the notice body renders');
  const text = /** @type {HTMLElement} */ (body).textContent ?? '';
  assert.match(text, /License Notice/, 'the notice heading renders');
  assert.match(
    text,
    /Paizo Community Use Policy/,
    'the CUP lane renders — the notice names where the curated prose comes from'
  );
  assert.match(
    text,
    /Curated condition prose/,
    'the curated-prose lane bullet renders (spec FR-9)'
  );
  assert.match(text, /prototype freeze/, 'the provenance line renders');
  // Structure survives: the bullet list is a real list, the coverage block
  // a real pre.
  const listItems = /** @type {HTMLElement} */ (body).querySelectorAll('li');
  assert.ok(listItems.length >= 4, 'the lane bullets render as list items');
  const pre = /** @type {HTMLElement} */ (body).querySelector('pre');
  assert.ok(pre, 'the machine-readable coverage block renders as a pre');
  assert.match(pre.textContent ?? '', /prose: CUP/);
});

// The bundle-copy drift check lives in notice-copy.test.js (node
// environment): this jsdom file's vite loader denies files outside web/.

test('hostile markup in the notice renders inert — no script node, no handler, no execution', () => {
  const hostile = [
    '# About',
    '',
    'A safe line first.',
    '',
    '<script>window.__e9pwned = true</' + 'script>',
    '',
    '<img src=x onerror="window.__e9pwned = true">',
    '',
    '<a href="javascript:window.__e9pwned = true">click me</a>',
  ].join('\n');
  const rendered = render(AboutView, {
    props: { notice: hostile, onback: noop },
  });
  const body = rendered.container.querySelector('.notice');
  assert.ok(body, 'the notice body renders');
  const scripts = /** @type {HTMLElement} */ (body).querySelectorAll('script');
  assert.equal(scripts.length, 0, 'no script node survives');
  const imgs = /** @type {HTMLElement} */ (body).querySelectorAll('img');
  assert.equal(imgs.length, 0, 'no img node survives');
  const jsLinks = /** @type {HTMLElement} */ (body).querySelectorAll(
    'a[href^="javascript:"]',
  );
  assert.equal(jsLinks.length, 0, 'no javascript: link survives');
  assert.equal(
    /** @type {any} */ (globalThis).__e9pwned,
    undefined,
    'nothing executed',
  );
  // The hostile markup is visible as TEXT, inert — the reader sees exactly
  // what the notice carried, and nothing runs.
  const text = /** @type {HTMLElement} */ (body).textContent ?? '';
  assert.match(text, /window.__e9pwned = true/, 'the payload renders as text, not markup');
});

test('inline markup renders bold, code, and http(s) links — escape-first still holds', () => {
  const notice = [
    '# Inline',
    '',
    'The **license** and a `code span` plus a [link](https://example.com/policy).',
    '',
    'A [bad](javascript:alert(1)) link and a [sneaky](https://example.com/" onmouseover="alert(1)) one stay literal.',
  ].join('\n');
  const rendered = render(AboutView, {
    props: { notice, onback: noop },
  });
  const body = rendered.container.querySelector('.notice');
  assert.ok(body, 'the notice body renders');
  const strong = /** @type {HTMLElement} */ (body).querySelector('strong');
  assert.ok(strong, '**bold** becomes a strong node');
  assert.equal(strong?.textContent, 'license', 'the strong carries the inner text');
  assert.ok(
    /** @type {HTMLElement} */ (body).querySelector('code'),
    'a backtick span becomes a code node',
  );
  assert.ok(
    /** @type {HTMLElement} */ (
      body
    ).querySelector('a[href="https://example.com/policy"]'),
    'an http(s) link becomes a real anchor',
  );
  assert.equal(
    /** @type {HTMLElement} */ (body).querySelectorAll('a[href^="javascript:"]').length,
    0,
    'a javascript: target never becomes an anchor',
  );
  assert.equal(
    /** @type {HTMLElement} */ (body).querySelector('[onmouseover]'),
    null,
    'no handler attribute is minted from a URL injection attempt',
  );
  const text = /** @type {HTMLElement} */ (body).textContent ?? '';
  assert.match(text, /javascript:alert\(1\)/, 'the bad link renders as visible text');
  assert.match(text, /\[sneaky\]/, 'an unclosable URL leaves the markup literal');
});
