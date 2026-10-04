<script>
  // Calm error with a retry affordance where a retry exists (design §7).
  // The sync reconnects itself; this surface is for bootstrap/import reads.
  let { message = 'Something went wrong.', detail = '', onretry } = $props();
</script>

<div class="state-block">
  <h2>That didn't work</h2>
  <p>{message}</p>
  {#if detail}
    <p class="err">{detail}</p>
  {/if}
  {#if onretry}
    <button class="btn" onclick={onretry}>Try again</button>
  {/if}
</div>
