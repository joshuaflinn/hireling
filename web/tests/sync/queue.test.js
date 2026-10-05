import { test } from 'vitest';
import assert from 'node:assert/strict';

import { createQueue } from '../../src/lib/sync/queue.js';

// The storage fake mirrors the real interface (getItem/setItem over one
// backing map) so a "reload" is just: new queue, same storage.
function fakeStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, v),
  };
}

function op(id, baseVersion = 1) {
  return {
    op_id: id,
    target: { kind: 'vitals', character_id: 7, field: 'hp' },
    base_version: baseVersion,
    value: 10,
    created_at: '2026-09-30T00:00:00Z',
  };
}

test('enqueue preserves FIFO order and reports length', () => {
  const q = createQueue({ storage: fakeStorage(), accountSub: 'sub-a' });
  q.enqueue(op('op-1'));
  q.enqueue(op('op-2'));
  q.enqueue(op('op-3'));
  assert.deepEqual(q.peekAll().map((o) => o.op_id), ['op-1', 'op-2', 'op-3']);
  assert.equal(q.length, 3);
});

test('the queue persists through a reload (new queue, same storage)', () => {
  const storage = fakeStorage();
  const q = createQueue({ storage, accountSub: 'sub-a' });
  q.enqueue(op('op-1'));
  q.enqueue(op('op-2'));

  const reloaded = createQueue({ storage, accountSub: 'sub-a' });
  assert.deepEqual(reloaded.peekAll().map((o) => o.op_id), ['op-1', 'op-2']);
  // The reloaded queue stays usable and still FIFO.
  reloaded.enqueue(op('op-3'));
  reloaded.dequeue('op-1');
  assert.deepEqual(reloaded.peekAll().map((o) => o.op_id), ['op-2', 'op-3']);
});

test('queues are isolated per account sub', () => {
  const storage = fakeStorage();
  const qa = createQueue({ storage, accountSub: 'sub-a' });
  const qb = createQueue({ storage, accountSub: 'sub-b' });
  qa.enqueue(op('op-a'));
  qb.enqueue(op('op-b'));
  assert.deepEqual(qa.peekAll().map((o) => o.op_id), ['op-a']);
  assert.deepEqual(qb.peekAll().map((o) => o.op_id), ['op-b']);
  // Dequeue on one account's queue cannot touch the other's rows.
  qb.dequeue('op-a');
  assert.equal(qa.length, 1);
});

test('dequeue removes exactly the named op', () => {
  const q = createQueue({ storage: fakeStorage(), accountSub: 'sub-a' });
  q.enqueue(op('op-1'));
  q.enqueue(op('op-2'));
  const removed = q.dequeue('op-1');
  assert.equal(removed.op_id, 'op-1');
  assert.deepEqual(q.peekAll().map((o) => o.op_id), ['op-2']);
  assert.equal(q.dequeue('nope'), null);
});

test('onChange fires on enqueue and dequeue', () => {
  const q = createQueue({ storage: fakeStorage(), accountSub: 'sub-a' });
  const events = [];
  q.onChange(() => events.push(q.length));
  q.enqueue(op('op-1'));
  q.enqueue(op('op-2'));
  q.dequeue('op-1');
  assert.deepEqual(events, [1, 2, 1]);
});

test('enqueue keeps full op payloads intact (no reshaping)', () => {
  const q = createQueue({ storage: fakeStorage(), accountSub: 'sub-a' });
  const source = op('op-1', 42);
  q.enqueue(source);
  assert.deepEqual(q.peekAll()[0], source);
});
