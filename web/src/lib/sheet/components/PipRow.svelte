<script>
  // One pip row (focus points, hero points — the Q1 tracked fields).
  // A pip click writes the COUNT of lit pips (absolute value, CAS-friendly);
  // values live in the aria-labels, colour never carries sole meaning.
  /** @type {{ label: string, current?: number, max?: number, editable?: boolean,
    disabled?: boolean, onset?: (value: number) => void }} */
  let {
    label,
    current = 0,
    max = 0,
    editable = true,
    disabled = false,
    onset,
  } = $props();

  const pips = $derived(Array.from({ length: max }, (_, index) => index + 1));
  const interactive = $derived(editable && !disabled && max > 0);
</script>

<div class="trk">
  <span class="k">{label}</span>
  <span class="pips" role="group" aria-label="{label}: {current} of {max}">
    {#each pips as pip (pip)}
      {#if interactive}
        <button
          class="pip"
          class:on={pip <= current}
          aria-label="{label} {pip} of {max}"
          aria-pressed={pip <= current}
          onclick={() => (pip === current ? onset?.(pip - 1) : onset?.(pip))}
        ></button>
      {:else}
        <!-- view-only: no controls, just the lit/unlit pips (design §6) -->
        <span class="pip" class:on={pip <= current} aria-hidden="true"></span>
      {/if}
    {/each}
  </span>
</div>
