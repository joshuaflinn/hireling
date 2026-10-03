<script>
  // The `syncing…` indicator (spec §6): visible iff the QUEUE is non-empty
  // (never connection state), subtle chrome, ≤1 Hz pulse, neutral colour.
  let { syncing = false } = $props();
</script>

{#if syncing}
  <span class="syncstat syncing-dot" role="status">syncing…</span>
{/if}
