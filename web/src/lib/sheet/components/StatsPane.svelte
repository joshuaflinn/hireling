<script>
  // The stats pane (spec §2.2): HP bar, stat tiles, hero/focus pips,
  // attributes, skills + lores, meta lines. All numbers come from the
  // adapter output (`numbers`) — the pane computes nothing.
  import HpBar from './HpBar.svelte';
  import PipRow from './PipRow.svelte';
  import SkillRow from './SkillRow.svelte';
  import StatTile from './StatTile.svelte';
  import { rankLetter, rankName, signed } from '../../engine/format.js';

  /** @type {{ view: any, baseSheet: any, hp: any, temp: any, money?: any,
    focusCurrent?: any, focusMax?: number, heroPoints?: any, heroMax?: number,
    editable?: boolean, offline?: boolean, ondamage?: (amount: number) => void,
    onheal?: (amount: number) => void, onfull?: () => void,
    ontemp?: (value: number) => void, onfocus?: (value: number) => void,
    onhero?: (value: number) => void }} */
  let {
    view, // the adapter output (engine/index.js derive())
    baseSheet, // identity/meta/base data
    hp,
    temp,
    money,
    focusCurrent,
    focusMax,
    heroPoints,
    heroMax,
    editable = true,
    offline = false,
    ondamage,
    onheal,
    onfull,
    ontemp,
    onfocus,
    onhero,
  } = $props();

  const identity = $derived(baseSheet.identity);
  const numbers = $derived(view.derived);
  const attrs = $derived(['str', 'dex', 'con', 'int', 'wis', 'cha']);
  const langList = $derived((identity.languages ?? []).filter((/** @type {string} */ lang) => lang && lang !== 'None selected'));
  const spellDc = $derived(
    numbers.casters.find((/** @type {any} */ caster) => !caster.innate) ?? numbers.casters[0] ?? null,
  );
  /** Display name for a skill key: the prototype capitalizes ("Thievery"). */
  const displayName = (/** @type {string} */ key) => key.charAt(0).toUpperCase() + key.slice(1);
</script>

<section class="panel" aria-label="Basic info">
  <h2>Basic Info <small>{identity.ancestry ?? ''} {identity.class ?? ''}</small></h2>
  <HpBar {hp} {temp} max={view.hp_max.total} {editable} {offline} {ondamage} {onheal} {onfull} {ontemp} />

  <div class="tiles">
    <StatTile label="AC" value={numbers.ac.total} cls="t-ac gold" note="Armor Class" />
    <StatTile label="Fort" value={signed(numbers.fort.total)} cls="t-fort" note={rankName(baseSheet.proficiencies.fortitude ?? 0)} />
    <StatTile label="Perception" value={signed(numbers.perception.total)} cls="t-perception" />
    <StatTile label="Class DC" value={numbers.class_dc.total ?? '—'} cls="t-classdc" note={(identity.keyability ?? '').toUpperCase()} />
    <StatTile label="Reflex" value={signed(numbers.ref.total)} cls="t-reflex" />
    <StatTile label="Speed" value={numbers.speed.total} cls="t-speed" note="feet" />
    {#if spellDc}
      <StatTile label="Spell DC" value={spellDc.spell_dc.total} cls="t-spelldc gold" note={`attack ${signed(spellDc.spell_attack.total)}`} />
    {/if}
    <StatTile label="Will" value={signed(numbers.will.total)} cls="t-will" />
    <StatTile label="Size" value={identity.size_name ?? '—'} cls="t-size" />
  </div>

  <div class="trackers">
    <PipRow label="Hero Points" current={heroPoints.value} max={heroMax} {editable} disabled={offline} onset={onhero} />
    {#if (focusMax ?? 0) > 0}
      <PipRow label="Focus" current={focusCurrent?.value ?? 0} max={focusMax ?? 0} {editable} disabled={offline} onset={onfocus} />
    {/if}
  </div>

  <h3>Attributes</h3>
  <div class="attrs">
    {#each /** @type {Array<'str' | 'dex' | 'con' | 'int' | 'wis' | 'cha'>} */ (attrs) as key (key)}
      <div class="attr" class:key={key === identity.keyability}>
        <div class="k">{key.toUpperCase()}</div>
        <div class="v">{signed(view.attributes[key])}</div>
      </div>
    {/each}
  </div>

  <h3>Skills</h3>
  <div class="skills">
    {#each numbers.skills as skill (skill.name)}
      <SkillRow
        name={displayName(skill.name)}
        rank={skill.rank}
        rankLetter={rankLetter(skill.rank)}
        modifier={signed(skill.total)}
        untrained={skill.rank === 0}
      />
    {/each}
    {#each numbers.lores as lore (lore.name)}
      <SkillRow
        name={lore.label}
        rank={lore.rank}
        rankLetter={rankLetter(lore.rank)}
        modifier={signed(lore.total)}
        untrained={lore.rank === 0}
        wide
      />
    {/each}
  </div>

  <div class="meta">
    <b>Ancestry</b> {identity.ancestry ?? '—'} ({identity.heritage ?? '—'}) ·
    <b>Background</b> {identity.background ?? '—'} ·
    <b>Alignment</b> {identity.alignment ?? '—'}
  </div>
  <div class="meta">
    <b>Languages</b>
    {#if langList.length}{langList.join(', ')}{:else}<span style="color:var(--dim)">not set in export</span>{/if}
    · <b>Deity</b> {identity.deity ?? '—'}
  </div>
</section>
