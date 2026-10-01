// Module hooks: compile .svelte imports with the Svelte compiler in
// server mode so tests can render components with `svelte/server`'s
// render() inside the dependency-free node test runner.
//
// Server-mode compilation is the honest test seam: it runs the real
// component code (props, runes, markup, each/if blocks) without a DOM.
// Client-only behaviors ($effect, event handlers) do not run here — the
// interactions they drive are pinned where they live, in the state layer
// and util tests.

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { compile } from 'svelte/compiler';

/** @type {import('node:module').LoadHook} */
export async function load(url, context, nextLoad) {
  if (new URL(url).pathname.endsWith('.svelte')) {
    const source = await readFile(fileURLToPath(url), 'utf8');
    const compiled = compile(source, {
      filename: fileURLToPath(url),
      generate: 'server',
      runes: true,
    });
    return {
      format: 'module',
      source: compiled.js.code,
      shortCircuit: true,
    };
  }
  return nextLoad(url, context);
}
