<script>
  // CharacterHeader (spec §2.1): name, the ancestry/class line, the level
  // display + adjust control (spec §4 — bounds 1..20, confirm on level-
  // down via Dialog), the SyncIndicator, and the New Day action.
  // Accepts `editable` (design §6): view-only renders no controls.
  import Dialog from './Dialog.svelte';
  import SyncIndicator from './SyncIndicator.svelte';

  let {
    name,
    subline,
    level,
    editable = true,
    offline = false,
    syncing = false,
    onadjustlevel,
    onnewday,
    onimport,
    onlogout,
  } = $props();

  let picking = $state(false);
  let confirmingDown = $state(false);
  let draftLevel = $state(level);

  function openPicker() {
    draftLevel = level;
    picking = true;
  }

  function commitPicker() {
    picking = false;
    if (draftLevel === level) return;
    if (draftLevel < level) {
      confirmingDown = true; // level-down always confirms (spec §4)
    } else {
      onadjustlevel?.(draftLevel);
    }
  }

  function confirmDown() {
    confirmingDown = false;
    onadjustlevel?.(draftLevel);
  }
</script>

<header class="top">
  <div>
    <h1>{name}</h1>
    <div class="sub">{subline}</div>
  </div>
  <div class="controls">
    {#if editable}
      <button class="btn" onclick={openPicker} title="Adjust your effective level (1–20)">
        Level {level}
      </button>
      <button class="btn gold" onclick={onnewday} disabled={offline} title="Clears cast slots, refills Focus Points, resets staff and Drain Bonded Item">
        New Day
      </button>
    {:else}
      <span class="pill">Level {level}</span>
    {/if}
    <SyncIndicator {syncing} />
    {#if onimport}
      <button class="btn" onclick={onimport} title="Import a Pathbuilder export (replaces this sheet's base data)">
        Import
      </button>
    {/if}
    {#if onlogout}
      <button class="btn" onclick={onlogout} title="End the session (E3 Story 6 AC3)">
        Log out
      </button>
    {/if}
  </div>
</header>

<Dialog open={picking} title="Adjust level" onclose={() => (picking = false)} oncommit={commitPicker}>
  <p style="color:var(--muted);font-size:13px">
    Every level-derived stat re-derives on tap: proficiency bonus, max HP,
    DCs, cantrip rank. Boosts, feats and spell slots come only from a
    re-import.
  </p>
  <div class="row2" style="margin-top:4px">
    <label style="display:flex;align-items:center;gap:8px">
      Level
      <select bind:value={draftLevel}>
        {#each Array.from({ length: 20 }, (_, i) => i + 1) as option}
          <option value={option}>{option}</option>
        {/each}
      </select>
    </label>
    <button class="btn gold" onclick={commitPicker}>
      {draftLevel < level ? `Step down to ${draftLevel}` : `Set level ${draftLevel}`}
    </button>
  </div>
</Dialog>

<Dialog
  open={confirmingDown}
  title={`Step down to level ${draftLevel}?`}
  onclose={() => (confirmingDown = false)}
  oncommit={confirmDown}
>
  <p style="color:var(--muted);font-size:13px">
    Max HP, DCs and proficiency drop to level {draftLevel} across the sheet —
    yours and everyone watching. Your export's real level is untouched.
  </p>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirmingDown = false)}>Keep level {level}</button>
    <button class="btn gold" onclick={confirmDown}>Step down to {draftLevel}</button>
  {/snippet}
</Dialog>
