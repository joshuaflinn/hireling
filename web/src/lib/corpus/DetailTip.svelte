<script>
  // The page-level detail card (Dave's #tip, specs/015 slice 4): spells,
  // feats, class features, actions, items, conditions — rendered from the
  // Foundry corpus (no hardcoded prose). Mounted once by SheetView. Foundry
  // markup is enriched (enrich.js) and adopted through the one inert setter.
  import { adoptHTML, esc } from '../util/inert-html.js';
  import { enrich } from './enrich.js';
  import { lookup, creatureItems } from './client.js';
  import { tip, closeTip, followTip, backTip } from './tip.svelte.js';
  import { ACTION_GLYPH } from './glyphs.js';

  /** @type {HTMLElement | null} */
  let card = $state(null);
  /** @type {HTMLElement | null} */
  let body = $state(null);
  let entry = $state(/** @type {any} */ (null));
  let missing = $state(false);
  /** A creature's embedded strikes/abilities (fetched with the entry). */
  let items = $state(/** @type {any[]} */ ([]));

  $effect(() => {
    const subject = tip.subject;
    entry = null;
    missing = false;
    if (!subject) return;
    const sel = subject.id ? { ids: [subject.id] } : { names: [subject.name ?? ''] };
    let live = true;
    /** Names on a sheet don't say which pack they live in ("Pet" is a
     * feat, "Arcane School" a class feature): try the asked kind first,
     * then its neighbours. */
    const FALLBACK = /** @type {Record<string, string[]>} */ ({
      class_feature: ['class_feature', 'feat'],
      feat: ['feat', 'class_feature'],
      spell: ['spell'],
      item: ['item'],
      action: ['action', 'feat'],
      condition: ['condition'],
      creature: ['creature'],
    });
    const kinds = subject.id ? [subject.kind] : (FALLBACK[subject.kind] ?? [subject.kind]);
    const tryKinds = async () => {
      for (const kind of kinds) {
        const rows = await lookup(kind, sel);
        if (rows.length) return rows;
      }
      return [];
    };
    tryKinds()
      .then((rows) => {
        if (!live) return;
        const want = (subject.name ?? '').toLowerCase();
        entry = rows.find((r) => r.source_id === subject.id || r.name.toLowerCase() === want) ?? rows[0] ?? null;
        missing = !entry;
        items = [];
        if (entry?.kind === 'creature') {
          creatureItems(entry.id)
            .then((list) => {
              if (live) items = list;
            })
            .catch(() => {});
        }
      })
      .catch(() => {
        if (live) missing = true;
      });
    return () => {
      live = false;
    };
  });

  const ord = (/** @type {number} */ n) => (n === 1 ? '1st' : n === 2 ? '2nd' : n === 3 ? '3rd' : `${n}th`);
  const cap = (/** @type {string} */ s) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

  function rankLabel(/** @type {any} */ e) {
    if (e.kind === 'spell') {
      if (e.traits.includes('cantrip')) return 'Cantrip';
      if (e.traits.includes('focus')) return `Focus ${e.level}`;
      return `Spell ${e.level}`;
    }
    if (e.kind === 'feat' || e.kind === 'class_feature') return e.level != null ? `Feat ${e.level}` : 'Feature';
    if (e.kind === 'item') return e.level != null ? `Item ${e.level}` : 'Item';
    if (e.kind === 'action') return e.detail?.category ? cap(e.detail.category) : 'Action';
    if (e.kind === 'creature') return `Creature ${e.level}`;
    return cap(e.kind);
  }

  function actionGlyph(/** @type {any} */ d) {
    if (!d) return '';
    const t = d.action_type;
    if (t === 'reaction') return ACTION_GLYPH.R;
    if (t === 'free') return ACTION_GLYPH.F;
    if (t === 'action' && d.actions) return ACTION_GLYPH[d.actions] ?? '';
    return '';
  }

  function spellGlyph(/** @type {any} */ d) {
    const v = String(d?.time ?? '');
    if (/^[123]$/.test(v)) return ACTION_GLYPH[v];
    if (v === 'reaction') return ACTION_GLYPH.R;
    if (v === 'free') return ACTION_GLYPH.F;
    const m = v.match(/^([123]) (?:to|-) ([123])$/);
    if (m) return `${ACTION_GLYPH[m[1]]} – ${ACTION_GLYPH[m[2]]}`;
    return '';
  }

  function statLines(/** @type {any} */ e) {
    const d = e.detail ?? {};
    /** @type {Array<[string, string]>} */
    const lines = [];
    if (e.kind === 'spell') {
      if (d.traditions?.length) lines.push(['Traditions', d.traditions.join(', ')]);
      const time = String(d.time ?? '');
      if (time && !/^[123]$|^reaction$|^free$|^[123] (to|-) [123]$/.test(time)) lines.push(['Cast', time]);
      if (d.cost) lines.push(['Cost', d.cost]);
      if (d.range) lines.push(['Range', d.range]);
      if (d.area?.value) lines.push(['Area', `${d.area.value}-foot ${d.area.type}`]);
      if (d.target) lines.push(['Targets', d.target]);
      if (d.defense?.save) lines.push(['Defense', `${d.defense.save.basic ? 'basic ' : ''}${cap(d.defense.save.statistic)}`]);
      else if (d.defense?.passive) lines.push(['Defense', String(d.defense.passive.statistic).toUpperCase()]);
      if (d.duration?.value) lines.push(['Duration', `${d.duration.sustained ? 'sustained up to ' : ''}${d.duration.value}`]);
    } else if (e.kind === 'feat' || e.kind === 'class_feature') {
      const pre = (d.prerequisites ?? []).map((/** @type {any} */ p) => p.value).filter(Boolean);
      if (pre.length) lines.push(['Prerequisites', pre.join('; ')]);
      if (d.frequency?.max) lines.push(['Frequency', `${d.frequency.max}/${d.frequency.per}`]);
      if (d.trigger) lines.push(['Trigger', d.trigger]);
      if (d.requirements) lines.push(['Requirements', d.requirements]);
    } else if (e.kind === 'item') {
      if (d.price) {
        const p = d.price;
        const coin = ['pp', 'gp', 'sp', 'cp'].filter((k) => p[k]).map((k) => `${p[k]} ${k}`).join(' ');
        if (coin) lines.push(['Price', coin]);
      }
      if (d.bulk !== undefined && d.bulk !== null) lines.push(['Bulk', d.bulk === 0 ? '—' : d.bulk === 0.1 ? 'L' : String(d.bulk)]);
      if (d.usage) lines.push(['Usage', String(d.usage).replace(/-/g, ' ')]);
      if (d.damage?.dice) lines.push(['Damage', `${d.damage.dice}${d.damage.die} ${d.damage.damageType ?? ''}`]);
      if (d.ac_bonus) lines.push(['AC bonus', `+${d.ac_bonus}${d.dex_cap != null ? ` (Dex cap +${d.dex_cap})` : ''}`]);
      if (d.hardness) lines.push(['Hardness', `${d.hardness}${d.hp?.max ? ` · HP ${d.hp.max}` : ''}`]);
    }
    return lines;
  }

  /** Monster Core stat block: defenses, abilities, strikes, actions. */
  function creatureHtml(/** @type {any} */ e) {
    const d = e.detail ?? {};
    const sg = (/** @type {number} */ n) => (n >= 0 ? `+${n}` : `−${Math.abs(n)}`);
    let h = `<div class="tt-h"><span class="tt-name">${esc(e.name)}</span><span class="tt-rank">Creature ${esc(e.level)}</span></div>`;
    const traits = [...(e.rarity && e.rarity !== 'common' ? [e.rarity] : []), d.size ? (/** @type {Record<string, string>} */ ({ tiny: 'tiny', sm: 'small', med: 'medium', lg: 'large', huge: 'huge', grg: 'gargantuan' }))[d.size] ?? d.size : null, ...e.traits].filter(Boolean);
    if (traits.length) h += `<div class="traits">${traits.map((t) => `<span class="trait">${esc(t)}</span>`).join('')}</div>`;
    if (d.perception != null) h += `<div class="line"><b>Perception</b> ${esc(sg(d.perception))}</div>`;
    const skills = Object.entries(d.skills ?? {}).map(([k, v]) => `${k.charAt(0).toUpperCase()}${k.slice(1)} ${sg(/** @type {any} */ (v).base ?? /** @type {any} */ (v).value ?? 0)}`);
    if (skills.length) h += `<div class="line"><b>Skills</b> ${esc(skills.join(', '))}</div>`;
    const ab = d.abilities ?? {};
    if (Object.keys(ab).length) h += `<div class="line">${['str', 'dex', 'con', 'int', 'wis', 'cha'].map((k) => `<b>${k.charAt(0).toUpperCase()}${k.slice(1)}</b> ${esc(sg(ab[k]?.mod ?? 0))}`).join(', ')}</div>`;
    h += '<hr>';
    const sv = d.saves ?? {};
    h += `<div class="line"><b>AC</b> ${esc(d.ac ?? '—')}; <b>Fort</b> ${esc(sg(sv.fortitude?.value ?? 0))}, <b>Ref</b> ${esc(sg(sv.reflex?.value ?? 0))}, <b>Will</b> ${esc(sg(sv.will?.value ?? 0))}</div>`;
    const iwr = (/** @type {any[]} */ xs, /** @type {boolean} */ val) => (xs ?? []).map((x) => `${x.type}${val && x.value ? ` ${x.value}` : ''}`).join(', ');
    let hpLine = `<b>HP</b> ${esc(d.hp ?? '—')}`;
    if (d.immunities?.length) hpLine += `; <b>Immunities</b> ${esc(iwr(d.immunities, false))}`;
    if (d.weaknesses?.length) hpLine += `; <b>Weaknesses</b> ${esc(iwr(d.weaknesses, true))}`;
    if (d.resistances?.length) hpLine += `; <b>Resistances</b> ${esc(iwr(d.resistances, true))}`;
    h += `<div class="line">${hpLine}</div><hr>`;
    const spd = d.speed ?? {};
    const others = (spd.otherSpeeds ?? []).map((/** @type {any} */ o) => `${o.type} ${o.value} feet`);
    h += `<div class="line"><b>Speed</b> ${esc([`${spd.value ?? 0} feet`, ...others].join(', '))}</div>`;
    for (const it of items.filter((i) => i.type === 'melee')) {
      const dmg = Object.values(it.system.damageRolls ?? {}).map((/** @type {any} */ r) => `${r.damage} ${r.damageType}`).join(' plus ');
      h += `<div class="line"><b>${it.system.traits?.includes('ranged') ? 'Ranged' : 'Melee'}</b> ${glyphFor('1')}${esc(it.name)} ${esc(sg(it.system.bonus ?? 0))}${it.system.traits?.length ? ` (${esc(it.system.traits.join(', '))})` : ''}, <b>Damage</b> ${esc(dmg)}</div>`;
    }
    for (const it of items.filter((i) => i.type === 'action')) {
      const g = it.system.actionType === 'reaction' ? 'R' : it.system.actionType === 'free' ? 'F' : it.system.actionType === 'action' ? String(it.system.actions ?? '') : '';
      h += `<p><b>${esc(it.name)}</b> ${g ? glyphFor(g) : ''} ${enrich(String(it.system.description ?? '').replace(/^<p>|<\/p>$/g, ''))}</p>`;
    }
    if (d.blurb) h += `<p class="none">${esc(d.blurb)}</p>`;
    if (e.publication) h += `<div class="foot"><span>${esc(e.publication)}</span></div>`;
    return h;
  }

  function glyphFor(/** @type {string} */ k) {
    return ACTION_GLYPH[k] ? `<span class="glyph">${ACTION_GLYPH[k]}</span>` : '';
  }

  function html(/** @type {any} */ e) {
    if (e.kind === 'creature') return creatureHtml(e);
    const glyph = e.kind === 'spell' ? spellGlyph(e.detail) : actionGlyph(e.detail);
    let h = `<div class="tt-h"><span class="tt-name">${glyph ? `<span class="glyph">${glyph}</span>` : ''}${esc(e.name)}</span><span class="tt-rank">${esc(rankLabel(e))}</span></div>`;
    const traits = [...(e.rarity && e.rarity !== 'common' ? [e.rarity] : []), ...e.traits];
    if (traits.length) h += `<div class="traits">${traits.map((t) => `<span class="trait ${t === 'uncommon' || t === 'rare' || t === 'unique' ? t : ''}">${esc(t)}</span>`).join('')}</div>`;
    const lines = statLines(e);
    if (lines.length) h += lines.map(([k, v]) => `<div class="line"><b>${esc(k)}</b> ${esc(v)}</div>`).join('');
    if (lines.length) h += '<hr>';
    h += enrich(e.description || '<p class="none">No description in the pack.</p>');
    const heightening = e.detail?.heightening;
    if (heightening?.type === 'interval' && heightening.damage) {
      const parts = Object.entries(heightening.damage).map(([, v]) => v).join(', ');
      h += `<p><b>Heightened (+${esc(heightening.interval)})</b> damage +${esc(parts)}</p>`;
    }
    if (e.publication) h += `<div class="foot"><span>${esc(e.publication)}</span>${e.lane === 'custom' ? '<span>custom</span>' : ''}</div>`;
    return h;
  }

  $effect(() => {
    if (!body) return;
    void items;
    if (entry) adoptHTML(body, html(entry));
    else if (missing) adoptHTML(body, `<div class="tt-h"><span class="tt-name">${esc(tip.subject?.name ?? '')}</span></div><p class="none">Not in the imported pack.</p>`);
    else adoptHTML(body, '<p class="none">Loading…</p>');
  });

  function onClick(/** @type {MouseEvent} */ ev) {
    const a = /** @type {HTMLElement} */ (ev.target).closest?.('a.xref');
    if (!a) return;
    ev.preventDefault();
    const el = /** @type {HTMLElement} */ (a);
    if (el.dataset.uuidKind) followTip({ kind: el.dataset.uuidKind, id: el.dataset.uuidId, name: el.textContent ?? '' });
    else if (el.dataset.act) followTip({ kind: 'action', name: (el.textContent ?? '').trim() });
  }

  $effect(() => {
    if (!tip.open) return;
    const key = (/** @type {KeyboardEvent} */ e) => {
      if (e.key === 'Escape') closeTip();
    };
    const out = (/** @type {MouseEvent} */ e) => {
      if (!tip.pinned) return;
      const t = /** @type {HTMLElement} */ (e.target);
      if (card && !card.contains(t) && !t.closest('.has-tip')) closeTip();
    };
    document.addEventListener('keydown', key);
    document.addEventListener('click', out, true);
    return () => {
      document.removeEventListener('keydown', key);
      document.removeEventListener('click', out, true);
    };
  });
</script>

{#if tip.open && tip.subject}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- Reason: delegated clicks on adopted cross-reference anchors; Escape is
  handled at document level above. -->
  <div
    class="detail-tip"
    class:pinned={tip.pinned}
    role="dialog"
    tabindex="-1"
    aria-label={tip.subject.name ?? 'Details'}
    style:left="{tip.at.x}px"
    style:top="{tip.at.y}px"
    bind:this={card}
    onclick={onClick}
  >
    {#if tip.pinned}
      <div class="tip-ctl">
        {#if tip.trail.length}<button class="tip-back" onclick={backTip} aria-label="Back">←</button>{/if}
        <button class="tip-x" onclick={closeTip} aria-label="Close">×</button>
      </div>
    {/if}
    <div bind:this={body}></div>
  </div>
{/if}

<style>
  .detail-tip {
    position: fixed;
    z-index: 70;
    width: 440px;
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 16px);
    overflow: auto;
    background: #0d0f13;
    border: 1px solid var(--gold-dim);
    border-radius: 10px;
    padding: 10px 12px;
    box-shadow: 0 12px 36px rgb(0 0 0 / 60%);
    font-size: 13px;
    line-height: 1.45;
    text-align: left;
    color: var(--text);
  }
  .detail-tip.pinned {
    border-color: var(--gold);
  }
  .tip-ctl {
    float: right;
    display: flex;
    gap: 4px;
    margin: -4px -4px 0 8px;
  }
  .tip-ctl button {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 6px;
    width: 24px;
    height: 24px;
    cursor: pointer;
  }
  .detail-tip :global(.tt-h) {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 10px;
    border-bottom: 1px solid var(--line2);
    padding-bottom: 4px;
    margin-bottom: 6px;
  }
  .detail-tip :global(.tt-name) {
    font-family: var(--serif);
    font-size: 17px;
    color: var(--gold);
  }
  .detail-tip :global(.glyph) {
    margin-right: 7px;
    color: var(--gold);
    display: inline-flex;
    align-items: center;
    vertical-align: -1px;
  }
  .detail-tip :global(.glyph svg) {
    height: 13px;
    width: auto;
  }
  .detail-tip :global(.tt-rank) {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--muted);
    white-space: nowrap;
  }
  .detail-tip :global(.traits) {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    margin-bottom: 6px;
  }
  .detail-tip :global(.trait) {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border: 1px solid #6b4f1d;
    background: #3a2a10;
    color: #e9c47c;
    border-radius: 3px;
    padding: 0 5px;
  }
  .detail-tip :global(.trait.uncommon) {
    background: #4a2210;
    border-color: #a0522d;
  }
  .detail-tip :global(.trait.rare) {
    background: #10204a;
    border-color: #2d52a0;
  }
  .detail-tip :global(.line) {
    margin: 1px 0;
  }
  .detail-tip :global(.line b) {
    color: var(--text);
  }
  .detail-tip :global(hr) {
    border: 0;
    border-top: 1px solid var(--line2);
    margin: 6px 0;
  }
  .detail-tip :global(p) {
    margin: 6px 0;
  }
  .detail-tip :global(p.none) {
    color: var(--muted);
    font-style: italic;
  }
  .detail-tip :global(a.xref) {
    color: var(--gold);
    text-decoration: underline dotted;
    cursor: pointer;
  }
  .detail-tip :global(b.roll),
  .detail-tip :global(b.check),
  .detail-tip :global(b.area) {
    color: #e9c47c;
    font-weight: 600;
  }
  .detail-tip :global(table) {
    border-collapse: collapse;
    margin: 6px 0;
  }
  .detail-tip :global(td),
  .detail-tip :global(th) {
    border: 1px solid var(--line2);
    padding: 2px 6px;
  }
  .detail-tip :global(.foot) {
    display: flex;
    justify-content: space-between;
    margin-top: 8px;
    font-size: 11px;
    color: var(--muted);
  }
</style>
