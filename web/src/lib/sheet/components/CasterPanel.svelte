<script>
  // One caster block (spec §2.3): header pills (tradition, type, ability,
  // spell attack / DC from the adapter), prepared slots by rank with cast
  // tracking, cantrips at heightened rank, innate rows (locked open — an
  // innate cantrip is always prepared), the spellbook collapsible, and
  // reset-to-export prep. Prep is keyboard-first: the picker is a Dialog
  // (util/keyboard.js discipline); drag-to-prepare has no place here.
  import Dialog from './Dialog.svelte';
  import SpellSlotRow from './SpellSlotRow.svelte';
  import { ordinal, signed } from '../../engine/format.js';

  /** @type {{ caster: any, slots: any[], numbers: any, cantripRank: number,
    editable?: boolean, offline?: boolean, known?: any[], focusSpells?: string[],
    opErrors?: any[],
    oncast?: (row: any, used: boolean) => void,
    onprepare?: (row: any, spell: string) => void,
    onreset?: () => void }} */
  let {
    caster,
    slots,
    numbers,
    cantripRank,
    editable = true,
    offline = false,
    known = [],
    focusSpells = [],
    opErrors = [],
    oncast,
    onprepare,
    onreset,
  } = $props();

  /** @type {{ rank: number, index: number } | null} */
  let picking = $state(null);
  let bookOpen = $state(false);

  const ranks = $derived(
    [...new Set(slots.map((slot) => slot.rank))].sort((a, b) => a - b),
  );
  const slotsAt = $derived((/** @type {number} */ rank) =>
    slots.filter((slot) => slot.rank === rank).sort((a, b) => a.slot_index - b.slot_index),
  );
  const attack = $derived(
    numbers?.casters?.find((/** @type {any} */ c) => c.caster_key === caster.caster_key) ?? null,
  );
  const preparedNames = $derived(new Set(slots.map((slot) => slot.prepared_spell).filter(Boolean)));
  const bookAt = $derived((/** @type {number} */ rank) =>
    (known.find((list) => list.rank === rank)?.spells ?? []).filter(
      (/** @type {string} */ spell) => !preparedNames.has(spell),
    ),
  );

  function commitPick(/** @type {string} */ spell) {
    if (!picking) return;
    const target = picking;
    picking = null;
    onprepare?.(
      { rank: target.rank, slot_index: target.index },
      spell,
    );
  }

  /** The inline rejection for one slot row, if any (spec §6). */
  const rowError = (/** @type {any} */ row) =>
    opErrors.find(
      (/** @type {any} */ error) =>
        error.target?.rank === row.rank &&
        error.target?.slot_index === row.slot_index,
    ) ?? null;
</script>

<section class="caster" aria-label="{caster.caster_key} spellcasting">
  <div class="caster-h">
    <span class="cname">{caster.caster_key}</span>
    <span class="pill">{caster.magic_tradition ?? '—'}</span>
    <span class="pill">{caster.spellcasting_type ?? '—'}</span>
    <span class="pill">{(caster.ability ?? '').toUpperCase()}</span>
    {#if attack}
      <span class="pill gold">atk {signed(attack.spell_attack.total)} · DC {attack.spell_dc.total}</span>
    {/if}
    {#if editable && onreset}
      <button class="btn" style="font-size:12px;padding:2px 9px" onclick={onreset} disabled={offline}>
        Reset prep to export
      </button>
    {/if}
  </div>

  {#each ranks as rank (rank)}
    <div class="rank-h">
      <span class="t">{rank === 0 ? 'Cantrips' : `${ordinal(rank)} rank`}</span>
      <span class="s">{rank === 0 ? `heightened to rank ${cantripRank}` : ''}</span>
      <span class="r">{slotsAt(rank).filter((slot) => !slot.used).length}/{slotsAt(rank).length} open</span>
    </div>
    {#each slotsAt(rank) as row (row.slot_index)}
      <SpellSlotRow
        {row}
        {editable}
        {offline}
        oncast={(used) => oncast?.(row, used)}
        onprepare={() => (picking = { rank: row.rank, index: row.slot_index })}
      />
      {#if rowError(row)}
        <p class="op-error" role="alert">{rowError(row).reason}</p>
      {/if}
    {/each}
  {/each}

  {#if focusSpells.length}
    <div class="rank-h" style="margin-top:8px">
      <span class="t">Focus spells</span>
    </div>
    {#each focusSpells as spell (spell)}
      <div class="row">
        <span></span>
        <span class="nm">{spell}</span>
        <span></span>
      </div>
    {/each}
  {/if}

  {#if caster.innate}
    <p class="meta" style="margin-top:4px">Innate spells are always prepared — nothing to track but the cast.</p>
  {/if}

  {#if known.length}
    <details class="book" bind:open={bookOpen}>
      <summary>Spellbook — known, not prepared</summary>
      {#each known.filter((/** @type {any} */ list) => bookAt(list.rank).length) as list (list.rank)}
        <div class="rank-h">
          <span class="t">{list.rank === 0 ? 'Cantrips' : `${ordinal(list.rank)} rank`}</span>
        </div>
        {#each bookAt(list.rank) as spell (spell)}
          <div class="row">
            <span></span>
            <span class="nm">{spell}</span>
            {#if editable && !offline}
              <button
                class="chip book"
                onclick={() => onprepare?.({ rank: list.rank, slot_index: -1 }, spell)}
              >Prepare</button>
            {/if}
          </div>
        {/each}
      {/each}
    </details>
  {/if}
</section>

<Dialog open={picking !== null} title={`Prepare — ${ordinal(picking?.rank ?? 0)} rank`} onclose={() => (picking = null)}>
  <p style="color:var(--muted);font-size:13px">Pick a known spell; Tab moves, Enter or click commits.</p>
  <div style="display:flex;flex-direction:column;gap:2px;max-height:50vh;overflow:auto">
    {#each (known.find((/** @type {any} */ list) => list.rank === picking?.rank)?.spells ?? []) as spell (spell)}
      <button class="btn" style="text-align:left" onclick={() => commitPick(spell)}>{spell}</button>
    {/each}
  </div>
</Dialog>
