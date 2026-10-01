<script>
  // One spell slot row (spec §2.3): cast toggle → slot.used, prepared
  // spell name, and (when unprepared) the prep affordance whose keyboard
  // path is the caster's Dialog picker. Spent slots strike through;
  // pending echoes tint (degraded-mode §4).
  /** @type {{ row: any, editable?: boolean, offline?: boolean, oncast?: (used: boolean) => void, onprepare?: () => void }} */
  let { row, editable = true, offline = false, oncast, onprepare } = $props();

  const busy = $derived(offline || row.pending);
</script>

<div class="row" class:spent={row.used} class:pending={row.pending}>
  {#if editable}
    <button
      class="use"
      class:on={row.used}
      disabled={busy}
      aria-label={row.used
        ? `Mark ${row.prepared_spell ?? 'slot'} uncast`
        : `Cast ${row.prepared_spell ?? 'empty slot'}`}
      aria-pressed={row.used}
      onclick={() => oncast?.(!row.used)}
    >✓</button>
  {:else}
    <span class="use none"></span>
  {/if}
  <span class="nm">{row.prepared_spell ?? (row.rank === 0 ? '—' : 'open slot')}</span>
  {#if editable && !row.prepared_spell && row.rank > 0}
    <button class="chip slot" onclick={onprepare} disabled={busy}>Prepare…</button>
  {/if}
</div>
