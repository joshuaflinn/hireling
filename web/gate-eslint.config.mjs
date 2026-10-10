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
    },
  },
];
