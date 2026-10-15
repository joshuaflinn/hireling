<script>
  // E12 Task 6 (design D6) — the party's stash screen.
  //
  // Read-only by construction where the seat may not write: the GM seat
  // renders NO control at all rather than a disabled one, because a button
  // that can only produce a `forbidden` ack is a lie about what the seat can
  // do (FR-11: "the GM's stash screen renders zero write affordances"). Every
  // write control is therefore behind a `{#if stash.can…()}` of the seat's own
  // predicate — the store owns the rule, this file only obeys it.
  //
  // Bulk comes from the sheet's own formatter (design D6: no second Bulk-math
  // path). Refused ops render inline and self-clear (degraded-mode §4) — no
  // modal, nothing blocking.
  //
  // Everything reads the store through plain function accessors rather than
  // bound stores, and re-reads when `changed()` fires, so a fan-out moves an
  // affordance without a reload (FR-5's requirement, and the reason the store
  // exports no unsubscribed `derived`).

  import { untrack } from 'svelte';
  import { tenthsText } from '../sheet/bulk.js';

  // Props. `lookUpBookValue` is D5's read route injected rather than fetched,
  // so both match outcomes are testable without a network: a resolved
  // {gp,sp,cp} prefills the sell prompt, a null is the corpus saying it does
  // not know — which the dialog must show, not swallow into a silent zero.
  /** @type {{ stash: import('./stash.js').StashState,
   *   roster?: Array<{character: {id: number, name: string, owner: string}}>,
   *   lookUpBookValue?: (name: string) => Promise<{gp: number, sp: number, cp: number} | null> }} */
  let {
    stash,
    roster = [],
    lookUpBookValue = async () => null,
  } = $props();

  // The projection, mirrored into local state so the markup reads flat. The
  // subscription lives inside an $effect (the repo's own pattern, SheetView) so
  // `stash` is read as a live reference rather than captured at setup —
  // svelte's `state_referenced_locally` warning is the compiler telling the
  // truth here, and a warning in the diff is a finding, not noise. The effect
  // also re-subscribes if the session ever swaps the store out from under it.
  let projection = $state(null);
  $effect(() => {
    const unsubscribe = stash.changed().subscribe(() => {
      // untrack: these reads run inside a store callback, not inside the
      // effect's own body, and must not register as dependencies of anything.
      untrack(readProjection);
    });
    // Prime immediately: the writable carries the projection as of the store's
    // construction, and a fixture delivered synchronously by a test has already
    // fanned out by the time this effect runs.
    readProjection();
    return unsubscribe;
  });

  // Flatten for the markup: the template stays as flat as the repo's style
  // asks, and a seat whose projection has not been read yet sees the store's
  // own empty values rather than a second set of defaults invented here.
  const rows = $derived(projection?.rows ?? []);
  const bank = $derived(projection?.bank ?? null);
  const claims = $derived(projection?.claims ?? []);
  const quartermasterId = $derived(projection?.quartermasterId ?? null);
  const canWrite = $derived(projection?.canWrite ?? false);
  const canSell = $derived(projection?.canSell ?? false);
  const canTransfer = $derived(projection?.canTransfer ?? false);
  const notices = $derived(projection?.notices ?? []);
  const unwired = $derived(projection?.unwired ?? []);

  function readProjection() {
    projection = {
      rows: stash.rows(),
      bank: stash.bank(),
      claims: stash.claims(),
      quartermasterId: stash.quartermasterId(),
      canWrite: stash.canWrite(),
      canSell: stash.canSell(),
      canTransfer: stash.canTransfer(),
      notices: stash.opErrors(),
      unwired: stash.unwiredActions(),
    };
  }

  // --- add form ----------------------------------------------------------
  let newName = $state('');
  let newQty = $state('1');
  let newBulk = $state('');
  let newNotes = $state('');

  function addItem() {
    stash.addRow({
      item_name: newName,
      quantity: Number(newQty),
      bulk: newBulk === '' ? null : Number(newBulk),
      notes: newNotes,
    });
    // Cleared only optimistically-ish: the row appears when the diff says so,
    // never before (no optimistic local row anywhere on this screen).
    newName = '';
    newQty = '1';
    newBulk = '';
    newNotes = '';
  }

  // --- dialogs -----------------------------------------------------------
  /** @type {{kind: 'transfer'|'sell', item: string, quantity: number} | null} */
  let dialog = $state(null);
  let dialogQty = $state('1');
  let dialogCharacter = $state('');
  let dialogError = $state('');
  /** @type {{gp: string, sp: string, cp: string} | null} */
  let saleCoins = $state(null);
  let bookValueKnown = $state(true);

  function openTransfer(item) {
    dialog = { kind: 'transfer', item: item.item_name, quantity: item.quantity };
    dialogQty = '1';
    dialogCharacter = String(roster[0]?.character.id ?? '');
    dialogError = '';
  }

  async function openSell(item) {
    dialog = { kind: 'sell', item: item.item_name, quantity: item.quantity };
    dialogQty = String(item.quantity);
    dialogError = '';
    saleCoins = { gp: '', sp: '', cp: '' };
    bookValueKnown = true;
    const value = await lookUpBookValue(item.item_name);
    // The dialog may have closed while the read was in flight.
    if (dialog?.kind !== 'sell' || dialog.item !== item.item_name) return;
    if (value) {
      saleCoins = { gp: String(value.gp), sp: String(value.sp), cp: String(value.cp) };
      bookValueKnown = true;
    } else {
      // D5: "no book value" is a state the user sees, not a silent zero that
      // quietly becomes the asking price.
      bookValueKnown = false;
    }
  }

  function closeDialog() {
    dialog = null;
    dialogError = '';
  }

  function submitDialog() {
    if (!dialog) return;
    const qty = Number(dialogQty);
    if (!Number.isInteger(qty) || qty < 1 || qty > dialog.quantity) {
      // FR-3's belt: the client refuses the over-draw and says what the bound
      // was. The server remains the authority.
      dialogError = `Only ${dialog.quantity} in the stash — the quantity cannot exceed it.`;
      return;
    }
    if (dialog.kind === 'transfer') {
      const id = Number(dialogCharacter);
      if (!Number.isInteger(id) || id === 0) {
        dialogError = 'Pick a character to move it to.';
        return;
      }
      stash.transfer(dialog.item, qty, id);
    } else {
      stash.sell(dialog.item, qty, {
        gp: Number(saleCoins?.gp || 0),
        sp: Number(saleCoins?.sp || 0),
        cp: Number(saleCoins?.cp || 0),
      });
    }
    closeDialog();
  }

  function setDesignation(event) {
    const raw = event.currentTarget.value;
    stash.setQuartermaster(raw === '' ? null : Number(raw));
  }

  /** Bulk is the sheet's number to format; this is the only call site, and it
   *  is the ONLY conversion anywhere: the wire carries Bulk units (design D1's
   *  numeric(4,2)), `tenthsText` takes tenths. A second such step on the client
   *  is the second Bulk-math path D6 forbids. */
  const bulkText = (bulk) => (bulk === null ? '—' : tenthsText(Math.round(bulk * 10)));

  /** FR-4's vocabulary is frozen history riding the row — the screen labels,
   *  it never rewrites. `character:<name at event time>` is already the name. */
  const place = (value) => (value === null ? '—' : String(value).replace(/^character:/, ''));

  const coinText = (row) => {
    const parts = [];
    if (row.coins_gp) parts.push(`${row.coins_gp} gp`);
    if (row.coins_sp) parts.push(`${row.coins_sp} sp`);
    if (row.coins_cp) parts.push(`${row.coins_cp} cp`);
    return parts.join(', ');
  };

  const timeText = (iso) => (iso ? String(iso).slice(0, 16).replace('T', ' ') : '');
</script>

<header>
  <h1>Stash</h1>
</header>

<!-- The bank balance. Its own region so a seat can read it independent of the
     loot list, and so the unknown state has somewhere honest to sit. -->
<section class="panel" aria-label="Party bank">
  <h2>Party bank</h2>
  {#if bank}
    <p class="balance">
      <span>{bank.gp} gp</span>
      <span>{bank.sp} sp</span>
      <span>{bank.cp} cp</span>
    </p>
  {:else}
    <!-- D1: the balance row is not zero-minted, so "we do not know yet" is
         the truth and 0/0/0 would be a fabrication. -->
    <p class="balance unknown">No bank record yet — the balance is unknown until the first movement.</p>
  {/if}
</section>

<!-- The refused-op records. Inline, self-clearing, non-blocking (§4): a
     refused write is a user-input fact, so it sits next to the work it
     concerns rather than stopping the screen. -->
{#if notices.length > 0}
  <ul class="notices" aria-label="Refused operations">
    {#each notices as notice (notice.op.op_id)}
      <li class="notice">
        <strong>{notice.outcome}</strong>
        {notice.op.value?.op ?? notice.op.target?.kind}
        {notice.reason ?? '— the party refused it'}
      </li>
    {/each}
  </ul>
{/if}

<section class="panel" aria-label="Party stash">
  <h2>Stash</h2>

  {#if rows.length === 0}
    <p class="empty">Nothing in the stash.</p>
  {:else}
    <ul class="loot" aria-label="Stash items">
      {#each rows as row (row.item_name)}
        <li role="listitem" aria-label={row.item_name}>
          <span class="name">{row.item_name}</span>
          <span class="qty">×{row.quantity}</span>
          <span class="bulk">{bulkText(row.bulk)}</span>
          <span class="notes">{row.notes ?? ''}</span>
          {#if canTransfer}
            <button type="button" onclick={() => openTransfer(row)}>Transfer</button>
          {/if}
          {#if canSell}
            <button type="button" onclick={() => openSell(row)}>Sell</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if canWrite}
    <form
      class="add"
      onsubmit={(event) => {
        event.preventDefault();
        addItem();
      }}
    >
      <h3>Add an item</h3>
      <label>
        Item name
        <input type="text" bind:value={newName} placeholder="Longsword" />
      </label>
      <label>
        Quantity
        <input type="number" min="1" step="1" bind:value={newQty} />
      </label>
      <label>
        Bulk
        <input type="number" step="0.1" min="0" bind:value={newBulk} placeholder="optional" />
      </label>
      <label>
        Note
        <input type="text" bind:value={newNotes} placeholder="optional" />
      </label>
      <button type="submit">Add to stash</button>
    </form>
  {/if}

  <!-- Party settings: the quartermaster flip. FR-5's flip rule says any
       character owner may set, change, or clear it — including the designated
       character's own owner, and including while one is designated. So the
       only seat that loses this control is the GM, who writes nothing. -->
  <!-- A labelled <section>, not a labelled <div>: aria-label on a plain div
       carries no role, so the block would answer to no query at all — and
       FR-11 puts this control in party settings, which a seat needs to find. -->
  <section class="settings" aria-label="Party settings">
    <h3>Party settings</h3>
    {#if canWrite}
      <label>
        Quartermaster
        <select value={quartermasterId ?? ''} onchange={setDesignation} aria-label="Quartermaster">
          <option value="">Nobody</option>
          {#each roster as entry (entry.character.id)}
            <option value={entry.character.id}>{entry.character.name}</option>
          {/each}
        </select>
      </label>
      <p class="hint">Only the quartermaster's owner may sell or adjust the bank.</p>
    {:else}
      <!-- The GM reads WHO holds the designation but is given no control for it.
           aria-hidden keeps this mirror out of the accessibility tree so a
           seat that may write nothing also OFFERS nothing — not even a
           control-shaped thing to tab onto (FR-11's "zero write affordances",
           read strictly). The name is carried in the heading's own text. -->
      <p class="designation" aria-hidden="true">
        Quartermaster: {roster.find((entry) => entry.character.id === quartermasterId)?.character.name ?? 'nobody'}
      </p>
    {/if}
  </section>
</section>

<!-- The claim log: server-committed rows only (D6 refuses optimistic local
     claim rows), newest-first by row id (D7). -->
<section class="panel" aria-label="Claim history">
  <h2>Claim history</h2>
  {#if claims.length === 0}
    <p class="empty">No movements recorded.</p>
  {:else}
    <ol role="log" aria-label="Claim history">
      {#each claims as claim (claim.id)}
        <li>
          <span class="when">{timeText(claim.created_at)}</span>
          <span class="what">{claim.quantity}× {claim.item_name}</span>
          <span class="route">{place(claim.origin)} → {place(claim.destination)}</span>
          {#if coinText(claim)}<span class="coins">{coinText(claim)}</span>{/if}
        </li>
      {/each}
    </ol>
  {/if}
</section>

{#if dialog}
  <div class="dialog-backdrop" role="presentation">
    <div
      class="dialog"
      role="dialog"
      aria-label={dialog.kind === 'sell' ? `Sell ${dialog.item}` : `Transfer ${dialog.item}`}
    >
      <h3>
        {dialog.kind === 'sell' ? 'Sell' : 'Transfer'} {dialog.item}
        <span class="on-hand">({dialog.quantity} in the stash)</span>
      </h3>

      {#if unwired.length > 0}
        <p class="warning" role="status">
          {unwired.join(' and ')} {unwired.length > 1 ? 'are' : 'is'} not wired up on the
          server yet (lane L3), so this cannot post. Everything else on this screen works.
        </p>
      {/if}

      <label>
        Quantity
        <input type="number" min="1" max={dialog.quantity} step="1" bind:value={dialogQty} />
      </label>

      {#if dialog.kind === 'transfer'}
        <label>
          To character
          <select bind:value={dialogCharacter} aria-label="Transfer to character">
            {#each roster as entry (entry.character.id)}
              <option value={entry.character.id}>{entry.character.name}</option>
            {/each}
          </select>
        </label>
      {:else}
        <!-- D5: book value is the prompt's default, never the sale's
             constraint. When the corpus is silent the dialog says so and lets
             the quartermaster name their own price. -->
        {#if bookValueKnown}
          <p class="book-value" role="status">Book value prefilled from the rules corpus.</p>
        {:else}
          <p class="no-book-value" role="status">No book value for this item — name your own price.</p>
        {/if}
        <div class="coins">
          <label>
            Gold (gp)
            <input type="number" min="0" step="1" bind:value={saleCoins.gp} aria-label="Gold coins" />
          </label>
          <label>
            Silver (sp)
            <input type="number" min="0" step="1" bind:value={saleCoins.sp} aria-label="Silver coins" />
          </label>
          <label>
            Copper (cp)
            <input type="number" min="0" step="1" bind:value={saleCoins.cp} aria-label="Copper coins" />
          </label>
        </div>
      {/if}

      {#if dialogError}
        <p class="error" role="status">{dialogError}</p>
      {/if}

      <div class="actions">
        <button type="button" onclick={closeDialog}>Cancel</button>
        <button type="button" onclick={submitDialog}>
          {dialog.kind === 'sell' ? 'Sell' : 'Transfer'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1.05rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.8;
  }
  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.95rem;
  }
  .panel {
    border: 1px solid var(--rule, #3a3630);
    border-radius: 6px;
    padding: 0.75rem;
    margin: 0.75rem 0;
  }
  .balance {
    display: flex;
    gap: 0.75rem;
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .balance.unknown {
    opacity: 0.7;
    font-style: italic;
  }
  .notices {
    list-style: none;
    margin: 0.5rem 0;
    padding: 0;
  }
  .notice {
    border-left: 3px solid var(--danger, #b4483d);
    padding: 0.3rem 0.6rem;
    background: color-mix(in srgb, var(--danger, #b4483d) 12%, transparent);
    border-radius: 3px;
    font-size: 0.9rem;
  }
  .loot {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .loot li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.35rem 0;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #3a3630) 55%, transparent);
  }
  .loot .name {
    font-weight: 600;
  }
  .loot .qty,
  .loot .bulk {
    font-variant-numeric: tabular-nums;
    opacity: 0.85;
  }
  .loot .notes {
    flex: 1 1 100%;
    opacity: 0.7;
    font-size: 0.85rem;
  }
  .loot button {
    margin-left: 0.25rem;
  }
  .empty {
    opacity: 0.65;
    font-style: italic;
  }
  .add {
    display: grid;
    gap: 0.4rem;
    margin-top: 0.9rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--rule, #3a3630);
  }
  .add label,
  .settings label,
  .dialog label {
    display: grid;
    gap: 0.15rem;
    font-size: 0.85rem;
  }
  .settings {
    margin-top: 0.9rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--rule, #3a3630);
  }
  .hint,
  .designation {
    font-size: 0.8rem;
    opacity: 0.75;
    margin: 0.35rem 0 0;
  }
  ol[role='log'] {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.85rem;
  }
  ol[role='log'] li {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding: 0.25rem 0;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #3a3630) 45%, transparent);
  }
  .when {
    font-variant-numeric: tabular-nums;
    opacity: 0.7;
  }
  .route,
  .coins {
    opacity: 0.8;
  }
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 20;
  }
  .dialog {
    background: var(--surface, #23201d);
    border: 1px solid var(--rule, #3a3630);
    border-radius: 8px;
    padding: 1rem;
    width: min(32rem, 92vw);
    display: grid;
    gap: 0.6rem;
  }
  .on-hand {
    font-weight: 400;
    opacity: 0.7;
    font-size: 0.85rem;
  }
  .coins {
    display: flex;
    gap: 0.5rem;
  }
  .coins label {
    flex: 1;
  }
  .book-value,
  .no-book-value,
  .warning {
    margin: 0;
    font-size: 0.85rem;
    padding: 0.3rem 0.5rem;
    border-radius: 3px;
  }
  .book-value {
    background: color-mix(in srgb, var(--good, #5c8a4a) 16%, transparent);
  }
  .no-book-value,
  .warning {
    background: color-mix(in srgb, var(--warn, #b58a3a) 16%, transparent);
  }
  .error {
    margin: 0;
    color: var(--danger, #b4483d);
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
