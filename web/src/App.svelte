<script>
  import { entryAction } from './lib/entry.js';
  import { logoutAction } from './lib/logout.js';

  // NOTE: never name a runes-mode variable `state` — svelte-check's
  // transform trips over the name (TDZ-style false errors) and fails the
  // CI gate. `view` names what it is: which screen is on display.
  // account mirrors the /api/me payload; only display_name is rendered.
  /** @type {{ display_name?: string } | null} */
  let account = $state(null);
  let view = $state('probing');

  // Entry flow (E3 Story 1 AC1): probe the session on load. A visitor
  // without one is sent to the login leg; a broken backend says so.
  async function probe() {
    view = 'probing';
    try {
      const response = await fetch('/api/me');
      if (response.ok) {
        account = await response.json();
        view = 'signed-in';
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

<main>
  <h1>Hireling</h1>
  <p class="tagline">Party-linked PF2e character tracking. The sheet is being built.</p>
  {#if view === 'entering'}
    <p class="status">Taking you to sign in…</p>
  {:else if view === 'signed-out'}
    <p class="status">You are signed out.</p>
    <button onclick={signIn}>Sign in</button>
  {:else if view === 'signed-in' || view === 'logout-failed'}
    {#if view === 'logout-failed'}
      <p class="error">Logging out failed — the session is still live. Try again.</p>
    {/if}
    <p class="status">Signed in as {account?.display_name}</p>
    <button onclick={logout}>Log out</button>
  {:else if view === 'offline'}
    <p class="status">The server is unreachable right now.</p>
    <button onclick={probe}>Try again</button>
  {:else}
    <p class="status">Checking your session…</p>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    background: #0f1117;
    color: #e6e6e6;
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
