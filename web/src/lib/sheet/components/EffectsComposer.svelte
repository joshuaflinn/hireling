<script>
  // The freeform effect composer (specs/010-effect-composer) — the PRD's
  // Step 3 write surface: name, modifier rows over the DERIVED stat
  // vocabulary (vocab.js — no client-side stat constant), duration note,
  // target picker from the party roster. Apply composes the exact
  // `effect_new` create op through the sheet's state layer; the manager
  // face below it lists active effects — the creator's carry add/remove-a-target
  // and End controls (the CAS mutation ops; PRD FG3: the creator adds AND
  // removes targets), everyone else's are read-only under their own heading.
  // Denials render the server's reason verbatim; an applied CREATE ack closes
  // the dialog (the parent owns that transition — an applied manager op only
  // settles and refetches). A failed effects fetch names itself here — no
  // silent failures.
  //
  // Zero client math: the composer composes ops; the server owns every
  // verdict, recompute, and provenance (the existing read path renders it).
  import Dialog from './Dialog.svelte';
  import { statOptionsFromView, modifierTypes } from '../vocab.js';

  /** @type {{ open?: boolean, onclose?: () => void, partyId: number,
    characterId: number, roster?: Array<{id: number, name: string | null}>,
    view?: any, effectsState?: {status: string, rows: any[]},
    composerOpState?: {phase: string, reason?: string} | null,
    oncreate?: (payload: any) => void, onretarget?: (effectId: number, targets: number[], baseVersion: number) => void,
    onend?: (effectId: number, baseVersion: number) => void }} */
  let {
    open = false,
    onclose,
    partyId,
    characterId,
    roster = [],
    view = null,
    effectsState = { status: 'idle', rows: [] },
    composerOpState = null,
    oncreate,
    onretarget,
    onend,
  } = $props();

  const TYPES = modifierTypes();

  let name = $state('');
  let durationNote = $state('');
  /** One row per modifier: stat → type → value (the PRD's picker order). */
  /** @type {Array<{type: string, stat: string, value: number | string}>} */
  const rows = $state([{ type: 'status', stat: '', value: 1 }]);
  /** Selected target character ids. */
  /** @type {number[]} */
  let selected = $state([]);

  const statGroups = $derived(statOptionsFromView(view));

  /**
   * Client-side bounds — the state layer re-validates; invalid input never
   * leaves the layer (spec FR-C4; the server's table is the reference:
   * name 1..=120, ≥1 target, ≤16 modifiers, integer value −50..=50).
   */
  const trimmedName = $derived(name.trim());
  // No rounding: the value that lands on the wire must be the value in the
  // field. A fraction fails the integer gate below (finding 6) — it never
  // silently becomes the nearest whole number.
  const cleanRows = $derived(
    rows.map((row) => ({
      type: row.type,
      stat: row.stat,
      value: row.value === '' || row.value === null ? NaN : Number(row.value),
    })),
  );
  const valid = $derived(
    trimmedName.length >= 1 &&
      trimmedName.length <= 120 &&
      selected.length >= 1 &&
      rows.length >= 1 &&
      rows.length <= 16 &&
      cleanRows.every(
        (row) =>
          TYPES.includes(row.type) &&
          row.stat !== '' &&
          Number.isInteger(row.value) &&
          row.value >= -50 &&
          row.value <= 50,
      ),
  );
  const writing = $derived(composerOpState?.phase === 'writing');

  /** The creator's own active effects — the controlled rows — and the rest
   *  of the party's, read-only (visibility, no controls). */
  const activeRows = $derived((effectsState?.rows ?? []).filter((row) => row.active));
  const managed = $derived(activeRows.filter((row) => row.source_character_id === characterId));
  const others = $derived(activeRows.filter((row) => row.source_character_id !== characterId));

  /** @param {number} id */
  function nameOf(id) {
    const entry = roster.find((candidate) => candidate.id === id);
    return entry ? (entry.name ?? `#${id}`) : `#${id}`;
  }

  function addRow() {
    if (rows.length < 16) rows.push({ type: 'status', stat: '', value: 1 });
  }

  /** @param {number} index */
  function removeRow(index) {
    rows.splice(index, 1);
  }

  function apply() {
    if (!valid || writing) return;
    oncreate?.({
      partyId,
      sourceCharacterId: characterId,
      name: trimmedName,
      targets: [...selected].sort((a, b) => a - b),
      modifiers: cleanRows,
      durationNote,
    });
  }

  /** @param {number} effectId @param {number[]} current @param {number} leaving */
  function removeTarget(effectId, current, leaving) {
    onretarget?.(
      effectId,
      current.filter((id) => id !== leaving),
      effectsState?.rows.find((row) => row.effect_id === effectId)?.version ?? 0,
    );
  }

  /** Add one roster member to an active effect's target set — the whole-set
   *  update op rides (PRD FG3: the creator adds AND removes targets; spec
   *  FR-C6). The state layer dedupes and bounds-checks the set.
   *  @param {any} row @param {number} id */
  function addTarget(row, id) {
    onretarget?.(row.effect_id, [...row.targets, id], row.version);
  }

  /** @param {any} row */
  function endEffect(row) {
    onend?.(row.effect_id, row.version);
  }
</script>

<Dialog {open} title="Effects" onclose={onclose} oncommit={apply}>
  {#if composerOpState?.phase === 'denied'}
    <p class="denial" role="alert">{composerOpState.reason}</p>
  {/if}

  <div class="field">
    <input aria-label="Effect name" placeholder="Name it — “Bless”" bind:value={name} maxlength="120" />
  </div>

  <div class="modifiers">
    {#each rows as row, index (index)}
      <div class="mod-row">
        <select aria-label="Stat" bind:value={row.stat}>
          <option value="" disabled hidden>stat…</option>
          {#each statGroups as group (group.group)}
            <optgroup label={group.group}>
              {#each group.options as option (option.value)}
                <option value={option.value}>{option.label}</option>
              {/each}
            </optgroup>
          {/each}
        </select>
        <select aria-label="Bonus type" bind:value={row.type}>
          {#each TYPES as type (type)}
            <option value={type}>{type}</option>
          {/each}
        </select>
        <input class="value" type="number" aria-label="Value" min="-50" max="50" step="1" bind:value={row.value} />
        {#if rows.length > 1}
          <button class="x" type="button" aria-label="Remove modifier {index + 1}" onclick={() => removeRow(index)}>✕</button>
        {/if}
      </div>
    {/each}
    <button class="add" type="button" onclick={addRow}>Add modifier</button>
  </div>

  <div class="field">
    <input aria-label="Duration note" placeholder="Duration note — “10 rounds”" bind:value={durationNote} />
  </div>

  <fieldset class="targets">
    <legend>Targets</legend>
    {#each roster as member (member.id)}
      <label class="target">
        <input
          type="checkbox"
          aria-label={member.name ?? `#${member.id}`}
          checked={selected.includes(member.id)}
          onchange={(event) => {
            const checked = /** @type {HTMLInputElement} */ (event.currentTarget).checked;
            selected = checked ? [...selected, member.id] : selected.filter((id) => id !== member.id);
          }}
        />
        {member.name ?? `#${member.id}`}
      </label>
    {/each}
  </fieldset>

  {#if effectsState?.status === 'error'}
    <!-- Finding 4: “no effects” and “couldn't load” must not look the same. -->
    <p class="fetch-error" role="alert">Couldn't load active effects — reopen the dialog to retry.</p>
  {/if}

  {#if effectsState?.status === 'ready' && (managed.length > 0 || others.length > 0)}
    <div class="managed">
      {#if managed.length > 0}
        <h4>Your active effects</h4>
        {#each managed as row (row.effect_id)}
          <div class="managed-row">
            <b>{row.name}</b>
            {#each row.targets as targetId (targetId)}
              <span class="managed-target">
                {nameOf(targetId)}
                {#if row.targets.length > 1}
                  <!-- Finding 5: the last remaining target's ✕ would be a
                       dead control (an empty target set is refused by the
                       write path's bounds — at least one target, gh#85) —
                       End is the operation there. -->
                  <button
                    class="x"
                    type="button"
                    aria-label={`Remove ${nameOf(targetId)} from ${row.name}`}
                    onclick={() => removeTarget(row.effect_id, row.targets, targetId)}>✕</button
                  >
                {/if}
              </span>
            {/each}
            {#each roster as member (member.id)}
              {#if !row.targets.includes(member.id)}
                <!-- Finding 1 (P0): the creator ADDS targets too — the
                     whole-set update op with the member included. -->
                <button
                  class="add-target"
                  type="button"
                  aria-label={`Add ${nameOf(member.id)} to ${row.name}`}
                  onclick={() => addTarget(row, member.id)}>+ {nameOf(member.id)}</button
                >
              {/if}
            {/each}
            <button class="end" type="button" aria-label={`End ${row.name}`} onclick={() => endEffect(row)}>End</button>
          </div>
        {/each}
      {/if}
      {#if others.length > 0}
        <!-- Finding 7: someone else's effects never render under “Your
             active effects” — their own heading. -->
        <h4>Party effects</h4>
        {#each others as row (row.effect_id)}
          <div class="managed-row other">
            <b>{row.name}</b>
            <span class="from">from {nameOf(row.source_character_id)}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" disabled={!valid || writing} onclick={apply}>
      {writing ? 'Applying…' : 'Apply effect'}
    </button>
  {/snippet}
</Dialog>

<style>
  .denial {
    margin: 0 0 10px;
    padding: 6px 9px;
    border-left: 3px solid #e07a6a;
    background: rgba(224, 122, 106, 0.08);
    color: #e07a6a;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }

  .field input,
  .mod-row select,
  .mod-row input {
    background: var(--panel2, #161a21);
    border: 1px solid var(--edge, #465070);
    border-radius: 6px;
    color: var(--text, #e9e5d9);
    padding: 5px 8px;
    font-size: 0.9rem;
  }

  .field {
    margin-bottom: 10px;
  }

  .field input {
    width: 100%;
    box-sizing: border-box;
  }

  .modifiers {
    margin-bottom: 10px;
  }

  .mod-row {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-bottom: 6px;
  }

  .mod-row .value {
    width: 5.5rem;
  }

  .x {
    background: none;
    border: none;
    color: var(--muted, #8a94a3);
    cursor: pointer;
    padding: 2px 6px;
  }

  .add {
    background: none;
    border: 1px dashed var(--edge, #465070);
    border-radius: 6px;
    color: var(--muted, #8a94a3);
    padding: 3px 9px;
    cursor: pointer;
  }

  .targets {
    border: 1px solid var(--edge, #465070);
    border-radius: 6px;
    padding: 6px 10px 8px;
    margin: 0 0 10px;
  }

  .targets legend {
    color: var(--muted, #8a94a3);
    font-size: 0.8rem;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .target {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin: 2px 12px 2px 0;
    font-size: 0.9rem;
  }

  .managed {
    margin-top: 4px;
  }

  .managed h4 {
    margin: 0 0 6px;
    color: var(--muted, #8a94a3);
    font-size: 0.8rem;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .managed-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-top: 1px solid var(--edge, #2c3444);
    font-size: 0.9rem;
  }

  .managed-row .from {
    color: var(--muted, #8a94a3);
  }

  .fetch-error {
    margin: 0 0 10px;
    padding: 6px 9px;
    border-left: 3px solid #e07a6a;
    background: rgba(224, 122, 106, 0.08);
    color: #e07a6a;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }

  .add-target {
    background: none;
    border: 1px dashed var(--edge, #465070);
    border-radius: 6px;
    color: var(--muted, #8a94a3);
    padding: 2px 8px;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .managed-target {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--gold, #d4af5f);
  }

  .end {
    margin-left: auto;
    background: none;
    border: 1px solid var(--edge, #465070);
    border-radius: 6px;
    color: var(--text, #e9e5d9);
    padding: 2px 10px;
    cursor: pointer;
  }
</style>
