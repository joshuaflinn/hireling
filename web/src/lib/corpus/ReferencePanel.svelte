<script>
  // Reference column (specs/015 slice 3; Dave's #pRef): Actions (Encounter /
  // Skill Actions / Exploration), Conditions, Rules. Every row and every
  // summary comes from the Foundry pack — the GM Screen journal's tables for
  // the action and condition summaries, the actions/conditions packs for
  // the detail cards. The character's own numbers ride alongside (Strikes,
  // skill modifiers) the way Dave's sheet shows them.
  import { adoptHTML } from '../util/inert-html.js';
  import { enrich } from './enrich.js';
  import { list } from './client.js';
  import { tipTrigger, pinTip } from './tip.svelte.js';
  import { glyphHTML } from './glyphs.js';

  /** @type {{ strikes?: Array<any>, skills?: Array<{name: string, mod: number, rank: number}>, perception?: number | null }} */
  let { strikes = [], skills = [], perception = null } = $props();

  let tab = $state('act');
  let sub = $state('encounter');
  let condSort = $state('cat');
  /** @type {Record<string, any>} */
  let pages = $state({});
  let loadError = $state('');
  /** @type {HTMLElement | null} */
  let rulesHost = $state(null);

  $effect(() => {
    list('rules_page')
      .then((rows) => {
        /** @type {Record<string, any>} */
        const byName = {};
        for (const row of rows) byName[row.name] = row;
        pages = byName;
      })
      .catch(() => (loadError = 'Rules reference unavailable.'));
  });

  const sg = (/** @type {number} */ n) => (n >= 0 ? `+${n}` : `−${Math.abs(n)}`);

  /**
   * Parse one GM Screen table into rows: first cell = linked name (+ glyph),
   * last cell = summary; middle cells may hold trait sections.
   * @param {string} html
   */
  function tableRows(html) {
    if (typeof DOMParser === 'undefined' || !html) return [];
    const doc = new DOMParser().parseFromString(html, 'text/html');
    /** @type {Array<{name: string, uuid: string | null, cost: string | null, summary: string, group: string | null}>} */
    const rows = [];
    for (const tr of doc.querySelectorAll('tbody tr')) {
      const cells = [...tr.querySelectorAll('td')];
      if (cells.length < 2) continue;
      const first = cells[0].innerHTML;
      const m = first.match(/@UUID\[Compendium\.pf2e\.[^.]+\.Item\.([A-Za-z0-9]+)\]\{([^}]*)\}/);
      if (!m) continue;
      const iconText = cells[0].querySelector('.pf2-icon, .action-glyph')?.textContent?.trim() ?? null;
      const cost = iconText === 'A' ? '1' : iconText === 'D' ? '2' : iconText === 'T' ? '3' : iconText;
      rows.push({ name: m[2], uuid: m[1], cost, summary: cells[cells.length - 1].innerHTML, group: null });
    }
    return rows;
  }

  /** Skill Actions table: Skill | Key | Untrained | Trained → grouped rows. */
  function skillRows(/** @type {string} */ html) {
    if (typeof DOMParser === 'undefined' || !html) return [];
    const doc = new DOMParser().parseFromString(html, 'text/html');
    /** @type {Array<{skill: string, actions: Array<{name: string, uuid: string, cost: string | null, trained: boolean}>}>} */
    const out = [];
    for (const tr of doc.querySelectorAll('tbody tr')) {
      const cells = [...tr.querySelectorAll('td')];
      if (cells.length < 4) continue;
      const skill = cells[0].textContent?.trim() ?? '';
      const actions = [];
      for (const [ci, trained] of /** @type {Array<[number, boolean]>} */ ([[2, false], [3, true]])) {
        for (const p of cells[ci].querySelectorAll('p')) {
          const m = p.innerHTML.match(/@UUID\[Compendium\.pf2e\.[^.]+\.Item\.([A-Za-z0-9]+)\]\{([^}]*)\}/);
          if (!m) continue;
          const g = p.querySelector('.action-glyph, .pf2-icon')?.textContent?.trim() ?? null;
          const cost = g === 'A' ? '1' : g === 'D' ? '2' : g === 'T' ? '3' : g === 'R' ? 'R' : g === 'F' ? 'F' : null;
          actions.push({ name: m[2], uuid: m[1], cost, trained });
        }
      }
      if (actions.length) out.push({ skill, actions });
    }
    return out;
  }

  const basic = $derived(tableRows(pages['Basic Actions']?.description ?? ''));
  const explore = $derived(tableRows(pages['Exploration Activities']?.description ?? ''));
  const conditions = $derived(tableRows(pages['Conditions']?.description ?? ''));
  const skillTable = $derived(skillRows(pages['Skill Actions']?.description ?? ''));

  const skillMod = $derived((/** @type {string} */ name) => {
    const hit = skills.find((s) => s.name.toLowerCase() === name.toLowerCase());
    return hit ? hit : null;
  });

  /** Plain-text summary for the right-hand column (enriched, tags stripped). */
  function plain(/** @type {string} */ html) {
    if (typeof DOMParser === 'undefined') return html;
    const doc = new DOMParser().parseFromString(enrich(html), 'text/html');
    return doc.body.textContent?.trim() ?? '';
  }

  /** Dave's condition groups are the books' grouping — sort A–Z or by the
   * journal's own order (the table is alphabetical; "by category" groups by
   * the condition's first trait in the pack where present). */
  const condsSorted = $derived(
    condSort === 'az' ? [...conditions].sort((a, b) => a.name.localeCompare(b.name)) : conditions,
  );

  const RULE_PAGES = [
    'Bonuses and Penalties',
    'Death, Dying and Unconscious',
    'Terrain and Cover',
    'Size and Reach',
    'Falling',
    'Treat Wounds',
    'Counteracting',
    'Light',
    'Senses',
    'Detecting Creatures',
    'Bulk and Encumbered',
    'Hero Points',
    'Resting',
    'Summon Trait',
    'Simple DCs',
    'DCs by Level',
  ];
  let rulePage = $state('Bonuses and Penalties');

  $effect(() => {
    if (tab !== 'rules' || !rulesHost) return;
    const page = pages[rulePage];
    adoptHTML(rulesHost, page ? enrich(page.description) : '<p class="none">Loading…</p>');
  });

  function onRulesClick(/** @type {MouseEvent} */ ev) {
    const a = /** @type {HTMLElement} */ (ev.target).closest?.('a.xref');
    if (!a) return;
    const el = /** @type {HTMLElement} */ (a);
    if (el.dataset.uuidKind) pinTip({ kind: el.dataset.uuidKind, id: el.dataset.uuidId, name: el.textContent ?? '' }, el);
    else if (el.dataset.act) pinTip({ kind: 'action', name: el.textContent ?? '' }, el);
  }

  /** svelte action: adopt inert glyph svg */
  function glyph(/** @type {HTMLElement} */ node, /** @type {string | null} */ cost) {
    const set = (/** @type {string | null} */ c) => adoptHTML(node, glyphHTML(c));
    set(cost);
    return { update: set };
  }
</script>

<section class="panel ref" aria-label="Reference">
  <h2>Reference <small>{tab === 'rules' ? 'from the pf2e GM Screen' : 'Click for rules'}</small></h2>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === 'act'} class:on={tab === 'act'} onclick={() => (tab = 'act')}>Actions</button>
    <button role="tab" aria-selected={tab === 'conds'} class:on={tab === 'conds'} onclick={() => (tab = 'conds')}>Conditions</button>
    <button role="tab" aria-selected={tab === 'rules'} class:on={tab === 'rules'} onclick={() => (tab = 'rules')}>Rules</button>
  </div>
  {#if loadError}<p class="meta">{loadError}</p>{/if}

  {#if tab === 'act'}
    <div class="tabs sub" role="tablist">
      <button role="tab" aria-selected={sub === 'encounter'} class:on={sub === 'encounter'} onclick={() => (sub = 'encounter')}>Encounter</button>
      <button role="tab" aria-selected={sub === 'skills'} class:on={sub === 'skills'} onclick={() => (sub = 'skills')}>Skill Actions</button>
      <button role="tab" aria-selected={sub === 'exploration'} class:on={sub === 'exploration'} onclick={() => (sub = 'exploration')}>Exploration</button>
    </div>
    {#if sub === 'encounter'}
      {#if strikes.length}
        <div class="grp">Strikes</div>
        {#each strikes as s (s.name)}
          <div class="row arow" use:tipTrigger={{ kind: 'item', name: s.base ?? s.name }}>
            <span use:glyph={'1'}></span>
            <span class="nm">{s.name}</span>
            <span class="q">{s.line}</span>
          </div>
        {/each}
      {/if}
      <div class="grp">Basic</div>
      {#each basic as a (a.uuid)}
        <div class="row arow" use:tipTrigger={{ kind: 'action', id: a.uuid ?? undefined, name: a.name }}>
          <span use:glyph={a.cost}></span>
          <span class="nm">{a.name}</span>
          <span class="q">{a.name === 'Seek' && perception != null ? `Perception ${sg(perception)} · ` : ''}{plain(a.summary)}</span>
        </div>
      {/each}
    {:else if sub === 'skills'}
      {#each skillTable as group (group.skill)}
        {@const mine = skillMod(group.skill)}
        <div class="grp">{group.skill}{#if mine} <span class="grp-mod">{sg(mine.mod)}{mine.rank ? '' : ' · untrained'}</span>{/if}</div>
        {#each group.actions as a (a.uuid)}
          {@const locked = a.trained && mine && !mine.rank}
          <div class="row arow" class:locked-out={locked} use:tipTrigger={{ kind: 'action', id: a.uuid, name: a.name }}>
            <span use:glyph={a.cost}></span>
            <span class="nm">{a.name}</span>
            <span class="q">{a.trained ? 'trained' : ''}{mine ? ` ${sg(mine.mod)}` : ''}</span>
          </div>
        {/each}
      {/each}
    {:else}
      <div class="grp">Exploration</div>
      {#each explore as a (a.uuid)}
        <div class="row arow" use:tipTrigger={{ kind: 'action', id: a.uuid ?? undefined, name: a.name }}>
          <span></span>
          <span class="nm">{a.name}</span>
          <span class="q">{plain(a.summary)}</span>
        </div>
      {/each}
    {/if}
  {:else if tab === 'conds'}
    <div class="tabs csort">
      <span>Sort</span>
      <button class:on={condSort === 'cat'} onclick={() => (condSort = 'cat')}>Book order</button>
      <button class:on={condSort === 'az'} onclick={() => (condSort = 'az')}>A–Z</button>
    </div>
    {#each condsSorted as c (c.uuid)}
      <div class="row arow crow" use:tipTrigger={{ kind: 'condition', id: c.uuid ?? undefined, name: c.name }}>
        <span class="nm">{c.name}</span>
        <span class="q">{plain(c.summary)}</span>
      </div>
    {/each}
  {:else}
    <div class="tabs rules-pick">
      {#each RULE_PAGES.filter((p) => pages[p]) as p (p)}
        <button class:on={rulePage === p} onclick={() => (rulePage = p)}>{p}</button>
      {/each}
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <!-- Reason: delegated clicks on adopted cross-reference anchors. -->
    <div class="rules-body" bind:this={rulesHost} onclick={onRulesClick}></div>
  {/if}
</section>

<style>
  .ref h2 small {
    float: right;
    font: 400 12.5px var(--sans);
    color: var(--muted);
    margin-top: 6px;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 6px 0;
  }
  .tabs button {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 6px;
    padding: 3px 10px;
    font: inherit;
    font-size: 13.5px;
    cursor: pointer;
  }
  .tabs button.on {
    border-color: var(--gold-dim);
    color: var(--gold);
  }
  .tabs.csort span {
    color: var(--muted);
    font-size: 12px;
    align-self: center;
  }
  .tabs.rules-pick button {
    font-size: 12px;
    padding: 2px 8px;
  }
  .grp {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1.2px;
    color: var(--muted);
    margin: 10px 0 2px;
    font-weight: 700;
  }
  .grp-mod {
    color: var(--gold);
    letter-spacing: 0;
    text-transform: none;
  }
  .row.arow {
    grid-template-columns: 36px minmax(110px, auto) minmax(0, 1fr);
    cursor: help;
  }
  .row.crow {
    grid-template-columns: minmax(110px, auto) minmax(0, 1fr);
  }
  .row .q {
    text-align: right;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row.locked-out {
    opacity: 0.45;
  }
  .row :global(.glyph) {
    color: var(--gold);
    display: inline-flex;
    align-items: center;
  }
  .row :global(.glyph svg) {
    height: 13px;
  }
  .rules-body {
    font-size: 13px;
    line-height: 1.45;
  }
  .rules-body :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: 6px 0;
  }
  .rules-body :global(td),
  .rules-body :global(th) {
    border: 1px solid var(--line2);
    padding: 3px 6px;
    vertical-align: top;
  }
  .rules-body :global(a.xref) {
    color: var(--gold);
    text-decoration: underline dotted;
    cursor: pointer;
  }
  .rules-body :global(b.check),
  .rules-body :global(b.roll) {
    color: #e9c47c;
  }
</style>
