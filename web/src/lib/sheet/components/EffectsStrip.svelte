<script>
  // The effects strip (spec §2.8): chips for the engine's effects targeting
  // this character — name, source, duration note — rendered verbatim from
  // `view.effects` (contract §3). Display-only conditions carry the
  // `tracked` badge (`tracked_manually`) so a zero-math condition never
  // looks like math. Hidden when the array is empty — no placeholder
  // theatre.
  //
  // E9: every chip is a ConditionTip trigger (name-match is the gate,
  // design D6 — the wire's chips carry no corpus link). `customConditions`
  // is the party's custom-row join, passed down by SheetView when the
  // custom read has landed: a chip whose name matches renders the creator's
  // description + the custom badge; anything else gets the honest fallback
  // popup. Additive — the chip markup/DOM is unchanged inside the wrapper.
  import ConditionTip from './ConditionTip.svelte';

  /** @type {{ effects?: any[], customConditions?: Array<{name: string,
    description: string, value_or_rank?: number | null}> }} */
  let { effects = [], customConditions = [] } = $props();

  const normalize = (/** @type {string} */ value) =>
    value.toLowerCase().replace(/\s+/g, ' ').trim();
  const customByName = $derived(
    new Map(customConditions.map((row) => [normalize(row.name), row])),
  );
  /** @param {string} name */
  const customFor = (name) => customByName.get(normalize(name)) ?? null;
</script>

{#if effects.length}
  <div class="strip" role="list" aria-label="Active effects">
    {#each effects as effect (effect.effect_id)}
      {@const custom = customFor(effect.name)}
      <!-- listitem stays on the OUTER node: the tip's trigger is a button,
        and a button's contents are presentational — the list's children must
        not live inside it (MOR-115 finding 4). -->
      <span role="listitem">
        <ConditionTip
          name={effect.name}
          customDescription={custom?.description ?? null}
          customValue={custom?.value_or_rank ?? null}
        >
          <span
            class="chip"
            class:manual={effect.tracked_manually}
            title={effect.duration_note
              ? `${effect.source_name} · ${effect.duration_note}`
              : effect.source_name}
          >
            {effect.name}<small>{effect.source_name}</small>{#if effect.tracked_manually}<em>tracked</em>{/if}
          </span>
        </ConditionTip>
      </span>
    {/each}
  </div>
{/if}

<style>
  .strip {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0 0 8px;
  }
  .chip {
    display: inline-flex;
    align-items: baseline;
    gap: 5px;
    border: 1px solid var(--gold-dim, #b8963e);
    background: var(--panel2, #161a21);
    border-radius: 999px;
    padding: 2px 9px;
    font-size: 12px;
    color: var(--gold, #d4af5f);
  }
  .chip small {
    color: var(--muted, #8a94a3);
    font-size: 10.5px;
  }
  .chip.manual {
    border-style: dashed;
  }
  .chip em {
    font-style: normal;
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--dim, #5c6672);
    border: 1px solid var(--muted, #8a94a3);
    border-radius: 4px;
    padding: 0 4px;
  }
</style>
