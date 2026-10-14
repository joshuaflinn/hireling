<script>
  // The stats pane (spec §2.2): HP bar, stat tiles, hero/focus pips,
  // attributes, skills + lores, meta lines. All numbers come from the
  // engine output (`view`, the wire's EngineOutput verbatim) — the pane
  // computes nothing. Post-swap, `derived.skills` carries the lores and
  // the partition is the one-line fold it always promised to be.
  import HpBar from './HpBar.svelte';
  import PipRow from './PipRow.svelte';
  import SkillRow from './SkillRow.svelte';
  import StatTile from './StatTile.svelte';
  import { findOpError } from '../state.js';
  import { primaryCaster } from '../../engine/caster.js';
  import { rankLetter, rankName, signed } from '../../engine/format.js';
  import { partitionSkills } from '../../engine/partition.js';

  /** @type {{ view: any, baseSheet: any, hp: any, temp: any, money?: any,
    focusCurrent?: any, focusMax?: number, heroPoints?: any, heroMax?: number,
    shieldRaised?: {value: boolean, pending: boolean},
    opErrors?: any[], editable?: boolean, offline?: boolean,
    ondamage?: (amount: number) => void,
    onheal?: (amount: number) => void, onfull?: () => void,
    ontemp?: (value: number) => void, onfocus?: (value: number) => void,
    onhero?: (value: number) => void, onshield?: (raised: boolean) => void }} */
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
    shieldRaised,
    opErrors = [],
    editable = true,
    offline = false,
    ondamage,
    onheal,
    onfull,
    ontemp,
    onfocus,
    onhero,
    onshield,
  } = $props();

  const identity = $derived(baseSheet.identity);
  const numbers = $derived(view.derived);
  const attrs = $derived(['str', 'dex', 'con', 'int', 'wis', 'cha']);
  const langList = $derived((identity.languages ?? []).filter((/** @type {string} */ lang) => lang && lang !== 'None selected'));
  // The selection rule lives in the engine lib (E14 design D5) — the same
  // helper the party-card stat block calls, so the two surfaces can't drift.
  const spellDc = $derived(primaryCaster(numbers.casters));
  // Inline rejections (spec §6): the matching opError renders at its
  // control, calm and auto-clearing (the state layer clears on the next
  // applied ack).
  /** @param {string} field */
  const vitalError = (field) => findOpError(opErrors, 'vitals', { field });
  const hpError = $derived(vitalError('hp') ?? vitalError('temp_hp'));
  const heroError = $derived(vitalError('hero_points'));
  const focusError = $derived(vitalError('focus_current'));
  const shieldError = $derived(vitalError('shield_raised'));
  // The Raise-a-Shield affordance (gh#82): only when the character's
  // export carries a shield (a positive `ac.shieldBonus`); default
  // lowered. The number shown is the bonus the engine adds while raised —
  // the AC itself comes from the wire's derived output, so the tile below
  // moves the moment the recompute lands.
  const shieldBonus = $derived(baseSheet.ac?.shieldBonus ?? 0);
  const shieldRaisedOn = $derived(shieldRaised?.value === true);
  // the house idiom (degraded-mode.md:32): edit affordances disable offline.
  // pending alone would let the first offline tap enqueue a write and then
  // lock the button until reconnect — offline must gate the control itself.
  const shieldBusy = $derived(offline || shieldRaised?.pending === true);
  /** Display name for a skill key: the prototype capitalizes ("Thievery"). */
  const displayName = (/** @type {string} */ key) => key.charAt(0).toUpperCase() + key.slice(1);
  /** Core skills and lores through one partition: the fold and the
   * label are E6's. The wire's lore rows carry their display name as
   * `label` (contract §3); the partition prefers it and falls back to
   * deriving from the key. */
  const partitioned = $derived(partitionSkills(numbers.skills));
</script>

<section class="panel" aria-label="Basic info">
  <h2>Basic Info <small>{identity.ancestry ?? ''} {identity.class ?? ''}</small></h2>
  <HpBar {hp} {temp} max={view.render_base.hp_max} {editable} {offline} {ondamage} {onheal} {onfull} {ontemp} />
  {#if hpError}
    <p class="op-error" role="alert">{hpError.reason}</p>
  {/if}

  <div class="tiles">
    <StatTile label="AC" value={numbers.ac.total} cls="t-ac gold" note="Armor Class" provenance={numbers.ac} />
    {#if shieldBonus > 0}
      <div class="tile t-shield">
        <div class="v">{shieldRaisedOn ? signed(shieldBonus) : '—'}</div>
        <div class="k">Shield</div>
        {#if editable}
          <button
            type="button"
            class="shield-toggle"
            aria-pressed={shieldRaisedOn}
            disabled={shieldBusy}
            onclick={() => onshield?.(!shieldRaisedOn)}
          >
            {shieldRaisedOn ? 'Lower shield' : 'Raise shield'}</button>
        {:else}
          <span class="shield-state">{shieldRaisedOn ? 'raised' : 'lowered'}</span>
        {/if}
        {#if shieldError}<p class="op-error" role="alert">{shieldError.reason}</p>{/if}
      </div>
    {/if}
    <StatTile label="Fort" value={signed(numbers.fort.total)} cls="t-fort" note={rankName(baseSheet.proficiencies.fortitude ?? 0)} provenance={numbers.fort} />
    <StatTile label="Perception" value={signed(numbers.perception.total)} cls="t-perception" provenance={numbers.perception} />
    <StatTile label="Class DC" value={numbers.class_dc.total ?? '—'} cls="t-classdc" note={(identity.keyability ?? '').toUpperCase()} provenance={numbers.class_dc} />
    <StatTile label="Reflex" value={signed(numbers.ref.total)} cls="t-reflex" provenance={numbers.ref} />
    <StatTile label="Speed" value={numbers.speed.total} cls="t-speed" note="feet" provenance={numbers.speed} />
    {#if spellDc}
      <StatTile label="Spell DC" value={spellDc.spell_dc.total} cls="t-spelldc gold" note={`attack ${signed(spellDc.spell_attack.total)}`} provenance={spellDc.spell_dc} />
    {/if}
    <StatTile label="Will" value={signed(numbers.will.total)} cls="t-will" provenance={numbers.will} />
    <StatTile label="Size" value={identity.size_name ?? '—'} cls="t-size" />
  </div>

  <div class="trackers">
    <div class="trk-wrap">
      <PipRow label="Hero Points" current={heroPoints.value} max={heroMax} {editable} disabled={offline} onset={onhero} />
      {#if heroError}<p class="op-error" role="alert">{heroError.reason}</p>{/if}
    </div>
    {#if (focusMax ?? 0) > 0}
      <div class="trk-wrap">
        <PipRow label="Focus" current={focusCurrent?.value ?? 0} max={focusMax ?? 0} {editable} disabled={offline} onset={onfocus} />
        {#if focusError}<p class="op-error" role="alert">{focusError.reason}</p>{/if}
      </div>
    {/if}
  </div>

  <h3>Attributes</h3>
  <div class="attrs">
    {#each /** @type {Array<'str' | 'dex' | 'con' | 'int' | 'wis' | 'cha'>} */ (attrs) as key (key)}
      <div class="attr" class:key={key === identity.keyability}>
        <div class="k">{key.toUpperCase()}</div>
        <div class="v">{signed(view.render_base.attributes[key])}</div>
      </div>
    {/each}
  </div>

  <h3>Skills</h3>
  <div class="skills">
    {#each partitioned.core as skill (skill.name)}
      <SkillRow
        name={displayName(skill.name)}
        rank={skill.rank}
        rankLetter={rankLetter(skill.rank)}
        modifier={signed(skill.total)}
        untrained={skill.rank === 0}
        provenance={skill}
      />
    {/each}
    {#each partitioned.lores as lore (lore.name)}
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
