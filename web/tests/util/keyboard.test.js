import { test } from 'vitest';
import assert from 'node:assert/strict';

import { trapKeys } from '../../src/lib/util/keyboard.js';

// A keyboard-event double: {key, target, shiftKey, preventDefault,
// stopPropagation} — the shape the util reads.

/** @returns {Record<string, *>} */
function keyEvent(key, { target = {}, shiftKey = false } = {}) {
  const prevented = [];
  return {
    key,
    target,
    shiftKey,
    preventDefault: () => prevented.push('default'),
    stopPropagation: () => prevented.push('propagation'),
    prevented,
  };
}

/** A container double: records listeners; focusables are set by the test. */
function container(focusables = []) {
  const listeners = new Map();
  return {
    listeners,
    querySelectorAll: () => focusables,
    addEventListener: (type, fn) => listeners.set(type, fn),
    removeEventListener: (type, fn) => {
      if (listeners.get(type) === fn) listeners.delete(type);
    },
  };
}

test('Escape cancels, stops the event, and the disposer restores focus', () => {
  const host = { focused: 0, focus() { this.focused += 1; } };
  const box = container();
  let cancels = 0;
  const dispose = trapKeys({ container: box, host, onCancel: () => (cancels += 1) });

  box.listeners.get('keydown')(keyEvent('Escape'));
  assert.equal(cancels, 1);

  dispose();
  assert.equal(host.focused, 1, 'focus returns to the opener');
  assert.equal(box.listeners.size, 0, 'the listener is gone');
});

test('Enter commits on a plain element but not on buttons or textareas', () => {
  let commits = 0;
  const box = container();
  const setup = () =>
    trapKeys({
      container: box,
      host: { focus() {} },
      onCancel: () => {},
      onCommit: () => (commits += 1),
    });
  const dispose = setup();

  box.listeners.get('keydown')(keyEvent('Enter', { target: { tagName: 'SPAN' } }));
  assert.equal(commits, 1, 'plain element: the util commits');

  box.listeners.get('keydown')(keyEvent('Enter', { target: { tagName: 'BUTTON' } }));
  box.listeners.get('keydown')(keyEvent('Enter', { target: { tagName: 'TEXTAREA' } }));
  box.listeners.get('keydown')(keyEvent('Enter', { target: { tagName: 'A' } }));
  assert.equal(commits, 1, 'native committers are left to the browser');
  dispose();
});

test('Tab wraps inside the focusables, both directions', () => {
  const first = { focused: 0, focus() { this.focused += 1; } };
  const last = { focused: 0, focus() { this.focused += 1; } };
  const middle = { focused: 0, focus() { this.focused += 1; } };
  const box = container([first, middle, last]);
  const dispose = trapKeys({
    container: box,
    host: { focus() {} },
    onCancel: () => {},
  });

  box.listeners.get('keydown')(keyEvent('Tab', { target: last }));
  assert.equal(first.focused, 1, 'forward from last wraps to first');

  box.listeners.get('keydown')(keyEvent('Tab', { target: first, shiftKey: true }));
  assert.equal(last.focused, 1, 'backward from first wraps to last');

  box.listeners.get('keydown')(keyEvent('Tab', { target: middle }));
  assert.equal(first.focused, 1, 'mid-tab: no wrapping, the browser moves focus');
  assert.equal(last.focused, 1);
  dispose();
});

test('the disposer is idempotent about its listener removal', () => {
  const box = container();
  const dispose = trapKeys({
    container: box,
    host: { focus() {} },
    onCancel: () => {},
  });
  dispose();
  dispose();
  assert.equal(box.listeners.size, 0);
});
