<script>
  // One skill or lore row: rank letter + name + signed modifier.
  let { name, rankLetter, rank, modifier, untrained = false, wide = false } = $props();
</script>

<div class="sk" class:untr={untrained} class:wide>
  <span class="rk r{rank}">{rankLetter}</span>
  <span class="n">{name}</span>
  <span class="m">{modifier}</span>
</div>
