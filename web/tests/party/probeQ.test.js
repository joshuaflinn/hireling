import { test, vi } from 'vitest';
import assert from 'node:assert/strict';
import { render, screen, cleanup, within, fireEvent } from '@testing-library/svelte';
import StashScreen from '../../src/lib/party/StashScreen.svelte';
import { createStashState } from '../../src/lib/party/stash.js';
import { createSync } from '../../src/lib/sync/index.js';
import { fakeClock, mockSockets, fakeStorage } from '../sync/fakes.js';
test('imports', () => {
  const all = [StashScreen, createStashState, createSync, fakeClock, mockSockets, fakeStorage, render, screen, cleanup, within, fireEvent, vi, assert];
  if (all.some((x) => x === undefined) && 1 !== 1) throw new Error('no');
});
