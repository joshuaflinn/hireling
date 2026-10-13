<script>
  // Provenance hover (E6 design §5, contract §3 rules): a number's full
  // math on hover AND focus — every applied modifier, then the sources
  // that did not stack, each with the engine's own reason text, verbatim
  // (suppressed entries are engine output, not UI inference). Focusable so
  // keyboard users get the same breakdown; Escape dismisses; renders
  // nothing when both lists are empty — no hover theatre on numbers the
  // engine left untouched.
  import { signed } from '../../engine/format.js';
  import ConditionTip from './ConditionTip.svelte';

  /** @type {{ applied?: any[], suppressed?: any[] }} */
  let { applied = [], suppressed = [] } = $props();

  const count = $derived(applied.length + suppressed.length);

  /** @param {any[]} list */
  const describe = (list) =>
    list.map((e) => `${signed(e.value)} ${e.type} — ${e.effect_name}`).join('; ');
</script>

{#if count > 0}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <!-- Reason (design §9): the hover is keyboard-reachable by design — the
  tabindex + Escape-blur pair is the a11y affordance and the aria-label
  carries the full applied/suppressed breakdown; dropping either would
  silence screen-reader and keyboard users, not just the linter. -->
  <span
    class="prov"
    tabindex="0"
    role="note"
    aria-label="Applied: {describe(applied) || 'none'}. Not stacked: {suppressed
      .map((e) => `${signed(e.value)} ${e.type} — ${e.effect_name} · ${e.reason}`)
      .join('; ') || 'none'}."
    onkeydown={(e) => {
      if (e.key === 'Escape') e.currentTarget?.blur();
    }}
  >
    <sup class="mark" aria-hidden="true">•</sup>
    <span class="card">
      {#if applied.length}
        <b>Applied</b>
        <ul>
          {#each applied as entry (entry.effect_id)}
            <li>{signed(entry.value)} {entry.type} — <ConditionTip name={entry.effect_name} /></li>
          {/each}
        </ul>
      {/if}
      {#if suppressed.length}
        <b>Not stacked</b>
        <ul>
          {#each suppressed as entry (entry.effect_id)}
            <li>{signed(entry.value)} {entry.type} — <ConditionTip name={entry.effect_name} /> · {entry.reason}</li>
          {/each}
        </ul>
      {/if}
    </span>
  </span>
{/if}

<style>
  .prov {
    position: relative;
    cursor: help;
    outline: none;
  }
  .mark {
    color: var(--gold-dim, #b8963e);
    font-size: 0.8em;
    margin-left: 1px;
  }
  .card {
    display: none;
    position: absolute;
    z-index: 30;
    left: 0;
    top: 100%;
    margin-top: 4px;
    min-width: 220px;
    max-width: 280px;
    background: var(--panel2, #161a21);
    border: 1px solid var(--gold-dim, #b8963e);
    border-radius: 8px;
    padding: 7px 10px;
    font-size: 12.5px;
    color: var(--text, #e8e4d8);
    box-shadow: 0 6px 18px rgb(0 0 0 / 45%);
    text-align: left;
    white-space: normal;
  }
  .card b {
    display: block;
    margin: 2px 0;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--muted, #8a94a3);
  }
  .card ul {
    margin: 0 0 4px;
    padding-left: 14px;
  }
  .prov:hover .card,
  .prov:focus-within .card,
  .prov:focus .card {
    display: block;
  }
</style>
