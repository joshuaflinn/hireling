<script>
  // The pet & familiars panel (spec §2.4) — the prototype's pet card,
  // ported: HP 5×level, the owner's AC and saves, 3+level for Perception/
  // Acrobatics/Stealth, level for other skills, abilities from the export,
  // and the pet-vs-familiar command note. Companions render read-only —
  // minion HP tracking is parked (Q4 ruling). No summons browser.
  import { signed } from '../../engine/format.js';

  /** @type {{ baseSheet: any, view: any }} */
  let { baseSheet, view } = $props();

  const companions = $derived(baseSheet.companions ?? []);
  const level = $derived(view.level);
  /** The Pet feat (not the Familiar) makes it a pet: 3+level skills, the pet note. */
  const isPet = $derived(
    (baseSheet.raw?.feats ?? []).some((/** @type {any[]} */ feat) => feat[0] === 'Pet') &&
      !(baseSheet.raw?.feats ?? []).some((/** @type {any[]} */ feat) => feat[0] === 'Familiar'),
  );
  const petLine = $derived(3 + level);
  const otherSkills = $derived(level);
  const speed = $derived(
    companions.some((/** @type {any} */ c) =>
      (c.abilities ?? []).some((/** @type {string} */ ability) => /^Fast Movement/.test(ability)),
    )
      ? 40
      : 25,
  );
</script>

{#if companions.length === 0}
  <p class="meta">No pet or familiar in this export.</p>
{:else}
  {#each companions as companion (companion.name ?? 'companion')}
    <div class="fam">
      <div class="hd">
        <b>{(companion.name ?? 'Familiar').replace(/^Familiar \((.*)\)$/, '$1')}</b>
        <small>{isPet ? 'Pet' : (companion.kind ?? 'Familiar')} {level} · Minion</small>
      </div>
      <div class="pet-line">
        <span>HP<b>{5 * level}</b></span>
        <span>AC<b>{view.derived.ac.total}</b></span>
        <span>Spd<b>{speed}</b></span>
        <span>Fort<b>{signed(view.derived.fort.total)}</b></span>
        <span>Ref<b>{signed(view.derived.ref.total)}</b></span>
        <span>Will<b>{signed(view.derived.will.total)}</b></span>
      </div>
      <div class="pet-list">
        <p><b>Perception</b> {signed(petLine)} · low-light vision</p>
        <p><b>Acrobatics</b> {signed(petLine)} · <b>Stealth</b> {signed(petLine)} · other skills {signed(otherSkills)}</p>
        {#if (companion.abilities ?? []).length}
          {#each companion.abilities as ability (ability)}
            <p><b>{ability}</b></p>
          {/each}
        {:else}
          <p><b>Abilities</b> <span style="color:var(--dim)">none recorded</span></p>
        {/if}
      </div>
      <div class="pet-note">
        {isPet
          ? 'Command an Animal (1 action, no Nature check) gives it 2 actions. No attack actions except Escape and Force Open.'
          : 'Empathic link within 1 mile. Command it (1 action) to give it 2 actions.'}
      </div>
      <div class="pet-src">
        {isPet
          ? `Pet feat: HP 5 per level, your AC and saves, 3 + level for Perception, Acrobatics and Stealth.`
          : `HP 5 per level, your AC and saves, level + spellcasting modifier for Perception, Acrobatics and Stealth.`}
      </div>
    </div>
  {/each}
{/if}

<style>
  .fam {
    margin-top: 12px;
    background: var(--panel2);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 8px 10px;
    font-size: 13px;
  }
  .fam .hd {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    border-bottom: 2px solid var(--line2);
    padding-bottom: 3px;
    margin-bottom: 5px;
  }
  .fam .hd b {
    font-family: var(--serif);
    font-size: 16px;
    color: var(--gold);
    font-weight: normal;
  }
  .fam p {
    margin: 3px 0;
  }
  .pet-line {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 2px 8px;
    padding: 5px 8px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    font-size: 13px;
    white-space: nowrap;
  }
  .pet-line span {
    color: var(--muted);
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }
  .pet-line b {
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    margin-left: 3px;
  }
  .pet-note {
    color: var(--muted);
    font-size: 12.5px;
    margin-top: 6px;
  }
  .pet-src {
    color: var(--dim);
    font-size: 11.5px;
    margin-top: 4px;
  }
</style>
