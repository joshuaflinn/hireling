<script>
  import { entryAction } from './lib/entry.js';
  import { logoutAction } from './lib/logout.js';
  import ImportPage from './lib/import/ImportPage.svelte';
  import SyncDebug from './lib/sync/SyncDebug.svelte';
  import SheetView from './lib/sheet/SheetView.svelte';
  import EmptyState from './lib/sheet/components/EmptyState.svelte';
  import ErrorState from './lib/sheet/components/ErrorState.svelte';
  import Skeleton from './lib/sheet/components/Skeleton.svelte';

  // NOTE: never name a runes-mode variable `state` — svelte-check's
  // transform trips over the name (TDZ-style false errors) and fails the
  // CI gate. `view` names what it is: which screen is on display.
  // account mirrors the /api/me payload; only display_name is rendered.
  /** @type {{ sub?: string, display_name?: string } | null} */
  let account = $state(null);
  let view = $state('probing');

  // The bootstrap payload for the sheet (/api/characters/me).
  let character = $state(null);
  let sheetError = $state('');

  // Entry flow (E3 Story 1 AC1): probe the session on load. A visitor
  // without one is sent to the login leg; a broken backend says so.
  async function probe() {
    view = 'probing';
    try {
      const response = await fetch('/api/me');
      if (response.ok) {
        account = await response.json();
        loadSheet();
        return;
      }
      if (entryAction(response.status) === 'enter') {
        view = 'entering';
        window.location.assign('/api/auth/login');
        return;
      }
      view = 'offline';
    } catch {
      view = 'offline';
    }
  }

  // The sheet boot (spec §7): 204 → the designed empty state; a failed
  // read → calm error with retry; otherwise the live sheet renders. The
  // sync store's last-known state renders instantly when present — this
  // fetch is first boot only.
  async function loadSheet() {
    view = 'sheet-loading';
    sheetError = '';
    try {
      const response = await fetch('/api/characters/me');
      if (response.status === 204) {
        view = 'sheet-empty';
        return;
      }
      if (!response.ok) {
        sheetError = `The server answered ${response.status}.`;
        view = 'sheet-error';
        return;
      }
      character = await response.json();
      view = 'sheet';
    } catch {
      sheetError = 'The server is unreachable right now.';
      view = 'sheet-error';
    }
  }

  // Logout (E3 Story 6 AC3): a confirmed logout lands on a signed-out
  // screen with an explicit Sign in action — never an automatic probe, or
  // the probe's redirect would ride the surviving house IdP session right
  // back in. Anything unconfirmed keeps the signed-in view with an error.
  async function logout() {
    let status = null;
    try {
      const response = await fetch('/api/auth/logout', { method: 'POST' });
      status = response.status;
    } catch {
      status = null; // the request never completed; the session is unknown
    }
    if (logoutAction(status) === 'signed-out') {
      account = null;
      character = null;
      view = 'signed-out';
      return;
    }
    view = 'logout-failed';
  }

  function signIn() {
    view = 'entering';
    window.location.assign('/api/auth/login');
  }

  probe();
</script>

{#if view === 'sheet-loading'}
  <Skeleton panes={3} />
{:else if view === 'sheet-empty'}
  <EmptyState onimport={() => (view = 'import')} />
{:else if view === 'sheet-error'}
  <ErrorState message="Your sheet could not be loaded." detail={sheetError} onretry={loadSheet} />
{:else if view === 'sheet' && character}
  <SheetView {character} accountSub={account?.sub ?? ''} />
{:else if view === 'import'}
  <main>
    <ImportPage />
    <button onclick={loadSheet}>Back to the sheet</button>
  </main>
{:else if view === 'sync-debug' && character}
  <main>
    <SyncDebug sub={account?.sub ?? ''} />
    <button onclick={() => (view = 'sheet')}>Back</button>
  </main>
{:else if view === 'entering'}
  <main>
    <p class="status">Taking you to sign in…</p>
  </main>
{:else if view === 'signed-out'}
  <main>
    <p class="status">You are signed out.</p>
    <button onclick={signIn}>Sign in</button>
  </main>
{:else if view === 'signed-in' || view === 'logout-failed'}
  <main>
    {#if view === 'logout-failed'}
      <p class="error">Logging out failed — the session is still live. Try again.</p>
    {/if}
    <p class="status">Signed in as {account?.display_name}</p>
    <button onclick={loadSheet}>Open the sheet</button>
    <button onclick={logout}>Log out</button>
  </main>
{:else if view === 'offline'}
  <main>
    <p class="status">The server is unreachable right now.</p>
    <button onclick={probe}>Try again</button>
  </main>
{:else}
  <main>
    <p class="status">Checking your session…</p>
  </main>
{/if}

<style>
  :global(body) {
    margin: 0;
    background: var(--bg, #111317);
    color: var(--text, #e9e5d9);
    font-family: system-ui, sans-serif;
  }

  main {
    max-width: 40rem;
    margin: 20vh auto 0;
    text-align: center;
  }

  h1 {
    font-size: 3rem;
    letter-spacing: 0.05em;
    margin-bottom: 0.25rem;
  }

  .tagline {
    color: #9aa4b2;
  }

  button {
    margin-top: 2rem;
    padding: 0.6rem 1.4rem;
    font-size: 1rem;
    color: #e6e6e6;
    background: #2b3245;
    border: 1px solid #465070;
    border-radius: 6px;
    cursor: pointer;
  }

  button:hover {
    background: #374060;
  }

  .status {
    margin-top: 1rem;
    color: #9aa4b2;
    font-family: ui-monospace, monospace;
    font-size: 0.9rem;
  }

  .error {
    margin-top: 1rem;
    color: #e07a6a;
    font-family: ui-monospace, monospace;
    font-size: 0.9rem;
  }
</style>
