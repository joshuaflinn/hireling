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
    syncing,
    offline,
  } = sheet;

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
    </div>
  </div>
</div>
