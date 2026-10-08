/* E10 shell-cache strategy (design D5/D6; contracts/sw-shell-cache.md §2–§3).
   Pure functions, no I/O, no framework imports — the emitted sw.js inlines
   this file verbatim at build time (web/plugins/sw-plugin.mjs), so every
   rule the worker runs is a rule these tests pin. */

/** The static, non-versioned shell files the SW owns. Built bundles live
    under /assets/ and are covered by the prefix below. */
const SHELL_FILES = [
  '/manifest.webmanifest',
  '/icon-192.png',
  '/icon-512.png',
  '/icon-maskable-512.png',
];

/** The shell cache prefix. One release ⇒ one cache:
    `hireling-shell-${buildId}`. */
const CACHE_PREFIX = 'hireling-shell-';

/**
 * Classify a same-origin request URL path.
 *
 * Order matters: an API navigation is still API. Returns 'asset'
 * (cache-first), 'navigation' (network-first with the cached shell as
 * fallback), or 'bypass' (never intercepted — every /api/* request and
 * WebSocket has one path: the app's).
 *
 * @param {string} pathname the request URL's pathname
 * @param {string} mode the request's mode ('navigate', 'cors', …)
 * @returns {'asset' | 'navigation' | 'bypass'} the fetch rule for this class
 */
export function classifyRequest(pathname, mode) {
  if (pathname === '/api' || pathname.startsWith('/api/')) return 'bypass';
  if (isShellAsset(pathname)) return 'asset';
  if (mode === 'navigate') return 'navigation';
  return 'bypass';
}

/**
 * The release's cache name.
 *
 * @param {string} buildId the content hash the build plugin computes
 * @returns {string} `hireling-shell-${buildId}`
 */
export function shellCacheName(buildId) {
  return `${CACHE_PREFIX}${buildId}`;
}

/**
 * From a dist file list, the URL paths to precache: the navigation shell
 * plus every shell asset. Anything else is never cached.
 *
 * @param {string[]} files dist file paths as emitted (e.g. 'index.html',
 *   'assets/index-a1b2c3.js')
 * @returns {string[]} URL paths (leading slash) for the PRECACHE list
 */
export function precacheList(files) {
  const paths = files.map(shellPath);
  return paths.filter((p) => p === '/index.html' || isShellAsset(p));
}

/**
 * Every old shell cache: same prefix, not this release's. Foreign caches
 * (other apps on the origin) are left alone.
 *
 * @param {string[]} cacheNames names from caches.keys()
 * @param {string} buildId this release's build id
 * @returns {string[]} the cache names to delete
 */
export function obsoleteCaches(cacheNames, buildId) {
  const current = shellCacheName(buildId);
  return cacheNames.filter((name) => name.startsWith(CACHE_PREFIX) && name !== current);
}

/**
 * @param {string} pathname
 * @returns {boolean}
 */
function isShellAsset(pathname) {
  return pathname.startsWith('/assets/') || SHELL_FILES.includes(pathname);
}

/**
 * @param {string} file a dist-relative file path
 * @returns {string} the URL path with a leading slash
 */
function shellPath(file) {
  return file.startsWith('/') ? file : `/${file.replace(/^\.\//, '')}`;
}
