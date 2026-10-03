<script>
  // Loading never blank-flashes: pane-shaped skeletons (design §7).
  let { panes = 3 } = $props();
  const heights = ['140px', '320px', '220px', '260px', '180px'];
</script>

<div class="board" role="status" aria-label="Loading your sheet">
  {#each Array.from({ length: panes }, (_, index) => index) as pane (pane)}
    <div class="panel">
      <div class="skeleton" style="width: 40%; height: 20px; margin-bottom: 10px;"></div>
      <div class="skeleton" style="width: 100%; height: {heights[pane % heights.length]};"></div>
    </div>
  {/each}
</div>
