<script>
  // HpBar (spec §2.2): 0–max clamped, temp HP as a distinct segment that
  // absorbs damage first, ±1/±5/Full, temp input. Every write goes through
  // the sheet state layer, which owns the client-side bounds; the bar only
  // renders. `editable=false` renders no controls (design §6).
  // Pending echoes tint the number; controls disable while offline or
  // pending (degraded-mode §4).
  let {
    hp = { value: 0, pending: false },
    temp = { value: 0, pending: false },
    max = 1,
    editable = true,
    offline = false,
    ondamage,
    onheal,
    onfull,
    ontemp,
  } = $props();

  const total = $derived(max + temp.value);
  const hpPct = $derived(Math.max(0, Math.min(100, (hp.value / (total || 1)) * 100)));
  const tempPct = $derived(Math.max(0, Math.min(100, (temp.value / (total || 1)) * 100)));
  const busy = $derived(offline || hp.pending || temp.pending);
</script>

<div class="hp" class:pending={hp.pending || temp.pending}>
  <div class="hp-top">
    <span class="lbl">Hit Points</span>
    <span class="hpnum">
      <b>{hp.value}</b> / {max}{temp.value
        ? ` <span style="color:var(--blue)">+${temp.value} temp</span>`
        : ''}</span>
  </div>
  <div
    class="bar"
    role="meter"
    aria-valuemin="0"
    aria-valuemax={max}
    aria-valuenow={hp.value}
    aria-label="Hit Points {hp.value} of {max}"
  >
    <i style="width:{hpPct}%"></i><i class="tmp" style="width:{tempPct}%"></i>
  </div>
  {#if editable}
    <div class="hp-btns">
      <button onclick={() => ondamage?.(5)} disabled={busy} aria-label="Damage 5">−5</button>
      <button onclick={() => ondamage?.(1)} disabled={busy} aria-label="Damage 1">−1</button>
      <button onclick={() => onheal?.(1)} disabled={busy} aria-label="Heal 1">+1</button>
      <button onclick={() => onheal?.(5)} disabled={busy} aria-label="Heal 5">+5</button>
      <button onclick={() => onfull?.()} disabled={busy} title="Restore to max">Full</button>
      <span class="tmpin">
        Temp
        <input
          type="number"
          min="0"
          value={temp.value}
          disabled={offline}
          aria-label="Temporary Hit Points"
          onchange={(event) => ontemp?.(event.currentTarget.value)}
        />
      </span>
    </div>
  {/if}
</div>
