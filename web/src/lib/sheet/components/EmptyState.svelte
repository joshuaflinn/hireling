<script>
  // The empty state is a real screen (spec §7): no character yet — the
  // house crest, one deliberate CTA into E5's import flow.
  let { onimport } = $props();
</script>

<div class="state-block">
  <div class="crest" aria-hidden="true">⚒</div>
  <h2>No character yet</h2>
  <p>
    Import a Pathbuilder 2e export and the whole table sees your sheet —
    every number in its place, every tap synced to the party.
  </p>
  <button class="btn gold" onclick={onimport}>Import your character</button>
</div>
