<script>
  // The strikes surface (spec §2.6) — the character's strikes, moved into
  // the sheet proper (deviation §9.3). Rows come from the adapter.
  import StrikeRow from './StrikeRow.svelte';

  /** @type {{ view: any }} */
  let { view } = $props();
  const strikes = $derived(view.derived.strikes ?? []);
</script>

<section class="panel" aria-label="Strikes">
  <h2>Strikes <small>MAP −5/−10, agile −4/−8</small></h2>
  {#each strikes as strike (strike.key)}
    <StrikeRow {strike} />
  {:else}
    <p class="meta">No strikes in this export.</p>
  {/each}
</section>
