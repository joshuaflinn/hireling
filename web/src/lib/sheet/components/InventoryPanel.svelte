<script>
  import { tipTrigger } from '../../corpus/tip.svelte.js';
  // The inventory pane (spec §2.5, Q2 ruling: display + qty). Container
  // groups from base_sheet with Bulk rollups (extradimensional excluded
  // and labeled), quantity editing (qty 0 = removed, design §11.6), coin
  // counts writing vitals.money, trait chips from the corpus. No container
  // editing, no equipped sync, no tag editing — parked.
  import { inventoryView, tenthsText } from '../bulk.js';
  import { organize, carriedTenths, bulkLimits, patchInv, INV_TABS, TAG_LABEL } from '../inv-edit.js';
  import { lookup as corpusLookup } from '../../corpus/client.js';
  import { findOpError } from '../state.js';

  /** @type {{ baseSheet: any, itemBulk: Record<string, number | null>,
    itemTraits: Record<string, string[]>, qtyMap: Record<string, {qty: number, pending: boolean}>,
    money: any, opErrors?: any[], editable?: boolean, offline?: boolean,
    onqty?: (name: string, qty: number) => void, onmoney?: (money: any) => void,
    customItems?: Array<{corpus_entry_id?: number, name: string, description?: string,
      value_or_rank?: number | null, qty?: number}>,
    onaddcustom?: () => void,
    sheetUi?: {notes: any[], minions: any[], layout: any, inv?: Record<string, any>} | null,
    strMod?: number, onwriteui?: (next: any) => void }} */
  let {
    baseSheet,
    itemBulk = {},
    itemTraits = {},
    qtyMap = {},
    money,
    opErrors = [],
    editable = true,
    offline = false,
    onqty,
    onmoney,
    customItems = [],
    onaddcustom,
    sheetUi = null,
    strMod = 0,
    onwriteui,
  } = $props();

  const items = $derived(baseSheet.equipment ?? []);
  const qtyOf = $derived((/** @type {string} */ name) => qtyMap[name]?.qty ?? 0);
  const viewOf = $derived(inventoryView(baseSheet, itemBulk, itemTraits, (name) => qtyOf(name)));

  const denominations = $derived([
    ['pp', 'PP'],
    ['gp', 'GP'],
    ['sp', 'SP'],
    ['cp', 'CP'],
  ]);
  const moneyError = $derived(findOpError(opErrors, 'vitals', { field: 'money' }));
  /** @param {string} name */
  const qtyError = (name) => findOpError(opErrors, 'inv', { item_name: name });

  // ---- Dave's inventory editing (specs/015 slice 8) ----------------------
  let tab = $state('all');
  let menuFor = $state(/** @type {string | null} */ (null));
  let restoreOpen = $state(false);
  /** Corpus item records, for tag derivation (gems/trade/magic). */
  let recs = $state(/** @type {Record<string, any>} */ ({}));
  $effect(() => {
    const names = (baseSheet.equipment ?? []).map((/** @type {any} */ e) => e.name);
    if (!names.length) return;
    corpusLookup('item', { names: names.slice(0, 200) })
      .then((rows) => {
        /** @type {Record<string, any>} */
        const m = {};
        for (const r of rows) if (!m[r.name]) m[r.name] = r;
        recs = m;
      })
      .catch(() => {});
  });
  const inv = $derived(sheetUi?.inv ?? {});
  const org = $derived(
    organize(
      baseSheet,
      inv,
      (name) => qtyOf(name),
      (name) => itemBulk[name] ?? null,
      (name) => itemTraits[name] ?? [],
      (name) => recs[name] ?? null,
    ),
  );
  const containers = $derived(org.rows.filter((r) => r.isContainer));
  const topItems = $derived(org.rows.filter((r) => !r.isContainer && r.loc === null));
  const inside = $derived((/** @type {string} */ c) => org.rows.filter((r) => r.loc === c));
  const visible = $derived((/** @type {any} */ r) =>
    tab === 'all' ? true : tab === 'equipped' ? r.equipped : r.tags.includes(tab),
  );
  const count = $derived((/** @type {string} */ k) =>
    k === 'all' ? org.rows.length : org.rows.filter((r) => (k === 'equipped' ? r.equipped : r.tags.includes(k))).length,
  );
  // Weapons + armor are carried too (bulk.js rules); add them to the meter.
  const wornTenths = $derived(viewOf.totalTenths - viewOf.groups.reduce((n, g) => n + (g.bulkTenths ?? 0), 0));
  const totalTenths = $derived(carriedTenths(org.rows) + Math.max(0, wornTenths));
  const limits = $derived(bulkLimits(strMod));
  const pct = $derived(Math.min(100, (totalTenths / Math.max(1, limits.max * 10)) * 100));
  const enc = $derived(totalTenths > limits.encumbered * 10);
  const containerBulk = (/** @type {string} */ c) =>
    inside(c).reduce((n, r) => n + (r.bulkTenths ?? 0) * r.qty, 0);

  function edit(/** @type {string} */ name, /** @type {Record<string, any>} */ patch) {
    if (!sheetUi || !onwriteui) return;
    onwriteui({ ...sheetUi, inv: patchInv(inv, name, patch) });
    menuFor = null;
  }
  function toggleTag(/** @type {any} */ r, /** @type {string} */ t) {
    const tags = r.tags.includes(t) ? r.tags.filter((/** @type {string} */ x) => x !== t) : [...r.tags, t];
    edit(r.name, { tags });
  }

</script>

<section class="panel" aria-label="Inventory">
  <h2>
    Inventory <small>{items.length + customItems.length} entries</small>
    {#if editable && onaddcustom}
      <button class="btn" style="float:right;font-size:12px;padding:2px 9px" onclick={onaddcustom}>Add custom item</button>
    {/if}
  </h2>

  {#if editable}
    <div class="coins">
      {#each denominations as [key, label] (key)}
        <div class="coin {key}">
          <div class="v">
            <input
              type="number"
              min="0"
              value={money?.value?.[key] ?? 0}
              disabled={offline || money?.pending}
              aria-label="{label} coins"
              onchange={(event) =>
                onmoney?.({ ...money.value, [key]: Number(event.currentTarget.value) })}
            />
          </div>
          <div class="k">{label}</div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="coins">
      {#each denominations as [key, label] (key)}
        <div class="coin {key}"><div class="v">{money?.value?.[key] ?? 0}</div><div class="k">{label}</div></div>
      {/each}
    </div>
  {/if}
  {#if moneyError}
    <p class="op-error" role="alert">{moneyError.reason}</p>
  {/if}

  <div class="tabs invtabs" role="tablist">
    {#each INV_TABS as [k, label] (k)}
      <button role="tab" aria-selected={tab === k} class:on={tab === k} onclick={() => (tab = k)}>
        {label} <span class="tcount">{count(k)}</span>
      </button>
    {/each}
    {#if editable && org.removed.length}
      <button class="restore" onclick={() => (restoreOpen = !restoreOpen)}>Restore ({org.removed.length})</button>
    {/if}
  </div>
  {#if restoreOpen}
    <div class="restore-box">
      {#each org.removed as name (name)}
        <button class="chip" onclick={() => edit(name, { removed: false })}>↺ {name}</button>
      {/each}
    </div>
  {/if}

  {#snippet itemRow(/** @type {any} */ item, /** @type {boolean} */ flat)}
    {@const itemError = qtyError(item.name)}
    <div class="row irow ed" class:pending={qtyMap[item.name]?.pending} class:nested={!flat && item.loc !== null}>
      {#if editable && item.loc === null}
        <input type="checkbox" class="eqcb" title="Equipped" aria-label="Equipped: {item.name}" checked={item.equipped} disabled={offline}
          onchange={(e) => edit(item.name, { equipped: e.currentTarget.checked })} />
      {:else}<span class="cbsp"></span>{/if}
      <span class="nm" use:tipTrigger={{ kind: 'item', name: item.name }}>
        {item.name}
        {#if flat && item.loc}<span class="where">in {item.loc}</span>{/if}
        {#each item.tags as t (t)}<span class="itag">{TAG_LABEL[t]}</span>{/each}
        {#if item.invested}<span class="chip">invested</span>{/if}
        {#each item.traits as trait (trait)}<span class="chip">{trait}</span>{/each}
      </span>
      <span class="q">{item.bulkTenths === null ? '—' : tenthsText(item.bulkTenths * item.qty)}</span>
      {#if editable}
        <span class="qty">
          <input type="number" min="0" value={item.qty} disabled={offline} aria-label="Quantity of {item.name}"
            onchange={(event) => onqty?.(item.name, Number(event.currentTarget.value))} />
        </span>
        <span class="imenu">
          <button class="mbtn" aria-label="Organize {item.name}" disabled={offline} onclick={() => (menuFor = menuFor === item.name ? null : item.name)}>⋯</button>
          {#if menuFor === item.name}
            <div class="mpop" role="menu">
              <div class="mh">Move to</div>
              <button role="menuitem" disabled={item.loc === null} onclick={() => edit(item.name, { loc: null })}>Carried (top level)</button>
              {#each containers.filter((c) => c.name !== item.name) as c (c.name)}
                <button role="menuitem" disabled={item.loc === c.name} onclick={() => edit(item.name, { loc: c.name })}>{c.name}</button>
              {/each}
              <div class="mh">Tags</div>
              {#each Object.keys(TAG_LABEL) as t (t)}
                <button role="menuitemcheckbox" aria-checked={item.tags.includes(t)} onclick={() => toggleTag(item, t)}>{item.tags.includes(t) ? '✓ ' : ''}{TAG_LABEL[t]}</button>
              {/each}
              <div class="mh">Item</div>
              <button role="menuitem" onclick={() => edit(item.name, { container: true, loc: null })}>Mark as container</button>
              <button role="menuitem" class="danger" onclick={() => edit(item.name, { removed: true })}>Delete (restorable)</button>
            </div>
          {/if}
        </span>
      {:else}
        <span class="qty">×{item.qty}</span>
      {/if}
    </div>
    {#if itemError}<p class="op-error" role="alert">{itemError.reason}</p>{/if}
  {/snippet}

  {#if tab === 'all' || tab === 'equipped'}
    {#each topItems.filter(visible) as item (item.name)}{@render itemRow(item, false)}{/each}
    {#each containers.filter((c) => tab === 'all' || c.equipped || inside(c.name).some(visible)) as c (c.name)}
      {@const sum = containerBulk(c.name)}
      <h3 class="chead">
        {#if editable}
          <input type="checkbox" class="eqcb" title="Equipped" aria-label="Equipped: {c.name}" checked={c.equipped} disabled={offline}
            onchange={(e) => edit(c.name, { equipped: e.currentTarget.checked })} />
        {/if}
        <span class="cnm" use:tipTrigger={{ kind: 'item', name: c.name }}>{c.name}</span>
        <span class="ccount">{inside(c.name).length}</span>
        {#if c.extradimensional}<span class="chip">extradimensional — contents excluded</span>{/if}
        <span class="h3s">{c.extradimensional ? 'Bulk: excluded' : `${tenthsText(sum)} inside${c.backpack && c.equipped ? `, ${tenthsText(Math.max(0, sum - 20))} counts` : ''}`}</span>
        {#if editable && !baseSheet.containers?.some((/** @type {any} */ ec) => ec.name === c.name)}
          <button class="mbtn" title="Unmark container" onclick={() => edit(c.name, { container: false })}>×</button>
        {/if}
      </h3>
      {#each inside(c.name).filter(visible) as item (item.name)}{@render itemRow(item, false)}{:else}
        <p class="pet-note">This container currently holds no items.</p>
      {/each}
    {/each}
  {:else}
    {#each org.rows.filter(visible).sort((a, b) => a.name.localeCompare(b.name)) as item (item.name)}{@render itemRow(item, true)}{:else}
      <p class="pet-note">Nothing tagged {TAG_LABEL[tab]} yet. Use ⋯ on a row to tag it.</p>
    {/each}
  {/if}

  {#if customItems.length}
    <h3 class="chead">
      <span class="cnm">Custom</span>
      <span class="ccount">{customItems.length}</span>
    </h3>
    {#each customItems as item (item.corpus_entry_id ?? item.name)}
      <div class="row irow">
        <span class="nm">
          {item.name}
          <span class="chip">custom</span>
          {#if item.description}<small style="color:var(--muted,#8a94a3);font-size:11px;margin-left:4px">{item.description}</small>{/if}
        </span>
        <span class="q">—</span>
        {#if editable}
          <span class="qty">
            <input
              type="number"
              min="0"
              value={item.qty ?? 1}
              disabled={offline}
              aria-label="Quantity of {item.name}"
              onchange={(event) => onqty?.(item.name, Number(event.currentTarget.value))}
            />
          </span>
        {:else}
          <span class="qty">×{item.qty ?? 1}</span>
        {/if}
      </div>
    {/each}
  {/if}

  <div class="bulk">
    Carrying <b>{tenthsText(totalTenths)}</b> · <span class:enc>encumbered</span> above <b>{limits.encumbered}</b> · max <b>{limits.max}</b>
    <div class="meter"><i class:over={enc} style:width="{pct}%"></i></div>
  </div>
</section>

<style>
  .invtabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 8px 0 6px;
  }
  .invtabs button {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 6px;
    padding: 2px 9px;
    font-size: 13px;
    cursor: pointer;
  }
  .invtabs button.on {
    color: var(--gold);
    border-color: var(--gold-dim);
  }
  .invtabs .tcount {
    color: var(--dim);
    font-size: 11px;
  }
  .invtabs .restore {
    margin-left: auto;
    border-color: var(--gold-dim);
    color: var(--gold);
  }
  .restore-box {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0 0 6px;
  }
  .restore-box .chip {
    cursor: pointer;
  }
  .row.irow.ed {
    grid-template-columns: 20px minmax(0, 1fr) auto auto 26px;
  }
  .row.irow.ed.nested {
    margin-left: 10px;
    border-left: 1px solid var(--line2);
  }
  .eqcb {
    accent-color: var(--gold);
    margin: 0;
  }
  .cbsp {
    display: inline-block;
    width: 13px;
  }
  .where {
    color: var(--dim);
    font-size: 11.5px;
    margin-left: 4px;
  }
  .itag {
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    border: 1px solid var(--line2);
    border-radius: 3px;
    padding: 0 4px;
    margin-left: 5px;
    color: var(--muted);
  }
  .imenu {
    position: relative;
  }
  .mbtn {
    background: none;
    border: 1px solid transparent;
    color: var(--muted);
    border-radius: 5px;
    cursor: pointer;
    padding: 0 5px;
  }
  .mbtn:hover {
    border-color: var(--line2);
  }
  .mpop {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 40;
    min-width: 190px;
    background: #0d0f13;
    border: 1px solid var(--gold-dim);
    border-radius: 8px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 24px rgb(0 0 0 / 50%);
  }
  .mpop .mh {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--dim);
    padding: 4px 6px 2px;
  }
  .mpop button {
    background: none;
    border: 0;
    color: var(--text);
    text-align: left;
    padding: 3px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }
  .mpop button:hover:not(:disabled) {
    background: var(--panel2);
  }
  .mpop button:disabled {
    color: var(--dim);
    cursor: default;
  }
  .mpop .danger {
    color: #e8907c;
  }
  .chead .eqcb {
    margin-right: 6px;
  }
  .meter {
    height: 6px;
    background: #2a2f38;
    border-radius: 3px;
    margin-top: 5px;
    overflow: hidden;
  }
  .meter i {
    display: block;
    height: 100%;
    background: #7fb65a;
  }
  .meter i.over {
    background: #d0653f;
  }
  .enc {
    color: #e8907c;
    font-weight: 700;
  }
</style>
