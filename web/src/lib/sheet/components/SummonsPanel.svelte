<script>
  /* global crypto */
  // Summons + current minions (Dave's #pMinSum / #pMinCur, specs/015 slice 7).
  // No hardcoded bestiary: for every summon spell the character knows, the
  // spell's own text names the creature trait ("has the undead trait"), the
  // GM Screen "Summon Trait" page gives rank → max creature level, and the
  // creature list comes from the imported core bestiaries (common only, per
  // the trait's rule). Current minions are synced in sheet_ui.minions.
  import { list, lookup } from '../../corpus/client.js';
  import { tipTrigger } from '../../corpus/tip.svelte.js';
  import { signed } from '../../engine/format.js';

  /** @type {{ knownSpells: string[], maxRank: number, sheetUi: {notes: any[], minions: any[], layout: any},
   *   editable?: boolean, offline?: boolean, onwrite?: (next: any) => void }} */
  let { knownSpells, maxRank, sheetUi, editable = true, offline = false, onwrite } = $props();

  /** @type {Array<{name: string, trait: string, time: string, duration: string}>} */
  let summonSpells = $state([]);
  /** rank → max creature level */
  let maxLevel = $state(/** @type {Record<number, number>} */ ({}));
  /** @type {Record<string, any[]>} trait → creatures */
  let creatures = $state({});
  let pick = $state('');
  let loadError = $state('');

  const minions = $derived(sheetUi?.minions ?? []);

  $effect(() => {
    const names = knownSpells.filter((n) => /^Summon /.test(n));
    if (!names.length) return;
    lookup('spell', { names })
      .then((rows) => {
        summonSpells = rows
          .filter((r) => r.traits.includes('summon'))
          .map((r) => {
            const m = r.description.match(/has the (\w[\w-]*) trait/i);
            return {
              name: r.name,
              trait: m ? m[1].toLowerCase() : '',
              time: String(r.detail?.time ?? ''),
              duration: r.detail?.duration?.sustained ? `sustained up to ${r.detail.duration.value || '1 minute'}` : String(r.detail?.duration?.value ?? ''),
            };
          })
          .filter((s) => s.trait);
        if (summonSpells.length && !pick) pick = summonSpells[0].name;
      })
      .catch(() => (loadError = 'Summon spells unavailable.'));
    list('rules_page', { q: 'Summon Trait' })
      .then((rows) => {
        const page = rows.find((r) => r.name === 'Summon Trait');
        /** @type {Record<number, number>} */
        const table = {};
        for (const m of (page?.description ?? '').matchAll(/<tr><td>(\d+)\w*<\/td><td>([–−-]?\d+)<\/td><\/tr>/g)) {
          table[Number(m[1])] = Number(m[2].replace(/[–−]/, '-'));
        }
        maxLevel = table;
      })
      .catch(() => (loadError = 'Summon table unavailable.'));
  });

  const current = $derived(summonSpells.find((s) => s.name === pick) ?? null);
  const cap = $derived(maxLevel[maxRank] ?? null);

  $effect(() => {
    const trait = current?.trait;
    if (!trait || cap === null || creatures[trait]) return;
    list('creature', { trait, max_level: cap })
      .then((rows) => {
        creatures = { ...creatures, [trait]: rows.filter((r) => (r.rarity ?? 'common') === 'common') };
      })
      .catch(() => (loadError = 'Bestiary unavailable.'));
  });

  const byLevel = $derived.by(() => {
    const rows = current ? (creatures[current.trait] ?? []) : [];
    /** @type {Map<number, any[]>} */
    const m = new Map();
    for (const r of rows) m.set(r.level, [...(m.get(r.level) ?? []), r]);
    return [...m.entries()].sort((a, b) => a[0] - b[0]);
  });

  const minRank = (/** @type {number} */ lvl) => {
    for (let r = 1; r <= 10; r += 1) if ((maxLevel[r] ?? -99) >= lvl) return r;
    return null;
  };
  const ord = (/** @type {number} */ n) => (n === 1 ? '1st' : n === 2 ? '2nd' : n === 3 ? '3rd' : `${n}th`);

  function summon(/** @type {any} */ c) {
    if (!editable || offline) return;
    const hp = Number(c.detail?.hp ?? 0) || 1;
    const n = minions.filter((m) => m.name === c.name).length + 1;
    onwrite?.({
      ...sheetUi,
      minions: [...minions, { id: `m-${crypto.randomUUID().slice(0, 8)}`, name: c.name, n, hp, hp_max: hp, corpus_id: c.id }],
    });
  }
  function setHp(/** @type {any} */ m, /** @type {number} */ hp) {
    onwrite?.({
      ...sheetUi,
      minions: minions.map((x) => (x.id === m.id ? { ...x, hp: Math.max(0, Math.min(x.hp_max, Math.round(hp))) } : x)),
    });
  }
  function dismiss(/** @type {any} */ m) {
    onwrite?.({ ...sheetUi, minions: minions.filter((x) => x.id !== m.id) });
  }
</script>

<div class="summons">
  <h3>Summons</h3>
  {#if loadError}<p class="meta">{loadError}</p>{/if}
  {#if !summonSpells.length}
    <p class="meta">No summon spells known.</p>
  {:else}
    {#if summonSpells.length > 1}
      <div class="tabs">
        {#each summonSpells as s (s.name)}
          <button class:on={pick === s.name} onclick={() => (pick = s.name)}>{s.name}</button>
        {/each}
      </div>
    {/if}
    {#if current}
      <p class="pet-note">
        <b use:tipTrigger={{ kind: 'spell', name: current.name }}>{current.name}</b>
        · {current.time === '3' ? '3 actions' : current.time}{current.duration ? `, ${current.duration}` : ''}.
        Common <i>{current.trait}</i> creatures up to level {cap ?? '…'} (your {ord(maxRank)}-rank slot).
        Click a name for its stat block{editable ? '; + summons it' : ''}.
      </p>
      {#each byLevel as [lvl, rows] (lvl)}
        {@const r = minRank(lvl)}
        <div class="sumlvl">Creature {signed(lvl)}{#if r}<span class="h3s"> · {ord(r)}-rank slot or higher</span>{/if}</div>
        <div class="sumlinks">
          {#each rows as c (c.id)}
            <span class="sumlink">
              <span use:tipTrigger={{ kind: 'creature', id: c.source_id, name: c.name }}>{c.name}</span>
              {#if editable}<button class="add" aria-label="Summon {c.name}" disabled={offline} onclick={() => summon(c)}>+</button>{/if}
            </span>
          {/each}
        </div>
      {/each}
    {/if}
  {/if}

  <h3>Current minions</h3>
  {#each minions as m (m.id)}
    <div class="minion">
      <span class="nm" use:tipTrigger={{ kind: 'creature', name: m.name }}>{m.name}{m.n > 1 ? ` ${m.n}` : ''}</span>
      <span class="hpbar"><i style:width="{(m.hp / Math.max(1, m.hp_max)) * 100}%"></i></span>
      <span class="hpn">{m.hp}/{m.hp_max}</span>
      {#if editable}
        <button aria-label="Damage {m.name}" disabled={offline} onclick={() => setHp(m, m.hp - 1)}>−1</button>
        <button aria-label="Heal {m.name}" disabled={offline} onclick={() => setHp(m, m.hp + 1)}>+1</button>
        <button class="x" aria-label="Dismiss {m.name}" disabled={offline} onclick={() => dismiss(m)}>Dismiss</button>
      {/if}
    </div>
  {:else}
    <p class="meta">None active. Summon one above.</p>
  {/each}
</div>

<style>
  .summons h3 {
    font-family: var(--serif);
    color: var(--gold);
    font-size: 16px;
    margin: 12px 0 4px;
    font-weight: 400;
  }
  .tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin: 4px 0;
  }
  .tabs button {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 6px;
    padding: 2px 9px;
    cursor: pointer;
  }
  .tabs button.on {
    color: var(--gold);
    border-color: var(--gold-dim);
  }
  .pet-note {
    font-size: 12.5px;
    color: var(--muted);
    margin: 4px 0 6px;
  }
  .pet-note b {
    color: var(--text);
  }
  .sumlvl {
    font-size: 12px;
    font-weight: 700;
    color: var(--text);
    margin: 8px 0 3px;
  }
  .h3s {
    font-weight: 400;
    color: var(--muted);
  }
  .sumlinks {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 6px;
  }
  .sumlink {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    border: 1px solid var(--line2);
    border-radius: 999px;
    padding: 1px 4px 1px 9px;
    font-size: 13px;
    cursor: help;
  }
  .sumlink .add {
    background: none;
    border: 1px solid var(--gold-dim);
    color: var(--gold);
    border-radius: 999px;
    width: 18px;
    height: 18px;
    line-height: 1;
    font-size: 12px;
    cursor: pointer;
  }
  .minion {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 90px auto auto auto auto;
    gap: 6px;
    align-items: center;
    padding: 3px 0;
  }
  .minion .nm {
    cursor: help;
  }
  .hpbar {
    height: 6px;
    background: #2a2f38;
    border-radius: 3px;
    overflow: hidden;
  }
  .hpbar i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #c0462f, #e0a43c, #6fbf5a);
  }
  .hpn {
    font-variant-numeric: tabular-nums;
    font-size: 12.5px;
    color: var(--muted);
  }
  .minion button {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 5px;
    padding: 1px 6px;
    font-size: 12px;
    cursor: pointer;
  }
</style>
