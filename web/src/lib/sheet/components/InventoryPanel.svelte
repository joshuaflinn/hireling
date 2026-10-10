<script>
  // The inventory pane (spec §2.5, Q2 ruling: display + qty). Container
  // groups from base_sheet with Bulk rollups (extradimensional excluded
  // and labeled), quantity editing (qty 0 = removed, design §11.6), coin
  // counts writing vitals.money, trait chips from the corpus. No container
  // editing, no equipped sync, no tag editing — parked.
  import { inventoryView } from '../bulk.js';
  import { findOpError } from '../state.js';

  /** @type {{ baseSheet: any, itemBulk: Record<string, number | null>,
    itemTraits: Record<string, string[]>, qtyMap: Record<string, {qty: number, pending: boolean}>,
    money: any, opErrors?: any[], editable?: boolean, offline?: boolean,
    onqty?: (name: string, qty: number) => void, onmoney?: (money: any) => void }} */
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
</script>

<section class="panel" aria-label="Inventory">
  <h2>Inventory <small>{items.length} entries</small></h2>

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

  {#each viewOf.groups as group (group.name ?? 'top')}
    <h3 class="chead">
      <span class="cnm">{group.name ?? 'Carried'}</span>
      <span class="ccount">{group.items.length}</span>
      {#if group.extradimensional}
        <span class="chip">extradimensional — contents excluded</span>
      {/if}
      <span class="h3s">Bulk: {group.bulkText}</span>
    </h3>
    {#each group.items as item (item.name)}
      {@const itemError = qtyError(item.name)}
      <div class="row irow" class:pending={qtyMap[item.name]?.pending}>
        <span class="nm">
          {item.name}
          {#if item.invested}<span class="chip">invested</span>{/if}
          {#each item.traits as trait (trait)}
            <span class="chip">{trait}</span>
          {/each}
        </span>
        <span class="q">{item.bulkText}</span>
        {#if editable}
          <span class="qty">
            <input
              type="number"
              min="0"
              value={item.qty}
              disabled={offline}
              aria-label="Quantity of {item.name}"
              onchange={(event) => onqty?.(item.name, Number(event.currentTarget.value))}
            />
          </span>
        {:else}
          <span class="qty">×{item.qty}</span>
        {/if}
      </div>
      {#if itemError}
        <p class="op-error" role="alert">{itemError.reason}</p>
      {/if}
    {/each}
  {/each}

  <div class="bulk">
    Total carried: <b>{viewOf.totalText}</b>
    <span style="color:var(--dim)">(includes carried and worn gear; extradimensional contents excluded)</span>
  </div>
</section>
