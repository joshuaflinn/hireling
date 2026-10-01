<script>
  // Staff Nexus panel (spec §2.3, Q1 ruling): the charge-rank select and
  // charge pips write `vitals.daily` (whole row); Drain Bonded Item is the
  // row's used flag. Charge math lives in sheet/staff.js (tested).
  import { chargesLeft, setSpent } from '../staff.js';

  /** @type {{ daily: {value: any, pending?: boolean}, editable?: boolean,
    offline?: boolean, onchange?: (daily: any) => void }} */
  let { daily, editable = true, offline = false, onchange } = $props();

  const row = $derived(daily.value ?? { staff_charge_rank: 0, staff_spent: 0, drain_used: false });
  const left = $derived(chargesLeft(row));
  const busy = $derived(offline || daily.pending);
  const pips = $derived(Array.from({ length: row.staff_charge_rank }, (_, index) => index + 1));
</script>

<div class="staff-ctl" class:pending={daily.pending}>
  <label>
    Staff charge rank
    <select
      value={row.staff_charge_rank}
      disabled={!editable || busy}
      onchange={(event) => onchange?.({ ...row, staff_charge_rank: Number(event.currentTarget.value) })}
    >
      {#each Array.from({ length: 11 }, (_, rank) => rank) as rank (rank)}
        <option value={rank}>{rank}</option>
      {/each}
    </select>
  </label>
  <span class="pips" role="group" aria-label="Staff charges: {left} of {row.staff_charge_rank} left">
    {#each pips as pip (pip)}
      <button
        class="pip"
        class:on={pip <= row.staff_charge_rank - row.staff_spent}
        disabled={!editable || busy}
        aria-label="Charge {pip}: click to mark spent up to here"
        onclick={() => onchange?.(setSpent(row, row.staff_charge_rank - pip + 1))}
      ></button>
    {/each}
    {#if row.staff_charge_rank === 0}
      <span class="chip">no charges</span>
    {/if}
  </span>
  <label class="ckl" style="display:flex;align-items:center;gap:6px">
    <input
      type="checkbox"
      checked={row.drain_used}
      disabled={!editable || busy}
      onchange={(event) => onchange?.({ ...row, drain_used: event.currentTarget.checked })}
    />
    Drain Bonded Item used
  </label>
</div>
