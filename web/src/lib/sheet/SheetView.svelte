<script>
  // SheetView — the composition root for one character's live sheet.
  // Owns the sheet state layer; components receive stores, never the
  // socket. The sync arrives from outside when the party shell mounts
  // this view (one socket per tab, E10 D2); standalone it builds its own
  // against the POC party as before.
  import { createSync, partySocketUrl } from '../sync/index.js';
  import { createSheetState } from './state.js';

  import CharacterHeader from './components/CharacterHeader.svelte';
  import StatsPane from './components/StatsPane.svelte';
  import MagicPane from './components/MagicPane.svelte';
  import CompanionsPanel from './components/CompanionsPanel.svelte';
  import InventoryPanel from './components/InventoryPanel.svelte';
  import StrikesPane from './components/StrikesPane.svelte';
  import FeatsPanel from './components/FeatsPanel.svelte';
  import EffectsStrip from './components/EffectsStrip.svelte';
  import ConditionPicker from './components/ConditionPicker.svelte';
  import AddCustomForm from './components/AddCustomForm.svelte';
  import Dialog from './components/Dialog.svelte';
  import { createCustomStore } from '../rules/custom-store.js';

  /** @type {{ character: any, accountSub?: string, editable?: boolean,
    partyId?: number,
    sync?: any, onimport?: () => void, onlogout?: () => void }} */
  let {
    character, // the /api/characters/me payload — or a roster element, same shape
    accountSub = '',
    editable = true,
    partyId = 1, // the shell passes session.roster.party_id; standalone is the POC party
    sync: providedSync, // the shell's session sync (E10 D2) — one socket per tab
    onimport,
    onlogout,
  } = $props();

  // Standalone/test path — the shell ALWAYS provides the session sync;
  // this self-construction against the POC party is the fallback only.
  const POC_PARTY_ID = 1;
  const sync =
    providedSync ??
    createSync({
      url: partySocketUrl(POC_PARTY_ID),
      storage: localStorage,
      accountSub,
    });
  const sheet = createSheetState({ sync, character, partyId });

  // ---- E9: the party's custom rows (store: rules/custom-store.js) --------
  const customStore = createCustomStore({ partyId });
  /** Which AddCustomForm is open: null | 'item' | 'spell' | 'condition'. */
  let customFormKind = $state(/** @type {null | 'item' | 'spell' | 'condition'} */ (null));
  let pickerOpen = $state(false);
  // Party-wide custom rows land on the next fetch (D3) — read at boot, and
  // a failed read degrades to the last fetched state (the store's rule).
  let customSpellRows = $state(/** @type {any[]} */ ([]));
  let customConditionRows = $state(/** @type {any[]} */ ([]));
  $effect(() => {
    const unsubscribeSpells = customStore.spells.subscribe((rows) => (customSpellRows = rows));
    const unsubscribeConditions = customStore.conditions.subscribe(
      (rows) => (customConditionRows = rows),
    );
    customStore.refresh('spell');
    customStore.refresh('condition');
    return () => {
      unsubscribeSpells();
      unsubscribeConditions();
    };
  });
  // Items bridge the store into the inventory panel with their qty-1
  // optimistic display; qty writes ride the ordinary inv path by exact name.
  let customItemRows = $state(/** @type {any[]} */ ([]));
  $effect(() => {
    const unsubscribe = customStore.items.subscribe((rows) => {
      customItemRows = rows.map((row) => ({ ...row, qty: 1 }));
    });
    customStore.refresh('item');
    return unsubscribe;
  });

  /** The AddCustomForm's submit: one store call per kind; an item create
   * also surfaces its qty-1 inventory row immediately (US-3 AC-1). */
  async function submitCustom(/** @type {any} */ fields) {
    if (customFormKind === 'item') {
      const row = await customStore.create('item', fields);
      customItemRows = [{ ...row, qty: 1 }, ...customItemRows];
    } else if (customFormKind === 'spell') {
      await customStore.create('spell', fields);
    } else if (customFormKind === 'condition') {
      await customStore.create('condition', fields);
    }
    customFormKind = null;
  }

  // Stores destructure into locals: `$view` and friends are the template's
  // auto-subscriptions; the write surface stays on `sheet`.
  const {
    view,
    hp,
    tempHp,
    money,
    focusCurrent,
    focusMax,
    heroPoints,
    heroMax,
    daily,
    slots,
    qtyMap,
    syncing,
    offline,
    opErrors,
  } = sheet;

  /**
   * Prepare a spell: the named slot when given, else the first open slot
   * at that rank (the spellbook's Prepare button). A full rank stays full —
   * slot counts anchor to the export (FR-12).
   * @param {string} casterKey @param {{rank: number, slot_index: number}} row
   * @param {string} spell
   */
  function prepare(casterKey, row, spell) {
    let index = row.slot_index;
    if (index < 0) {
      const open = $slots.find(
        (/** @type {any} */ candidate) =>
          candidate.caster_key === casterKey &&
          candidate.rank === row.rank &&
          !candidate.prepared_spell,
      );
      if (!open) return;
      index = /** @type {any} */ (open).slot_index;
    }
    sheet.writeSlot(casterKey, row.rank, index, { prepared: spell });
  }

  $effect(() => {
    sync.connect();
    return () => {
      sheet.destroy();
      // A borrowed sync belongs to the shell — backing out of a drill-in
      // must not tear down the party screen's link (one socket per tab).
      // Only the standalone sync, which this view built, dies here.
      if (!providedSync) sync.disconnect();
    };
  });

  const identity = character.base_sheet.identity;

  function subline() {
    const parts = [identity.ancestry, identity.class, identity.heritage].filter(Boolean);
    return parts.join(' · ');
  }
</script>

<div class="sheet">
  <CharacterHeader
    name={identity.name}
    subline={subline()}
    level={$view?.render_base?.level ?? null}
    {editable}
    offline={$offline}
    wireReady={$view !== null}
    syncing={$syncing}
    onadjustlevel={(/** @type {number} */ level) => sheet.writeLevelAdjust(level)}
    onnewday={() => sheet.newDay()}
    onimport={onimport}
    onlogout={onlogout}
  />

  <EffectsStrip effects={$view?.effects ?? []} customConditions={customConditionRows} />

  {#if editable}
    <div style="margin:-4px 0 8px">
      <button class="btn" style="font-size:12px;padding:2px 9px" onclick={() => (pickerOpen = true)}>
        Add condition
      </button>
    </div>
  {/if}

  {#if pickerOpen}
    <ConditionPicker
      {partyId}
      targetId={character.character.id}
      sourceId={character.character.id}
      open={pickerOpen}
      onclose={() => (pickerOpen = false)}
      onapply={(/** @type {any} */ create) => sheet.writeEffect(create)}
      onsubmitcustom={(/** @type {any} */ fields) => customStore.create('condition', fields)}
      customRows={customConditionRows}
    />
  {/if}

  {#if customFormKind}
    <Dialog
      open={customFormKind !== null}
      title="Add custom content"
      onclose={() => (customFormKind = null)}
    >
      <AddCustomForm
        kind={/** @type {'item' | 'spell' | 'condition'} */ (customFormKind)}
        onsubmit={submitCustom}
        oncancel={() => (customFormKind = null)}
      />
    </Dialog>
  {/if}

  <div class="board">
    <div class="col">
      {#if $view}
        <StatsPane
          view={$view}
          baseSheet={character.base_sheet}
          hp={$hp}
          temp={$tempHp}
          money={$money}
          focusCurrent={$focusCurrent}
          focusMax={$focusMax ?? 0}
          heroPoints={$heroPoints}
          heroMax={$heroMax ?? 0}
          opErrors={$opErrors}
          {editable}
          offline={$offline}
          ondamage={(/** @type {number} */ amount) => sheet.writeHp($hp.value - amount)}
          onheal={(/** @type {number} */ amount) => sheet.writeHp($hp.value + amount)}
          onfull={() => sheet.writeHp($view.render_base.hp_max)}
          ontemp={(/** @type {number} */ value) => sheet.writeTempHp(value)}
          onfocus={(/** @type {number} */ value) => sheet.writeFocus(value)}
          onhero={(/** @type {number} */ value) => sheet.writeHeroPoints(value)}
        />
        <StrikesPane view={$view} />
      {:else}
        <!-- Engine output not on the wire yet: pane-shaped skeletons, no
             invented numbers (design §7's loading state; derived is never
             stored client-side, so there is nothing honest to show). -->
        <div class="panel" role="status" aria-label="Loading your stats">
          <div class="skeleton" style="width: 40%; height: 20px; margin-bottom: 10px;"></div>
          <div class="skeleton" style="width: 100%; height: 320px;"></div>
        </div>
        <div class="panel" role="status" aria-label="Loading your strikes">
          <div class="skeleton" style="width: 40%; height: 20px; margin-bottom: 10px;"></div>
          <div class="skeleton" style="width: 100%; height: 180px;"></div>
        </div>
      {/if}
      <FeatsPanel baseSheet={character.base_sheet} {editable} />
    </div>
    <div class="col">
      <MagicPane
        baseSheet={character.base_sheet}
        slots={$slots}
        view={$view}
        daily={$daily}
        opErrors={$opErrors}
        {editable}
        offline={$offline}
        oncast={(/** @type {string} */ casterKey, /** @type {any} */ row, /** @type {boolean} */ used) =>
          sheet.writeSlot(casterKey, row.rank, row.slot_index, { used })}
        onprepare={(/** @type {string} */ casterKey, /** @type {any} */ row, /** @type {string} */ spell) =>
          prepare(casterKey, row, spell)}
        onreset={(/** @type {string} */ casterKey) => sheet.resetPrep(casterKey)}
        ondaily={(/** @type {any} */ next) => sheet.writeDaily(next)}
        customSpells={customSpellRows}
        onaddcustom={() => (customFormKind = 'spell')}
      >
        {#snippet companionsSlot()}
          {#if $view}
            <CompanionsPanel baseSheet={character.base_sheet} view={$view} />
          {:else}
            <p class="meta" role="status">Character numbers still loading…</p>
          {/if}
        {/snippet}
      </MagicPane>
    </div>
    <div class="col">
      <InventoryPanel
        baseSheet={character.base_sheet}
        itemBulk={character.item_bulk ?? {}}
        itemTraits={character.item_traits ?? {}}
        qtyMap={$qtyMap}
        money={$money}
        opErrors={$opErrors}
        {editable}
        offline={$offline}
        onqty={(/** @type {string} */ name, /** @type {number} */ qty) => sheet.writeItemQty(name, qty)}
        onmoney={(/** @type {any} */ next) => sheet.writeMoney(next)}
        customItems={customItemRows}
        onaddcustom={() => (customFormKind = 'item')}
      />
    </div>
  </div>
</div>
