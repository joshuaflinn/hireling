// @vitest-environment node
/* global URL */
// The bundle-copy drift check (E9 Task 10): web/src/lib/rules/notice.md is
// the build-time copy of the repo's NOTICE.md — the about view renders the
// copy, so the copy must never drift from the file the license verdict
// reads. Runs in the node environment: this file sits outside web/, where
// the jsdom suite's vite loader denies reads.

import { test } from 'vitest';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import bundledNotice from '../../src/lib/rules/notice.md?raw';

test('the bundled notice copy is the repo NOTICE.md, byte for byte', () => {
  // web/tests/rules/ -> web/tests/ -> web/ -> repo root
  const repoNotice = readFileSync(
    fileURLToPath(new URL('../../../NOTICE.md', import.meta.url)),
    'utf8',
  );
  assert.equal(
    bundledNotice,
    repoNotice,
    'the bundle copy drifts from NOTICE.md — update rules/notice.md with it',
  );
});
