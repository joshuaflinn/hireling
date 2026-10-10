// The grizzly-gate image's `node:eslint` pass, replicated bench-side so
// `just ci-local` cannot pass a tree the remote gate fails (it did — three
// pushes red on GitHub while the local recipe was green, until this file
// existed; see gate run 37850972131). The image lints with
// `--no-config-lookup` and its own flat config: ES builtins only — no
// browser env, no node env — so every platform global must be declared
// in-file (a `/* global */` header; convention: web/src/lib/sync/index.js).
//
// Calibrated against this tree: before the header fixes this config
// reproduced the image's 44 findings one-for-one, and flagged nothing
// extra. A gate-image digest bump that moves the rule set updates this
// file in the same PR.
//
// Second calibration, by fire (gate run 38080211231 red on `73512a9`
// while `ci-local` was green): the image also enforces `no-script-url`
// on JS and lints `.svelte` files — `svelte/no-unused-svelte-ignore`
// was the finding the js/mjs-only replica could never see. The svelte
// block carries the parser and the one rule the image has been observed
// to enforce on components; widening it further waits for observed
// findings, not speculation. eslint-plugin-svelte + svelte-eslint-parser
// are devDependencies for this replica alone.
import sveltePlugin from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';

export default [
  {
    files: ['**/*.js', '**/*.mjs'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
    },
    rules: {
      'no-undef': 'error',
      'no-unused-vars': 'error',
      'require-await': 'error',
      'no-new-func': 'error',
      'no-script-url': 'error',
    },
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteParser,
      ecmaVersion: 'latest',
      sourceType: 'module',
    },
    plugins: {
      svelte: sveltePlugin,
    },
    rules: {
      'svelte/no-unused-svelte-ignore': 'error',
    },
  },
];
