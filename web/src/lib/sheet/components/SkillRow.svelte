<script>
  // One skill or lore row: rank letter + name + signed modifier, with the
  // row's provenance hover when the engine has modifiers on it.
  import Provenance from './Provenance.svelte';

  /** @type {{ name: string, rankLetter: string, rank: number, modifier: string,
    untrained?: boolean, wide?: boolean,
    provenance?: { applied?: any[], suppressed?: any[] } }} */
  let {
    name,
    rankLetter,
    rank,
    modifier,
    untrained = false,
    wide = false,
    provenance = undefined,
  } = $props();
</script>

<div class="sk" class:untr={untrained} class:wide>
  <span class="rk r{rank}">{rankLetter}</span>
  <span class="n">{name}</span>
  <span class="m">{modifier}<Provenance applied={provenance?.applied} suppressed={provenance?.suppressed} /></span>
</div>
