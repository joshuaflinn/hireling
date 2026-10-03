<script>
  // One strike row (spec §2.6): name, attack, the MAP row (−5/−10, agile
  // −4/−8), damage dice, trait names — all display fields straight off the
  // wire's strike object. Trait *text* popups are E9. Attack and flat
  // damage carry their provenance hovers.
  import Provenance from './Provenance.svelte';
  import { signed } from '../../engine/format.js';

  /** @type {{ strike: any }} */
  let { strike } = $props();

  const agile = $derived(strike.map === 4);
</script>

<div class="strike">
  <div class="strike-h">
    <span class="nm">{strike.label}</span>
    <span class="pill gold">atk {signed(strike.attack.total)}<Provenance applied={strike.attack.applied} suppressed={strike.attack.suppressed} /></span>
    <span class="q">MAP {signed(-strike.map)} / {signed(-2 * strike.map)}</span>
  </div>
  <div class="strike-d">
    <span class="q">damage {strike.damage_expr} {strike.damage_type_name}<Provenance applied={strike.damage_flat.applied} suppressed={strike.damage_flat.suppressed} /></span>
    {#each strike.traits as trait (trait)}
      <span class="chip">{trait}</span>
    {/each}
  </div>
</div>

<style>
  .strike {
    padding: 4px 6px;
    border-radius: 6px;
    border-bottom: 1px dotted #2a2f38;
  }
  .strike-h {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .strike-h .nm {
    flex: 1;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .strike-d {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
    flex-wrap: wrap;
  }
</style>
