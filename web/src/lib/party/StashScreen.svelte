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
  import { createBookValueLookup } from './book-value.js';

  // Props. The screen reads the corpus book value itself, through D5's route:
  // a resolved {gp,sp,cp} prefills the sell prompt, a null is the corpus saying
  // it does not know — which the dialog must show, not swallow into a silent
  // zero. `partyId` is a prop, not a lookup seam: an earlier revision took the
  // lookup as a prop with an `async () => null` default, `App.svelte` never
  // passed it, and every book-value test stayed green against the injected fake
  // while the built app could never prefill. A seam with one caller and a
  // default that is the broken behaviour is how that survived.
  /** @typedef {import('./stash.js').Projection} Projection */
  /** @typedef {import('./stash.js').StashRow} StashRow */
  /** @typedef {import('./stash.js').ClaimRow} ClaimRow */
  /** @typedef {import('./stash.js').BankBalance} BankBalance */

  /** @type {{ stash: import('./stash.js').StashState,
   *   partyId: number,
   *   roster?: Array<{character: {id: number, name: string, owner: string}}> }} */
  let {
    stash,
    partyId,
    roster = [],
  } = $props();

  // Built lazily, not at setup: `createBookValueLookup({ partyId })` here would
  // capture the initial prop value (compiler warning `state_referenced_locally`,
  // and a party switch would keep reading the old party's corpus).
  /** @param {string} name */
  function lookUpBookValue(name) {
    return createBookValueLookup({ partyId })(name);
  }

  // The projection, mirrored into local state so the markup reads flat. The
  // subscription lives inside an $effect (the repo's own pattern, SheetView) so
  // `stash` is read as a live reference rather than captured at setup —
  // svelte's `state_referenced_locally` warning is the compiler telling the
  // truth here, and a warning in the diff is a finding, not noise. The effect
  // also re-subscribes if the session ever swaps the store out from under it.
  // The declared type rides a const, not the `$state` call itself: svelte-check's
  // transform hands tsc `let x = $state(null)` with no annotation, so the slot
  // infers `null` and every `projection?.rows` below reports "property does not
  // exist on type 'never'". The const is the annotation; the binding is still a
  // `$state` slot, so the `$derived` reads above stay reactive.
  // The annotation is an inline cast on the initial value, not a JSDoc on the
  // declaration: svelte-check's transform drops a leading `@type` on a `$state`
  // line, so the slot infers `null` and every `projection?.rows` below reads as
  // "property does not exist on type 'never'". This is the form `SheetView.svelte:72`
  // already uses for the same job (`$state(/** @type {any[]} */ ([]))`).
  let projection = $state(/** @type {Projection | null} */ (null));
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

  function readProjection() {
    projection = stash.snapshot();
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
  // The union carries all three kinds, because `openMeta` adds the third and an
  // annotation copied from the transfer/sell pair makes every `meta` branch
  // unreadable — `svelte-check` reads the comparison as provably false and the
  // editor's markup as dead code, while the component runs perfectly. Name the
  // whole set so the branches that render the row editor are type-checked as
  // reachable, which is what FR-8's affordance actually is.
  /** @type {{kind: 'transfer'|'sell', item: string, quantity: number} | {kind: 'meta', item: string, quantity: number} | null} */
  let dialog = $state(null);
  let dialogQty = $state('1');
  let dialogCharacter = $state('');
  /** §3's `direction` is part of the frame, so the dialog asks for it. */
  let dialogDirection = $state('out');
  let dialogError = $state('');
  // The entry the frame is built from. `pp` is a field the USER may name and
  // the read route can never supply (design D5's book value is {gp,sp,cp}; the
  // bank holds no pp, and `stash_sell_tx` converts at credit time — 1 pp =
  // 10 gp, Player Core pg. 267, cited at that one conversion site), so it is
  // optional here: the prefill omits it, and `submitDialog` defaults it.
  /** @type {{pp?: string, gp: string, sp: string, cp: string} | null} */
  let saleCoins = $state(null);
  let bookValueKnown = $state(true);

  /** @param {StashRow} item */
  function openTransfer(item) {
    dialog = { kind: 'transfer', item: item.item_name, quantity: item.quantity };
    dialogQty = '1';
    dialogCharacter = String(roster[0]?.character.id ?? '');
    dialogDirection = 'out';
    dialogError = '';
  }

  /** @param {StashRow} item */
  async function openSell(item) {
    dialog = { kind: 'sell', item: item.item_name, quantity: item.quantity };
    dialogQty = String(item.quantity);
    dialogError = '';
    saleCoins = { pp: '', gp: '', sp: '', cp: '' };
    bookValueKnown = true;
    const value = await lookUpBookValue(item.item_name);
    // The dialog may have closed while the read was in flight.
    if (dialog?.kind !== 'sell' || dialog.item !== item.item_name) return;
    if (value) {
      // No pp line: design D5's book value is a {gp,sp,cp} coin object, so
      // there is nothing to prefill there and a platinum box the corpus can
      // never fill would read as a missing value rather than a non-applicable
      // one. A seat that sold something at platinum names it by hand.
      saleCoins = {
        gp: String(value.gp),
        sp: String(value.sp),
        cp: String(value.cp),
      };
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
    metaBulk = '';
    metaNotes = '';
  }

  // --- FR-7 manual adjustments / FR-8 row meta ----------------------------
  //
  // Three affordances, all of them the quartermaster's, all of them obeying the
  // store's own predicate rather than a second copy of the rule: FR-7's signed
  // row delta, FR-7's signed bank delta, and the row's bulk/note editor (which
  // is FR-8's field set reached from the row — NOT a delta, and not labelled as
  // one). The store actions behind them shipped in PR #311 with no caller at
  // all; gh#336 is the wiring, and the gate below is what makes the claim true.
  //
  // One rule governs all three: the client NEVER sends an absolute it computed
  // from what it read. A row delta is `qty_delta`, a bank move is `gp_delta`,
  // and only the meta frame is absolute — because meta is CAS'd on the version
  // this seat read (design D2), so an absolute there is the safe form, while the
  // same shape on a count or a balance is a lost-update race.
  let metaBulk = $state('');
  let metaNotes = $state('');

  /** @param {StashRow} row */
  function openMeta(row) {
    dialog = { kind: 'meta', item: row.item_name, quantity: row.quantity };
    // Prefilled from the row, because the editor cannot tell "unset" from
    // "unedited" any other way — and clearing IS a legal action here: the
    // both-null frame is the server's clear arm.
    metaBulk = row.bulk === null ? '' : String(row.bulk);
    metaNotes = row.notes ?? '';
    dialogError = '';
  }

  function submitMeta() {
    if (!dialog) return;
    // Blank means CLEAR (`bulk: null`), not zero. `''`/`null` are the blanks the
    // store's own comment names; a coercion here would silently price a 0 Bulk
    // onto an item the corpus leaves unweighed.
    stash.editMeta(dialog.item, {
      bulk: metaBulk === '' ? null : Number(metaBulk),
      notes: metaNotes.trim() === '' ? null : metaNotes.trim(),
    });
    closeDialog();
  }

  // The bank adjuster: one signed delta per denomination, applied on one click.
  // Blank means "no adjustment", which the store encodes as the integer 0 — the
  // frame `validate_bank` requires all three keys of (write.rs:3182-3199).
  // Held as STRINGS because `bind:value` on a `type=number` input keeps the
  // control's text until it parses, so a blank is `''` here and not `0`; the
  // store's `encodeCoin` is the boundary that decides what is an integer and
  // refuses anything it cannot encode rather than coercing it (see `adjustBank`).
  /** @type {{gp: string, sp: string, cp: string}} */
  let bankDelta = $state({ gp: '', sp: '', cp: '' });

  function adjustBank() {
    stash.adjustBank(bankDelta);
    // Cleared on send, not on success: the send is what this control does, and
    // whether it landed is the notice strip's and the header's to say. Leaving
    // the number in the box invites a second click that double-posts — the harm
    // path PR #311's finding 2 named.
    bankDelta = { gp: '', sp: '', cp: '' };
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
      // §3's frame carries the direction: `out` draws the stash down and names
      // the receiving character, `in` returns stock to the stash. The screen
      // asks which way; the store builds the frame.
      stash.transfer(dialog.item, qty, /** @type {'in' | 'out'} */ (dialogDirection), id);
    } else if (saleCoins) {
      // §3's price frame: `{pp?, gp?, sp?, cp?}` with at least one positive
      // denomination. pp is offered because the server converts it (1 pp =
      // 10 gp, the one conversion site); the store drops zero denominations.
      // The `else if` is the binding guard: the inputs below only render inside
      // the sell branch and only while this entry exists, and `bind:value` on a
      // possibly-null object is exactly the crash `svelte-check` names.
      const coins = saleCoins;
      stash.sell(dialog.item, qty, {
        pp: Number(coins.pp || 0),
        gp: Number(coins.gp || 0),
        sp: Number(coins.sp || 0),
        cp: Number(coins.cp || 0),
      });
    }
    closeDialog();
  }

  // Keyed on `notice.op_id`, the event's own id, NOT `notice.op.op_id`: the
  // transport hands the store `op: null` when the op has already left the queue
  // (sync/index.js:188) and the store files that as `{}`, so a key read off the
  // frame is `undefined` for every such notice — a duplicate key, and Svelte
  // drops one of the rows. `op_id` is on the record for exactly this.

  /** A refused op is opaque to the store (`OpRecord.op` is whatever frame was
   *  sent), so the notice names the frame by the one field that identifies it:
   *  a value's `op` for the stash ops, else the target's `kind`. Declared here
   *  rather than reached with optional chains on `unknown`. */
  /** @param {Record<string, unknown>} op
   *  @returns {{value?: {op?: string}, target?: {kind?: string}}} */
  const noticeFrame = (op) => /** @type {any} */ (op);

  /** @param {Event & {currentTarget: HTMLSelectElement}} event */
  function setDesignation(event) {
    const raw = event.currentTarget.value;
    stash.setQuartermaster(raw === '' ? null : Number(raw));
  }

  /** Bulk is the sheet's number to format; this is the only call site, and it
   *  is the ONLY conversion anywhere: the wire carries Bulk units (design D1's
   *  numeric(4,2)), `tenthsText` takes tenths. A second such step on the client
   *  is the second Bulk-math path D6 forbids. */
  /** @param {number|null} bulk */
  const bulkText = (bulk) => (bulk === null ? '—' : tenthsText(Math.round(bulk * 10)));

  /** FR-4's vocabulary is frozen history riding the row — the screen labels,
   *  it never rewrites. `character:<name at event time>` is already the name. */
  /** @param {string|null} value */
  const place = (value) => (value === null ? '—' : String(value).replace(/^character:/, ''));

  /** @param {ClaimRow} row */
  const coinText = (row) => {
    const parts = [];
    if (row.coins_gp) parts.push(`${row.coins_gp} gp`);
    if (row.coins_sp) parts.push(`${row.coins_sp} sp`);
    if (row.coins_cp) parts.push(`${row.coins_cp} cp`);
    return parts.join(', ');
  };

  /** @param {string|null|undefined} iso */
  const timeText = (iso) => (iso ? String(iso).slice(0, 16).replace('T', ' ') : '');

  /** The dialog's word, in the three places it appears (label, heading, submit).
   *  One function so the `meta` arm cannot be added to one and missed in another
   *  — the shape that made the sell dialog's label and heading disagree once. */
  const dialogHeadings = { transfer: 'Transfer', sell: 'Sell', meta: 'Edit' };
  /** @param {{kind: string}} d */
  const dialogHeading = (d) => dialogHeadings[/** @type {keyof typeof dialogHeadings} */ (d.kind)];
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

  {#if canSell}
    <!-- FR-7's manual adjustment, and the one place in this screen where the
         quartermaster's owner is the gate rather than membership: every
         `FieldTarget::Bank` write goes through `deny_non_quartermaster_owner`
         (`write.rs:995-1003`), so this renders where the frame would land and
         nowhere else. Signed amounts, never a balance: the store sends
         `gp_delta`/`sp_delta`/`cp_delta`, and a box here that carried a resulting
         balance would be the read-modify-write that design D2 exists to refuse.
         A blank box is no adjustment at all — `adjustBank` sends 0 for it and
         refuses the all-zero frame, because the server would record a claim row
         for a movement that never happened. -->
    <div class="adjust">
      <label>
        Gold (gp)
        <input type="number" step="1" bind:value={bankDelta.gp} placeholder="±" />
      </label>
      <label>
        Silver (sp)
        <input type="number" step="1" bind:value={bankDelta.sp} placeholder="±" />
      </label>
      <label>
        Copper (cp)
        <input type="number" step="1" bind:value={bankDelta.cp} placeholder="±" />
      </label>
      <button type="button" onclick={adjustBank}>Adjust bank</button>
    </div>
  {/if}
</section>

<!-- The refused-op records. Inline, self-clearing, non-blocking (§4): a
     refused write is a user-input fact, so it sits next to the work it
     concerns rather than stopping the screen. -->
{#if notices.length > 0}
  <ul class="notices" aria-label="Refused operations">
    {#each notices as notice (notice.op_id)}
      <li class="notice">
        <strong>{notice.outcome}</strong>
        {noticeFrame(notice.op).value?.op ?? noticeFrame(notice.op).target?.kind}
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
            <button type="button" aria-label="Transfer" onclick={() => openTransfer(row)}>
              Transfer
            </button>
          {/if}
          {#if canWrite}
            <!-- gh#336's row controls, and the gate is the part of them that can
                 be got wrong. The tracking issue says "quartermaster-only … gated on
                 the quartermaster check", but the spec, the server, and the epic text
                 all say the row half of this is every party member's:

                 - `specs/014-shared-inventory/spec.md:441` (FR-10): "stash writes
                   (add, transfer, quantity adjust, metadata edit) — ANY character
                   owner of the party; bank mutations (sell, manual adjustment) — the
                   quartermaster character's owner only".
                 - The server matches that reading at the dispatch: only the `sell`
                   arm of a stash target routes to the Bank resource, and the `qty`
                   and `meta` arms go to `deny_non_member` with
                   `Resource::Stash { actor_is_party_member }` (`write.rs:969-992`);
                   `authorize` allows that on membership alone (`auth/authz.rs:134-144`).
                   Every Bank write, by contrast, goes through
                   `deny_non_quartermaster_owner` (`write.rs:995-1003`).
                 - `docs/EPICS.md:225` scopes the quartermaster's exclusivity to
                   "quartermaster-only sell" and "quartermaster-only manual
                   adjustments" (the bank's), and nothing else in the stash.
                 - FR-1's duplicate-name refusal tells the member who hit it to
                   "adjust the existing row's quantity instead" — a lie unless the
                   control renders for the member who was refused.

                 So `canWrite`, the store's own predicate for the member-and-not-GM
                 gate, rather than a second copy of a rule the store already owns.
                 The GM is excluded inside it, which is FR-11's rule: the socket
                 refuses a GM frame outright, so a button that can only produce a
                 refusal is not a control. The bank's adjuster below IS `canSell` —
                 that is the quartermaster's, and the two gates differing is the
                 rule, not an oversight. -->
            <button
              type="button"
              aria-label={`Take one ${row.item_name} from the stash`}
              onclick={() => stash.adjustQty(row.item_name, -1)}
            >
              −1 {row.item_name}
            </button>
            <button
              type="button"
              aria-label={`Add one ${row.item_name} to the stash`}
              onclick={() => stash.adjustQty(row.item_name, 1)}
            >
              +1 {row.item_name}
            </button>
            <button type="button" aria-label="Edit" onclick={() => openMeta(row)}>
              Edit
            </button>
          {/if}
          {#if canSell}
            <!-- FR-6's sell, unchanged in gate and in place. `aria-label` is new
                 and is not decoration: a button's accessible name is its subtree's
                 WHOLE text content, so with three more buttons in this row the
                 implicit name became "Transfer −1 Longsword +1 Longsword Edit".
                 The label names one control to a screen reader and keeps a
                 name-based query pointed at it. -->
            <button type="button" aria-label="Sell" onclick={() => openSell(row)}>
              Sell
            </button>
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
      aria-label={`${dialogHeading(dialog)} ${dialog.item}`}
    >
      <h3>
        {dialogHeading(dialog)} {dialog.item}
        <span class="on-hand">({dialog.quantity} in the stash)</span>
      </h3>

      {#if dialog.kind === 'meta'}
        <!-- FR-8's fields, reached from the row. Absolute and CAS'd, not
             additive: `stash_meta_tx` matches the row on the version this seat read
             and writes these values as given (`write.rs:1418-1450`) — which is why a
             box that carries a value is honest here where the same box is a lie on a
             count or a balance. A blank is the CLEAR arm, not a zero: the server
             distinguishes the three shapes by which keys are present (:1425-1431). -->
        <label>
          Bulk
          <input type="number" step="0.1" min="0" bind:value={metaBulk} placeholder="—" />
        </label>
        <label>
          Note
          <input type="text" bind:value={metaNotes} maxlength="200" placeholder="no note" />
        </label>
      {:else}
        <label>
          Quantity
          <input type="number" min="1" max={dialog.quantity} step="1" bind:value={dialogQty} />
        </label>
      {/if}

      {#if dialog.kind === 'transfer'}
        <label>
          Direction
          <select bind:value={dialogDirection} aria-label="Transfer direction">
            <option value="out">Out of the stash to a character</option>
            <option value="in">Into the stash from a character</option>
          </select>
        </label>
        <label>
          {dialogDirection === 'out' ? 'To character' : 'From character'}
          <select bind:value={dialogCharacter} aria-label="Transfer character">
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
        {#if saleCoins}
          <!-- The block binds, so the entries below are the guard's own type:
               `saleCoins` is non-null exactly while the sell dialog is open
               (`openSell` sets it, `closeDialog` clears it with the dialog), and
               a two-way bind on a possibly-null object is the crash the checker
               names. -->
          <div class="coins">
            <label>
              Platinum (pp)
              <input type="number" min="0" step="1" bind:value={saleCoins.pp} aria-label="Platinum coins" />
            </label>
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
      {/if}

      {#if dialogError}
        <p class="error" role="status">{dialogError}</p>
      {/if}

      <div class="actions">
        <button type="button" onclick={closeDialog}>Cancel</button>
        <button type="button" onclick={dialog.kind === 'meta' ? submitMeta : submitDialog}>
          {dialog.kind === 'sell' ? 'Sell' : dialog.kind === 'meta' ? 'Save' : 'Transfer'}
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
  .adjust {
    display: flex;
    align-items: end;
    gap: 0.5rem;
    margin-top: 0.6rem;
    flex-wrap: wrap;
  }
  .adjust label {
    display: grid;
    gap: 0.2rem;
    font-size: 0.8rem;
    opacity: 0.85;
  }
  .adjust input {
    width: 5.5rem;
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
  .no-book-value {
    margin: 0;
    font-size: 0.85rem;
    padding: 0.3rem 0.5rem;
    border-radius: 3px;
  }
  .book-value {
    background: color-mix(in srgb, var(--good, #5c8a4a) 16%, transparent);
  }
  .no-book-value {
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
