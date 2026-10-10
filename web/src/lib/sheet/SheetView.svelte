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
  import EffectsComposer from './components/EffectsComposer.svelte';

  /** @type {{ character: any, accountSub?: string, editable?: boolean,
    sync?: any, partyId?: number | null, roster?: Array<{id: number, name: string | null}>,
    fetchImpl?: typeof fetch,
    onimport?: () => void, onlogout?: () => void }} */
  let {
    character, // the /api/characters/me payload — or a roster element, same shape
    accountSub = '',
    editable = true,
    sync: providedSync, // the shell's session sync (E10 D2) — one socket per tab
    partyId = null, // the session's party (specs/010) — null hides the composer
    roster = [], // the party roster — the composer's target picker
    fetchImpl, // injectable REST face for tests; the real app uses global fetch
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
  const sheet = createSheetState({ sync, character, fetchImpl });

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
    composerOp,
    partyEffects,
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

  // The composer (specs/010): an editable sheet with a party and a delivered
  // engine output (the vocabulary source). Absent any of the three, the
  // entry point is absent — the GM seat and non-owner drill-ins render no
  // edit control (PRD), and a pre-wire sheet has no vocabulary to pick from.
  let composerOpen = $state(false);
  const canCompose = $derived(Boolean(editable && partyId !== null && $view !== null));

  function openComposer() {
    sheet.clearComposerOp();
    if (partyId !== null) sheet.loadPartyEffects(partyId);
    composerOpen = true;
  }

  // An applied CREATE ack closes the dialog (spec FR-C5); an applied
  // MANAGER op (retarget/end, kind 'effect') must not — the phase-only
  // settle used to discard a half-composed form with it. A manager settle
  // refetches the rows instead (FR-C6: the list is refetched after every
  // op) and the dialog stays open. A denial leaves it open with the reason
  // inline — the state layer owns the verdict.
  $effect(() => {
    if ($composerOp?.phase !== 'applied') return;
    if ($composerOp.kind === 'effect_new') {
      composerOpen = false;
      sheet.clearComposerOp();
    } else {
      if (partyId !== null) sheet.loadPartyEffects(partyId);
      sheet.clearComposerOp();
    }
  });

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

  <div class="effects-head">
    <EffectsStrip effects={$view?.effects ?? []} />
    {#if canCompose}
      <button class="new-effect" onclick={openComposer}>New effect</button>
    {/if}
  </div>

  {#if composerOpen && partyId !== null}
    <EffectsComposer
      open={composerOpen}
      onclose={() => (composerOpen = false)}
      partyId={partyId}
      characterId={character.character.id}
      {roster}
      view={$view}
      effectsState={$partyEffects}
      composerOpState={$composerOp}
      oncreate={sheet.createEffect}
      onretarget={sheet.retargetEffect}
      onend={sheet.endEffect}
    />
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
      />
    </div>
  </div>
</div>

<style>
  .effects-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .new-effect {
    background: none;
    border: 1px solid var(--gold-dim, #b8963e);
    border-radius: 999px;
    color: var(--gold, #d4af5f);
    font-size: 0.8rem;
    padding: 2px 12px;
    cursor: pointer;
  }
</style>
