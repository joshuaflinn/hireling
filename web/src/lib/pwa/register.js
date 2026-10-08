/* E10 registration (contracts/sw-shell-cache.md §6): register the shell
   service worker in production builds only, and reload exactly once when a
   new release takes the page over. Registration failure is logged and
   non-blocking — the app remains a plain web page; installability is an
   enhancement, never a gate. */

/**
 * Register `/sw.js` (scope `/`) when `env.PROD`, and implement the update
 * policy's page half (contract §4): on `controllerchange`, reload once —
 * and only if a controller existed at register time. A first install never
 * reloads. The reload is safe mid-session: E7's write queue and boot cache
 * are reload-durable.
 *
 * @param {{ PROD?: boolean }} [env] the vite env (default `import.meta.env`)
 * @param {ServiceWorkerContainer | undefined} [reg] the registration
 *   surface (default `navigator.serviceWorker`; absent in old browsers and
 *   under jsdom — both bail cleanly)
 * @param {() => void} [reload] the page reload (default `location.reload`;
 *   injectable so the policy is testable)
 */
export function registerPwa(
  env = import.meta.env,
  reg = typeof navigator !== 'undefined' ? navigator.serviceWorker : undefined,
  reload = () => location.reload(),
) {
  // Dev stays SW-free: the vite dev server is untouched day to day.
  if (!env.PROD || !reg) return;

  const hadController = Boolean(reg.controller);
  let reloaded = false;
  reg.addEventListener('controllerchange', () => {
    if (!hadController || reloaded) return;
    reloaded = true;
    reload();
  });

  reg.register('/sw.js', { scope: '/' }).catch((err) => {
    console.warn('[pwa] service worker registration failed:', err);
  });
}
