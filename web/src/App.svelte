<script>
  import { entryAction } from './lib/entry.js';

  let account = $state(null);
  let state = $state('probing');

  // Entry flow (E3 Story 1 AC1): probe the session on load. A visitor
  // without one is sent to the login leg; a broken backend says so.
  async function probe() {
    state = 'probing';
    try {
      const response = await fetch('/api/me');
      if (response.ok) {
        account = await response.json();
        state = 'signed-in';
        return;
      }
      if (entryAction(response.status) === 'enter') {
        state = 'entering';
        window.location.assign('/api/auth/login');
        return;
      }
      state = 'offline';
    } catch {
      state = 'offline';
    }
  }

  async function logout() {
    await fetch('/api/auth/logout', { method: 'POST' });
    account = null;
    probe();
  }

  probe();
</script>

<main>
  <h1>Hireling</h1>
  <p class="tagline">Party-linked PF2e character tracking. The sheet is being built.</p>
  {#if state === 'entering'}
    <p class="status">Taking you to sign in…</p>
  {:else if state === 'signed-in'}
    <p class="status">Signed in as {account.display_name}</p>
    <button onclick={logout}>Log out</button>
  {:else if state === 'offline'}
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
</style>
