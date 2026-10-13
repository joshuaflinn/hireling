<script>
  // The condition picker surface (E9 FR-7 — the surface E8's spec assumed;
  // E8 shipped the REST + apply frames only). A Dialog over
  // GET /api/parties/{id}/conditions: search, tier/lane/valued badges, a
  // value input for valued conditions, and apply through the EXISTING
  // effect-create write — the payload is the wire's own create shape,
  // corpus-sourced, zero inline modifiers (FR-6: custom rows resolve to
  // tracked-manually on the server; nothing here computes). Custom rows
  // list inline, badged, their optional value as a display note joined
  // from the custom read (GET /conditions carries no description/value).
  // "Add custom condition" rides the inline AddCustomForm.
  import Dialog from './Dialog.svelte';
  import ConditionTip from './ConditionTip.svelte';
  import AddCustomForm from './AddCustomForm.svelte';

  /** @type {{ partyId: number, targetId: number, sourceId: number,
    open?: boolean, onclose?: () => void, onapply?: (create: any) => void,
    onsubmitcustom?: (fields: any) => Promise<any>,
    customRows?: Array<any>, fetchImpl?: typeof fetch }} */
  let {
    partyId,
    targetId,
    sourceId,
    open = false,
    onclose,
    onapply,
    onsubmitcustom,
    customRows = [],
    fetchImpl = fetch,
  } = $props();

  /** @type {Array<{corpus_entry_id: number, name: string, tier: string, lane: string, valued: boolean}>} */
  let rows = $state([]);
  let search = $state('');
  /** @type {Record<number, string>} */
  let values = $state({});
  /** @type {Record<number, string>} */
  let rowErrors = $state({});
  let adding = $state(false);

  const normalized = (/** @type {string} */ value) => value.toLowerCase().replace(/\s+/g, ' ').trim();
  const noteByRow = $derived(new Map(customRows.map((row) => [normalized(row.name), row])));

  const filtered = $derived(
    rows.filter((row) => normalized(row.name).includes(normalized(search))),
  );

  // Every open fetches fresh — the party-wide pick-up path (design D3).
  $effect(() => {
    if (!open) return;
    fetchImpl(`/api/parties/${partyId}/conditions`)
      .then((response) => {
        if (!response.ok) throw new Error(`the server answered ${response.status}`);
        return response.json();
      })
      .then((next) => {
        rows = /** @type {any[]} */ (next);
      })
      .catch(() => {
        // Degrade to the last fetched state (design D3) — an offline open
        // shows what the last open showed, and the apply path is the
        // sheet's offline queue anyway.
      });
  });

  /**
   * Apply one row: the existing effect-create payload, corpus-sourced.
   * @param {{corpus_entry_id: number, name: string, valued: boolean}} row
   */
  function apply(row) {
    let conditionValue = null;
    if (row.valued) {
      const raw = values[row.corpus_entry_id] ?? '1';
      const value = Number(raw);
      if (!Number.isInteger(value) || value < 1 || value > 20) {
        rowErrors = { ...rowErrors, [row.corpus_entry_id]: 'value must be 1..20' };
        return;
      }
      conditionValue = value;
    }
    rowErrors = { ...rowErrors, [row.corpus_entry_id]: '' };
    onapply?.({
      name: row.name,
      source_character_id: sourceId,
      targets: [targetId],
      modifiers: [],
      duration_note: '',
      corpus_entry_id: row.corpus_entry_id,
      condition_value: conditionValue,
    });
    onclose?.();
  }

  /** The Add-custom submit: the parent owns the create; the list refetches. */
  async function submitCustom(/** @type {any} */ fields) {
    await onsubmitcustom?.(fields);
    adding = false;
    const response = await fetchImpl(`/api/parties/${partyId}/conditions`);
    if (response.ok) rows = /** @type {any[]} */ (await response.json());
  }
</script>

<Dialog {open} title="Add a condition" onclose={onclose}>
  <div class="picker">
    <input
      class="search"
      bind:value={search}
      aria-label="Search conditions"
      placeholder="Search…"
    />
    <div class="list" role="list" aria-label="Conditions">
      {#each filtered as row (row.corpus_entry_id)}
        {@const note = noteByRow.get(normalized(row.name))}
        <div class="row" role="listitem">
          <span class="nm">
            <ConditionTip name={row.name} tier={row.tier} lane={row.lane}
              customDescription={note?.description ?? null}
              customValue={note?.value_or_rank ?? null}
            />
            {#if row.lane === 'custom'}
              <em class="badge">custom</em>
            {/if}
            <em class="badge">{row.tier}</em>
            {#if note?.description}
              <small class="note">{note.description}</small>
            {/if}
            {#if note?.value_or_rank !== undefined && note?.value_or_rank !== null}
              <small class="note">Value: {note.value_or_rank}</small>
            {/if}
          </span>
          {#if row.valued}
            <input
              class="val"
              type="number"
              min="1"
              max="20"
              value="1"
              aria-label="Value of {row.name}"
              oninput={(event) => (values = { ...values, [row.corpus_entry_id]: event.currentTarget.value })}
            />
          {/if}
          <button class="btn" onclick={() => apply(row)}>Apply {row.name}</button>
        </div>
        {#if rowErrors[row.corpus_entry_id]}
          <p class="op-error" role="alert">{rowErrors[row.corpus_entry_id]}</p>
        {/if}
      {/each}
    </div>
    {#if adding}
      <AddCustomForm kind="condition" onsubmit={submitCustom} oncancel={() => (adding = false)} />
    {:else}
      <button class="btn wide" onclick={() => (adding = true)}>Add custom condition</button>
    {/if}
  </div>
</Dialog>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 300px;
  }
  .search {
    background: var(--panel, #10141b);
    border: 1px solid var(--edge, #2c3444);
    border-radius: 6px;
    color: var(--text, #e8e4d8);
    padding: 5px 8px;
    font-size: 13px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 46vh;
    overflow: auto;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 13px;
  }
  .nm {
    display: inline-flex;
    align-items: baseline;
    gap: 5px;
    flex: 1;
    min-width: 0;
    flex-wrap: wrap;
  }
  .badge {
    font-style: normal;
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--dim, #5c6672);
    border: 1px solid var(--muted, #8a94a3);
    border-radius: 4px;
    padding: 0 4px;
  }
  .note {
    color: var(--muted, #8a94a3);
    font-size: 11px;
  }
  .val {
    width: 58px;
    background: var(--panel, #10141b);
    border: 1px solid var(--edge, #2c3444);
    border-radius: 6px;
    color: var(--text, #e8e4d8);
    padding: 3px 6px;
    font-size: 12.5px;
  }
  .op-error {
    margin: 0;
    color: var(--danger, #d06a5a);
    font-size: 12px;
  }
  .wide {
    align-self: stretch;
  }
</style>
