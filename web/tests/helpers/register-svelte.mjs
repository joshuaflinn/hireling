// Registers the Svelte compile-on-load hook for `node --test`.
// The hook compiles .svelte imports as SERVER components (generate:
// 'server'), which `svelte/server`'s render() can execute in plain node —
// no DOM, no extra test dependencies. See svelte-hooks.mjs.
import { register } from 'node:module';

register('./svelte-hooks.mjs', import.meta.url);
