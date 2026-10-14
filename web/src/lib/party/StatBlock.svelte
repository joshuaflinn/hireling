<script>
  // The party-card stat block (E14 design D2): E6 tiles and the key-skills
  // line under the card's HP bar — one glanceable read (spec FR-1), the
  // GM's whole encounter-calibration picture without drilling in.
  //
  // Every number here is a field of the engine output read as-is (spec
  // FR-3): the block computes nothing. `primaryCaster` and `keySkills` are
  // selection (compare, sort) — the same helpers the sheet's pane calls.
  // Loading (view null) renders skeleton rows — the honest gap, mirroring
  // the card's bar skeleton (spec FR-6).
  //
  // No control of its own anywhere in it (SC-5): the only focusable
  // elements are the tiles' read-only provenance hovers — the same
  // affordance the sheet's tiles carry.
  import Provenance from '../sheet/components/Provenance.svelte';
  import StatTile from '../sheet/components/StatTile.svelte';
  import { primaryCaster } from '../engine/caster.js';
  import { keySkills } from '../engine/key-skills.js';
  import { signed } from '../engine/format.js';

  /** @type {{ view: any }} the engine output for this card, or null until
   * the wire delivers it */
  let { view } = $props();

  const numbers = $derived(view?.derived ?? null);
  const spellDc = $derived(numbers ? primaryCaster(numbers.casters) : null);
  const skills = $derived(numbers ? keySkills(numbers.skills) : []);
</script>

<div class="stats" role="group" aria-label="Stats">
  {#if numbers}
    <div class="tiles">
      <StatTile label="AC" value={numbers.ac.total} provenance={numbers.ac} />
      <StatTile label="Fort" value={signed(numbers.fort.total)} provenance={numbers.fort} />
      <StatTile label="Reflex" value={signed(numbers.ref.total)} provenance={numbers.ref} />
      <StatTile label="Will" value={signed(numbers.will.total)} provenance={numbers.will} />
      <StatTile label="Perception" value={signed(numbers.perception.total)} provenance={numbers.perception} />
      <!-- Initiative modifier (spec FR-4): the value IS perception.total —
        PF2e initiative is "typically ... a Perception check to determine
        your initiative" (Step 1: Roll Initiative, Player Core pg. 435;
        read off the source page, AoN Rules ID 2423). The engine's closed
        vocabulary has no initiative stat (engine/src/vocab.rs), so
        Perception is the only single-path source; the provenance hover is
        Perception's own, that being the math behind the number. -->
      <StatTile label="Init" value={signed(numbers.perception.total)} provenance={numbers.perception} />
      {#if spellDc}
        <StatTile
          label="Spell DC"
          value={spellDc.spell_dc.total}
          cls="gold"
          note={`attack ${signed(spellDc.spell_attack.total)}`}
          provenance={spellDc.spell_dc}
        />
      {/if}
      <!-- Contract §1: `class_dc` is always an object; a null `base` means
        the character has no class DC — no entry, never a dash or a zero. -->
      {#if numbers.class_dc.base !== null && numbers.class_dc.base !== undefined}
        <StatTile label="Class DC" value={numbers.class_dc.total} provenance={numbers.class_dc} />
      {/if}
    </div>
    <dl class="key-skills">
      {#each skills as skill (skill.name)}
        <div class="row">
          <dt>{skill.label}</dt>
          <dd>{signed(skill.total)}<Provenance applied={skill.applied} suppressed={skill.suppressed} /></dd>
        </div>
      {/each}
    </dl>
  {:else}
    <!-- Engine output not on the wire yet: skeleton entries, no invented
         numbers (SC-7) — the same honest gap as the card's bar skeleton. -->
    <div class="skeleton stat-loading" role="status" aria-label="Stats loading"></div>
    <div class="skeleton skills-loading" role="status" aria-label="Key skills loading"></div>
  {/if}
</div>

<style>
  .stats {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  /* The card's density: four tiles per row (the sheet's pane uses three)
     so eight tiles wrap into two compact rows. */
  .tiles {
    grid-template-columns: repeat(4, 1fr);
  }

  .tiles :global(.tile .v) {
    font-size: 16px;
  }

  .tiles :global(.tile .x) {
    font-size: 7.5px;
  }

  /* The key-skills line: one wrapping row of name/modifier pairs. */
  .key-skills {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
    margin: 0;
  }

  .row {
    display: flex;
    align-items: baseline;
    gap: 4px;
    min-width: 0;
  }

  .key-skills dt {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--muted, #8a94a3);
    font-weight: 600;
    white-space: nowrap;
  }

  .key-skills dd {
    display: inline-flex;
    align-items: baseline;
    margin: 0;
    font-size: 12px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .stat-loading {
    height: 2.2rem;
    width: 100%;
  }

  .skills-loading {
    height: 0.8rem;
    width: 70%;
  }
</style>
