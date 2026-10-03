<script>
  // One stat tile (prototype `.tile`). `value` arrives pre-formatted.
  // `provenance` (contract §3 applied/suppressed) activates the hover when
  // the engine has something to say about the number.
  import Provenance from './Provenance.svelte';

  /** @type {{ label: string, value: *, note?: string, cls?: string,
    provenance?: { applied?: any[], suppressed?: any[] } }} */
  let { label, value, note = '', cls = '', provenance = undefined } = $props();
</script>

<div class="tile {cls}">
  <div class="v">{value}<Provenance applied={provenance?.applied} suppressed={provenance?.suppressed} /></div>
  <div class="k">{label}</div>
  {#if note}<div class="x">{note}</div>{/if}
</div>
