<script>
  // One stat tile (prototype `.tile`). `value` arrives pre-formatted.
  let { label, value, note = '', cls = '' } = $props();
</script>

<div class="tile {cls}">
  <div class="v">{value}</div>
  <div class="k">{label}</div>
  {#if note}<div class="x">{note}</div>{/if}
</div>
