<script>
  // Feats & features (spec §2.7): feats level-grouped (names + categories
  // only — detail prose needs the feats corpus, E9's lane) and a features
  // tab with class features and ancestry & heritage from the export's
  // specials list.
  /** @type {{ baseSheet: any, editable?: boolean }} */
  let { baseSheet, editable = true } = $props();

  let tab = $state('feats');

  /** @type {Array<{ name: string, category: string | null, level: number }>} */
  const feats = $derived(
    (baseSheet.raw?.feats ?? []).map((/** @type {any[]} */ feat) => ({
      name: feat[0],
      category: typeof feat[2] === 'string' ? feat[2] : null,
      level: typeof feat[3] === 'number' ? feat[3] : 0,
    })),
  );
  const levels = $derived([...new Set(feats.map((feat) => feat.level))].sort((a, b) => a - b));
  const featsAt = $derived((/** @type {number} */ level) =>
    feats.filter((feat) => feat.level === level),
  );
  const specials = $derived(baseSheet.raw?.specials ?? []);
</script>

<section class="panel" aria-label="Feats and features">
  <h2>Feats &amp; Features</h2>
  {#if editable}
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === 'feats'} class:on={tab === 'feats'} onclick={() => (tab = 'feats')}>Feats</button>
      <button role="tab" aria-selected={tab === 'features'} class:on={tab === 'features'} onclick={() => (tab = 'features')}>Features</button>
    </div>
  {/if}

  {#if !editable || tab === 'feats'}
    <!-- view-only renders both lists stacked: no tabs, no lost information -->
    {#each levels as level (level)}
      <div class="rank-h">
        <span class="t">Level {level}</span>
        <span class="r">{featsAt(level).length}</span>
      </div>
      {#each featsAt(level) as feat (feat.name + feat.category)}
        <div class="row">
          <span class="nm">{feat.name}</span>
          {#if feat.category}<span class="chip">{feat.category}</span>{/if}
        </div>
      {/each}
    {:else}
      <p class="meta">No feats recorded.</p>
    {/each}
  {:else}
    {#each specials as feature (feature)}
      <div class="row"><span class="nm">{feature}</span></div>
    {:else}
      <p class="meta">No features recorded.</p>
    {/each}
  {/if}
  {#if !editable}
    <h3>Features</h3>
    {#each specials as feature (feature)}
      <div class="row"><span class="nm">{feature}</span></div>
    {:else}
      <p class="meta">No features recorded.</p>
    {/each}
  {/if}
</section>

<style>
  .tabs {
    display: flex;
    gap: 4px;
    margin: -2px 0 6px;
    flex-wrap: wrap;
  }
  .tabs button {
    background: none;
    border: 1px solid var(--muted);
    border-radius: 6px;
    padding: 3px 10px;
    color: var(--muted);
    font-size: 13px;
  }
  .tabs button.on {
    border-color: var(--gold-dim);
    color: var(--gold);
    background: var(--panel2);
  }
</style>
