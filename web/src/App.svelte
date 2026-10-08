<script>
  import { entryAction } from './lib/entry.js';
  import { logoutAction } from './lib/logout.js';
  import { createPartySession } from './lib/party/session.js';
  import ImportPage from './lib/import/ImportPage.svelte';
  import PartyView from './lib/party/PartyView.svelte';
  import SheetView from './lib/sheet/SheetView.svelte';
  import ErrorState from './lib/sheet/components/ErrorState.svelte';
  import Skeleton from './lib/sheet/components/Skeleton.svelte';

  // NOTE: never name a runes-mode variable `state` — svelte-check's
  // transform trips over the name (TDZ-style false errors) and fails the
  // CI gate. `view` names it: which screen is on display. The machine is
  // party-rooted now (E10): probe → party (or its first-run states), the
  // sheet is a drill-in with a back affordance, never the boot target.
  // account mirrors the /api/me payload; only display_name is rendered.
  /** @type {any} */
  let account = $state(null);
  let view = $state('probing');

  // The party session: one roster fetch + one sync per logged-in tab.
  /** @type {any} */
  let session = $state(null);
  let offlineColdBoot = $state(false);
  let partyError = $state('');

  // The drill-in target: { payload, editable } (FR-3's rule below).
  /** @type {{ payload: any, editable: boolean } | null} */
  let sheetTarget = $state(null);

  // Entry flow (E3 Story 1 AC1): probe the session on load. A visitor
  // without one is sent to the login leg; a broken backend says so.
  async function probe() {
    view = 'probing';
    try {
      const response = await fetch('/api/me');
      if (response.ok) {
        account = await response.json();
        await bootParty();
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

  // The party boot (E10 FR-5/FR-9): one roster read; failure with a boot
  // cache cold-boots read-only (the session says so); failure without
  // one is a calm error with retry. An empty roster is a state, not an
  // error — the player sees the import CTA, the GM a note.
  async function bootParty() {
    view = 'party-loading';
    partyError = '';
    try {
      const made = await createPartySession({ account });
      session = made;
      offlineColdBoot = made.offlineColdBoot;
      view = made.roster.characters.length === 0 ? 'party-empty' : 'party';
    } catch (error) {
      partyError = error instanceof Error ? error.message : 'The server is unreachable right now.';
      view = 'party-error';
    }
  }

  // Import success (or a returning player): one fresh roster fetch, the
  // cache rewritten, the grid re-rendered — no reload, no invalidation
  // machinery beyond the one fetch (design D4).
  async function refreshParty() {
    try {
      const made = await session.refresh();
      session = { ...session, roster: made.roster };
      offlineColdBoot = false;
      view = made.roster.characters.length === 0 ? 'party-empty' : 'party';
    } catch (error) {
      partyError = error instanceof Error ? error.message : 'The server is unreachable right now.';
      view = 'party-error';
    }
  }

  // The drill-in (FR-3): every account opens every character; the sheet
  // is editable only for the owner AND a player — E3's rule, rendered.
  /** @param {number} id */
  function openCharacter(id) {
    const payload = session.roster.characters.find((/** @type {any} */ c) => c.character.id === id);
    if (!payload) return;
    sheetTarget = {
      payload,
      editable: account?.sub === payload.character.owner && session.roster.you.role === 'player',
    };
    view = 'sheet';
  }

  function backToParty() {
    sheetTarget = null;
    view = session?.roster?.characters.length ? 'party' : 'party-empty';
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
      session = null;
      sheetTarget = null;
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

{#if view === 'party-loading'}
  <Skeleton panes={3} />
{:else if view === 'party-error'}
  <ErrorState
    message="The party could not be loaded."
    detail={partyError}
    onretry={bootParty}
  />
{:else if (view === 'party' || view === 'party-empty') && session}
  <PartyView
    {session}
    {offlineColdBoot}
    onimport={() => (view = 'import')}
    onopenCharacter={openCharacter}
    onlogout={logout}
  />
{:else if view === 'sheet' && sheetTarget && session}
  <main>
    <button class="back" onclick={backToParty}>← The party</button>
    <SheetView
      character={sheetTarget.payload}
      accountSub={account?.sub ?? ''}
      editable={sheetTarget.editable}
      sync={session.sync}
      onlogout={logout}
    />
  </main>
{:else if view === 'import'}
  <main>
    <ImportPage />
    <button onclick={refreshParty}>Back to the party</button>
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
    <button onclick={bootParty}>The party</button>
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
    max-width: 72rem;
    margin: 2rem auto 0;
    padding: 0 1rem;
    text-align: center;
  }

  button {
    margin-top: 1rem;
    padding: 0.5rem 1.1rem;
    font-size: 0.95rem;
    color: #e6e6e6;
    background: #2b3245;
    border: 1px solid #465070;
    border-radius: 6px;
    cursor: pointer;
  }

  button:hover {
    background: #374060;
  }

  .back {
    display: block;
    margin: 0 0 1rem auto;
    text-align: left;
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
