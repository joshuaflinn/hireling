<script>
  // RosterCard (E10 design D3): E6's units composed into one party-screen
  // card — view-only CharacterHeader line, the effects strip fed from the
  // engine seam ($view.effects, the SAME source the sheet's chips render
  // from), the HP bar, plus card chrome (portrait initial, down/full
  // state) that no existing unit renders.
  //
  // The whole card is a button: the tap is NAVIGATION (the drill-in), not
  // an edit. Every inner unit renders editable={false} — the GM seat gets
  // zero edit affordances by construction, and a player's card offers no
  // write surface either; writes happen in the owner's sheet.
  import CharacterHeader from '../sheet/components/CharacterHeader.svelte';
  import HpBar from '../sheet/components/HpBar.svelte';
  import EffectsStrip from '../sheet/components/EffectsStrip.svelte';
  import SyncIndicator from '../sheet/components/SyncIndicator.svelte';

  /** @type {{ card: any, editable?: boolean, onopen?: () => void }} */
  let { card, editable = false, onopen } = $props();

  // Store members through $derived: the auto-subscriptions re-establish
  // when the roster refreshes and hands the card a new state object.
  const hp = $derived(card.hp);
  const tempHp = $derived(card.tempHp);
  const hpMax = $derived(card.hpMax);
  const effects = $derived(card.effects);
  const syncing = $derived(card.syncing);

  // down/full are renders of store state, not tracked anywhere (data-model §5)
  const down = $derived($hp.value === 0);
  const full = $derived($hpMax !== null && $hp.value === $hpMax);
  // Portrait initial: first character of the identity name; a missing name
  // renders the neutral placeholder, never a crash (spec edge case).
  const initial = $derived((card.character.name?.[0] ?? '?').toUpperCase());
</script>

<button class="card" class:down class:full onclick={onopen}>
  <span class="avatar" aria-hidden="true">{initial}</span>
  <CharacterHeader
    name={card.character.name}
    subline={card.character.class ?? ''}
    level={card.character.level}
    editable={false}
  />
  {#if editable}
    <SyncIndicator syncing={$syncing} />
  {/if}
  {#if $hpMax === null}
    <!-- Engine output not on the wire yet: a skeleton bar, no placeholder
         numbers — the sheet's loading state, kept honest here too. -->
    <div class="skeleton bar-loading" role="status" aria-label="Hit points loading"></div>
  {:else}
    <HpBar hp={$hp} temp={$tempHp} max={$hpMax} editable={false} offline={false} />
  {/if}
  <EffectsStrip effects={$effects ?? []} />
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
    padding: 1rem;
    text-align: left;
    color: inherit;
    background: var(--panel, #1a1f2b);
    border: 1px solid var(--edge, #2c3444);
    border-radius: 10px;
    cursor: pointer;
    font: inherit;
  }

  .card:hover {
    border-color: var(--gold, #c8a84b);
  }

  .card.down {
    border-color: var(--danger, #b0492f);
  }

  .card.full {
    border-color: var(--ok, #4f7d4f);
  }

  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 50%;
    background: var(--gold, #c8a84b);
    color: var(--bg, #10141b);
    font-weight: 700;
  }

  .bar-loading {
    height: 0.9rem;
    width: 100%;
  }
</style>
