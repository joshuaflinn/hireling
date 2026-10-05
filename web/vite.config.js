import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

// The backend serves the built bundle (web/dist) and owns all routing —
// no SvelteKit, no client router at this stage. The same config also drives
// the component tests: `defineConfig` from vitest/config adds the `test`
// block (jsdom everywhere; the issue names the fidelity default) while the
// svelte plugin below compiles .svelte in both build and test, and
// svelteTesting flips svelte's module conditions to the browser build so
// components mount client-side under jsdom (build output is untouched).
export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  test: {
    environment: 'jsdom',
    setupFiles: ['./tests/setup.js'],
  },
});
