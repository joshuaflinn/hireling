<script>
  // PartyView (E10 design D3/D4): the roster grid — one RosterCard per
  // party character over the session sync, plus the first-run states.
  // Renders nothing writable: the header's only affordances are the
  // player's import CTA (role- and ownership-ruled) and logout; every
  // card tap is navigation into the drill-in.
  import { createRosterState } from './state.js';
  import RosterCard from './RosterCard.svelte';
  import EmptyState from '../sheet/components/EmptyState.svelte';

  /** @type {{ session: any, offlineColdBoot?: boolean, onimport?: () => void,
      onopenCharacter?: (id: number) => void, onlogout?: () => void }} */
  let { session, offlineColdBoot = false, onimport, onopenCharacter, onlogout } = $props();

  const you = $derived(session.roster.you);
  const empty = $derived(session.roster.characters.length === 0);
  const hasOwn = $derived(
    session.roster.characters.some((/** @type {any} */ c) => c.character.owner === you.sub),
  );
  // The import affordance: players without a character (US3); the GM
  // never sees it (FR-4).
  const canImport = $derived(you.role === 'player' && !hasOwn);
  // FR-3's rendering hint for the drill-in; here it only decides whether
  // the owner's own card shows the sync indicator (the queue is local).
  const ownSub = $derived(you.sub);

  // Rebuilt when the roster refreshes (import → refresh); the previous
  // state's sync subscription is released on swap.
  const rosterState = $derived.by(() =>
    createRosterState({ sync: session.sync, roster: session.roster }),
  );
  $effect(() => {
    const current = rosterState;
    return () => current.destroy();
  });
</script>

{#if empty}
  {#if canImport}
    <EmptyState onimport={onimport} />
  {:else}
    <main>
      <p class="status" role="status">The party has no characters yet.</p>
      <button onclick={onlogout}>Log out</button>
    </main>
  {/if}
{:else}
  <main>
    <header class="party-head">
      <h1>The Party</h1>
      <div class="actions">
        {#if canImport}
          <button onclick={onimport}>Import your character</button>
        {/if}
        <button onclick={onlogout}>Log out</button>
      </div>
    </header>
    {#if offlineColdBoot}
      <p class="offline-note" role="status">
        Offline — showing the party as it was last known. Writes wait for the link.
      </p>
    {/if}
    <div class="grid" role="list" aria-label="Party roster">
      {#each rosterState.cards as card (card.character.id)}
        <div role="listitem">
          <RosterCard
            {card}
            editable={you.role === 'player' && card.character.owner === ownSub}
            onopen={() => onopenCharacter?.(card.character.id)}
          />
        </div>
      {/each}
    </div>
  </main>
{/if}

<style>
  h1 {
    font-size: 3rem;
    letter-spacing: 0.05em;
    margin-bottom: 0.25rem;
  }

  .party-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
  }

  .offline-note {
    margin: 0 0 1rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--edge, #2c3444);
    border-left: 3px solid var(--gold, #c8a84b);
    border-radius: 6px;
    color: var(--muted, #9aa4b2);
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr));
    gap: 1rem;
  }

  .status {
    margin-top: 1rem;
    color: var(--muted, #9aa4b2);
    font-family: ui-monospace, monospace;
  }

  button {
    padding: 0.5rem 1.1rem;
    font-size: 0.95rem;
    color: var(--text, #e6e6e6);
    background: var(--panel, #2b3245);
    border: 1px solid var(--edge, #465070);
    border-radius: 6px;
    cursor: pointer;
  }
</style>
