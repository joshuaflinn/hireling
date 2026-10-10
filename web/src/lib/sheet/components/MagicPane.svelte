<script>
  // The magic pane (spec §2.3): per-caster blocks with slots, cantrips at
  // heightened rank, the spellbook, prep reset, and the Staff Nexus panel.
  // Tab shell: Spells | Pet & Minions (the companions tab renders in Task
  // 10). Every number rides the adapter; every write rides sheet/state.js.
  import CasterPanel from './CasterPanel.svelte';
  import StaffPanel from './StaffPanel.svelte';
  import { findOpError } from '../state.js';

  /** @type {{ baseSheet: any, slots: any[], view: any, daily: any,
    opErrors?: any[], editable?: boolean, offline?: boolean,
    customSpells?: any[],
    oncast?: (casterKey: string, row: any, used: boolean) => void,
    onprepare?: (casterKey: string, row: any, spell: string) => void,
    onreset?: (casterKey: string) => void,
    onaddcustom?: () => void,
    ondaily?: (daily: any) => void, companionsSlot?: import('svelte').Snippet }} */
  let {
    baseSheet,
    slots,
    view,
    daily,
    opErrors = [],
    editable = true,
    offline = false,
    customSpells = [],
    oncast,
    onprepare,
    onreset,
    onaddcustom,
    ondaily,
    companionsSlot,
  } = $props();

  let tab = $state('spells');

  // The focus section (spec §2.3 — review finding 11): the export's
  // `focus[tradition][ability]` block carries focus cantrips and focus
  // spells; rendered with the caster whose tradition it belongs to.
  const focusFor = (/** @type {any} */ caster) => {
    const byAbility = baseSheet.focus?.[caster.magic_tradition ?? ''] ?? {};
    const entry =
      byAbility[caster.ability ?? ''] ?? Object.values(byAbility)[0] ?? null;
    return /** @type {any} */ (entry)?.focusSpells ?? [];
  };
</script>

<section class="panel" aria-label="Magic">
  <h2>Magic <small>{baseSheet.identity.class ?? ''} spellcasting</small></h2>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === 'spells'} class:on={tab === 'spells'} onclick={() => (tab = 'spells')}>Spells</button>
    <button role="tab" aria-selected={tab === 'companions'} class:on={tab === 'companions'} onclick={() => (tab = 'companions')}>Pet &amp; Minions</button>
  </div>

  {#if tab === 'spells'}
    {#each baseSheet.spellcasters as caster (caster.caster_key)}
      <CasterPanel
        {caster}
        slots={slots.filter((/** @type {any} */ slot) => slot.caster_key === caster.caster_key)}
        numbers={view?.derived ?? null}
        cantripRank={view?.render_base?.cantrip_rank ?? null}
        {editable}
        {offline}
        known={caster.known}
        focusSpells={focusFor(caster)}
        {customSpells}
        {onaddcustom}
        opErrors={opErrors.filter(
          (/** @type {any} */ error) => error.target?.kind === 'slot' && error.target?.caster_key === caster.caster_key,
        )}
        oncast={(/** @type {any} */ row, /** @type {boolean} */ used) => oncast?.(caster.caster_key, row, used)}
        onprepare={(/** @type {any} */ row, /** @type {string} */ spell) => onprepare?.(caster.caster_key, row, spell)}
        onreset={() => onreset?.(caster.caster_key)}
      />
    {:else}
      <p class="meta">No spellcasting in this export.</p>
    {/each}

    <h3>Staff Nexus</h3>
    <StaffPanel {daily} {editable} {offline} opError={findOpError(opErrors, 'vitals', { field: 'daily' })} onchange={(next) => ondaily?.(next)} />
  {:else if companionsSlot}
    {@render companionsSlot()}
  {:else}
    <p class="meta">No pet or familiar panel wired.</p>
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
