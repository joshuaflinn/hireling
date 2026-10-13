<script>
  // The one dialog (design §6): native <dialog> + the shared keyboard
  // discipline (util/keyboard.js) — focus trapped, Escape cancels, Enter
  // commits, focus restores to the opener on close. Every dialog in the
  // sheet (prep picker, level-down confirm, New Day confirm) rides this;
  // `window.confirm` appears nowhere.
  import { trapKeys } from '../../util/keyboard.js';

  /** @type {{ open?: boolean, title?: string, onclose?: () => void,
    oncommit?: () => void,
    children?: import('svelte').Snippet, footer?: import('svelte').Snippet }} */
  let {
    open = false,
    title = '',
    onclose,
    oncommit,
    children,
    footer,
  } = $props();

  /** @type {HTMLDialogElement | null} */
  let dialogHost = $state(null);
  /** @type {HTMLElement | null} */
  let opener = $state(null);

  $effect(() => {
    if (open && dialogHost) {
      opener = /** @type {HTMLElement | null} */ (document.activeElement);
      dialogHost.showModal();
      const dispose = trapKeys({
        container: dialogHost,
        host: opener ?? dialogHost,
        onCancel: () => onclose?.(),
        // Enter commits: the prop exists so the claim in
        // util/keyboard.js and spec §7 is wired, not just implemented.
        onCommit: oncommit,
      });
      return () => {
        dispose();
        if (dialogHost?.open) dialogHost.close();
      };
    }
    return undefined;
  });
</script>

{#if open}
  <dialog bind:this={dialogHost}>
    <h3>{title}</h3>
    {@render children?.()}
    <div class="row2">
      <span></span>
      <span style="display:flex;gap:8px">{@render footer?.()}</span>
    </div>
  </dialog>
{/if}
