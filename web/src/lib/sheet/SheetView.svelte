<script>
  // SheetView — the composition root for one character's live sheet.
  // Owns the sheet state layer (the only place createSync is constructed
  // on this surface); components receive stores, never the socket.
  //
  // POC party: the seed guarantees exactly one party (E7 design), so the
  // socket URL addresses party 1 — a named constant, revisited with E10.
  import { createSync, partySocketUrl } from '../sync/index.js';
  import { createSheetState } from './state.js';

  import CharacterHeader from './components/CharacterHeader.svelte';
  import StatsPane from './components/StatsPane.svelte';
  import MagicPane from './components/MagicPane.svelte';
  import CompanionsPanel from './components/CompanionsPanel.svelte';
  import InventoryPanel from './components/InventoryPanel.svelte';
  import StrikesPane from './components/StrikesPane.svelte';
  import FeatsPanel from './components/FeatsPanel.svelte';

  /** @type {{ character: any, accountSub?: string, editable?: boolean }} */
  let {
    character, // the /api/characters/me payload
    accountSub = '',
    editable = true,
  } = $props();

  const POC_PARTY_ID = 1;

  const sync = createSync({
    url: partySocketUrl(POC_PARTY_ID),
    storage: localStorage,
    accountSub,
  });
  const sheet = createSheetState({ sync, character });

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
      sync.disconnect();
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
    level={$view.level}
    {editable}
    offline={$offline}
    syncing={$syncing}
    onadjustlevel={(/** @type {number} */ level) => sheet.writeLevelAdjust(level)}
    onnewday={() => sheet.newDay()}
  />

  <div class="board">
    <div class="col">
      <StatsPane
        view={$view}
        baseSheet={character.base_sheet}
        hp={$hp}
        temp={$tempHp}
        money={$money}
        focusCurrent={$focusCurrent}
        focusMax={$focusMax}
        heroPoints={$heroPoints}
        heroMax={$heroMax}
        {editable}
        offline={$offline}
        ondamage={(/** @type {number} */ amount) => sheet.writeHp($hp.value - amount)}
        onheal={(/** @type {number} */ amount) => sheet.writeHp($hp.value + amount)}
        onfull={() => sheet.writeHp($view.hp_max.total)}
        ontemp={(/** @type {number} */ value) => sheet.writeTempHp(value)}
        onfocus={(/** @type {number} */ value) => sheet.writeFocus(value)}
        onhero={(/** @type {number} */ value) => sheet.writeHeroPoints(value)}
      />
      <StrikesPane view={$view} />
      <FeatsPanel baseSheet={character.base_sheet} {editable} />
    </div>
    <div class="col">
      <MagicPane
        baseSheet={character.base_sheet}
        slots={$slots}
        view={$view}
        daily={$daily}
        {editable}
        offline={$offline}
        oncast={(/** @type {string} */ casterKey, /** @type {any} */ row, /** @type {boolean} */ used) =>
          sheet.writeSlot(casterKey, row.rank, row.slot_index, { used })}
        onprepare={(/** @type {string} */ casterKey, /** @type {any} */ row, /** @type {string} */ spell) =>
          prepare(casterKey, row, spell)}
        onreset={(/** @type {string} */ casterKey) => sheet.resetPrep(casterKey)}
        ondaily={(/** @type {any} */ next) => sheet.writeDaily(next)}
      >
        {#snippet companionsSlot()}
          <CompanionsPanel baseSheet={character.base_sheet} view={$view} />
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
        {editable}
        offline={$offline}
        onqty={(/** @type {string} */ name, /** @type {number} */ qty) => sheet.writeItemQty(name, qty)}
        onmoney={(/** @type {any} */ next) => sheet.writeMoney(next)}
      />
    </div>
  </div>
</div>
