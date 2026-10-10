<script>
  // ConditionTip (E9 FR-1, design D6): the one tooltip primitive every
  // condition-name surface mounts. Hover/focus opens, click/Enter pins,
  // Escape and click-outside close; a linkified condition name inside the
  // popup swaps the content IN PLACE — second layer, never a third (the
  // prototype's layer behavior, ported). Matching is case-insensitive exact
  // name against the curated seed; corpus context (lane/tier) only adds
  // badges, it is never the gate. All prose rides the one inert setter
  // (util/inert-html.js adoptHTML) — this and AboutView are the only
  // sanctioned {@html} sites in the tree (web-html-boundary gate).
  //
  // NOT a modal (design: "Dialog-adjacent"): no focus trap — Tab may leave,
  // like any popover. The keyboard discipline still follows
  // util/keyboard.js's pattern where it applies: Enter commits (pins /
  // activates a nested link), Escape cancels, and closing restores focus to
  // the trigger when focus had moved into the popup.
  import { adoptHTML, esc, linkifyConditions } from '../../util/inert-html.js';
  import { lookup } from '../../rules/prose.js';

  /** @type {{ name: string, lane?: string | null, tier?: string | null,
    customDescription?: string | null, customValue?: number | string | null,
    children?: import('svelte').Snippet }} */
  let {
    name,
    lane = null,
    tier = null,
    customDescription = null,
    customValue = null,
    children,
  } = $props();

  let open = $state(false);
  let pinned = $state(false);
  /** The popup's current subject key — '' renders the trigger's own name;
   * a nested-link click swaps it (second layer). */
  let subject = $state('');
  /** @type {HTMLElement | null} */
  let host = $state(null);
  /** @type {HTMLElement | null} */
  let popHost = $state(null);

  const keyOf = (/** @type {string} */ value) =>
    value.toLowerCase().replace(/\s+/g, ' ').trim();
  const cap = (/** @type {string} */ value) =>
    value ? value.charAt(0).toUpperCase() + value.slice(1) : value;

  /** The condition key this popup renders right now. */
  const currentKey = $derived(subject || keyOf(name));
  const ownLayer = $derived(subject === '');
  const isCustom = $derived(
    ownLayer && customDescription !== null && customDescription !== undefined,
  );

  /** The popup content, assembled esc'ed and parsed inertly on adoption. */
  function contentHtml() {
    let badges = '';
    if (ownLayer) {
      if (isCustom || lane === 'custom') badges += '<span class="badge">custom</span>';
      if (tier) badges += `<span class="badge">${esc(tier)}</span>`;
    }
    let body;
    let foot = '';
    if (isCustom) {
      // Party input renders as TEXT — escaped, never linkified, never markup
      // (contracts/inert-html.md §5). The optional value is a display note.
      body = `<p>${esc(customDescription)}</p>`;
      if (customValue !== null && customValue !== undefined) {
        body += `<p class="value">Value: ${esc(customValue)}</p>`;
      }
      foot = '<div class="foot"><span>Custom entry</span></div>';
    } else {
      const entry = lookup(currentKey);
      if (entry.found) {
        // Curated prose may carry <b>/<i> and is linkified; the popup's own
        // subject is never self-linked.
        body = `<p>${linkifyConditions(esc(entry.text), { skip: currentKey })}</p>`;
        foot =
          `<div class="foot"><span>Player Core p. ${esc(entry.page)}</span>` +
          `<a href="https://2e.aonprd.com/Conditions.aspx?ID=${esc(entry.aonId)}" target="_blank" rel="noopener">AoN ↗</a></div>`;
      } else {
        // The honest fallback: name + badges + "no paraphrase yet" — never
        // invented rules text (design D9.2).
        body = '<p class="none">No paraphrase yet.</p>';
      }
    }
    return `<div class="tt-h"><span class="tt-name">${esc(ownLayer ? name : cap(currentKey))}</span>${badges}</div>${body}${foot}`;
  }

  // Render on open and on every subject swap: same popup element, content
  // re-adopted in place — the second layer replaces, it never stacks.
  $effect(() => {
    if (!open || !popHost) return;
    adoptHTML(popHost, contentHtml());
    // Second-layer anchors carry no href (the tip owns activation); make
    // them reachable and name their role for the keyboard path.
    for (const anchor of popHost.querySelectorAll('a[data-cond]')) {
      anchor.setAttribute('tabindex', '0');
      anchor.setAttribute('role', 'button');
    }
  });

  function openTip() {
    if (pinned) return;
    subject = '';
    open = true;
  }

  /**
   * Close and unpin. `restore` puts focus back on the trigger when focus
   * had moved inside the popup (the keyboard discipline's restore-on-close).
   * @param {{restore?: boolean}} [options]
   */
  function closeTip({ restore = false } = {}) {
    const focusInside =
      restore && host && popHost && host.contains(document.activeElement) &&
      document.activeElement !== host;
    pinned = false;
    open = false;
    subject = '';
    if (focusInside) host?.focus();
  }

  /** @param {MouseEvent} event */
  function togglePin(event) {
    // Link clicks (AoN, second layer) and any click inside the popup are
    // not pin toggles — the delegation handlers own them.
    if (/** @type {HTMLElement} */ (event.target).closest?.('.pop')) return;
    if (pinned) {
      closeTip();
    } else {
      subject = '';
      open = true;
      pinned = true;
    }
  }

  /** @param {KeyboardEvent} event */
  function onKeydown(event) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      closeTip({ restore: true });
      return;
    }
    if (event.key === 'Enter') {
      // Second-layer activation: Enter on a nested link swaps the popup.
      const nested = /** @type {HTMLElement | null} */ (
        /** @type {HTMLElement} */ (event.target).closest?.('a[data-cond]')
      );
      if (nested) {
        event.preventDefault();
        subject = /** @type {string} */ (nested.dataset.cond);
        return;
      }
      if (/** @type {HTMLElement} */ (event.target).closest?.('.pop')) return;
      // The trigger itself: Enter pins (the click path's twin).
      if (!pinned) {
        subject = '';
        open = true;
        pinned = true;
      }
    }
  }

  /** @param {MouseEvent} event */
  function onPopupClick(event) {
    const nested = /** @type {HTMLElement | null} */ (
      /** @type {HTMLElement} */ (event.target).closest?.('a[data-cond]')
    );
    if (nested) subject = /** @type {string} */ (nested.dataset.cond);
  }

  // While open: a click anywhere outside the wrapper closes (pinned or not —
  // the pointer has demonstrably gone elsewhere).
  $effect(() => {
    if (!open) return;
    const onDocClick = (/** @type {MouseEvent} */ event) => {
      if (host && !host.contains(/** @type {Node} */ (event.target))) closeTip();
    };
    const onDocKey = (/** @type {KeyboardEvent} */ event) => {
      if (event.key === 'Escape') closeTip({ restore: true });
    };
    document.addEventListener('click', onDocClick, true);
    document.addEventListener('keydown', onDocKey, true);
    return () => {
      document.removeEventListener('click', onDocClick, true);
      document.removeEventListener('keydown', onDocKey, true);
    };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<!-- Reason (design D6): the tip is a focusable, keyboard-operable popover —
tabindex + role carry the a11y affordance; Escape/Enter are handled above. -->
<span
  class="tip-wrap"
  bind:this={host}
  role="button"
  tabindex="0"
  aria-expanded={open}
  onmouseenter={openTip}
  onmouseleave={() => {
    if (!pinned) closeTip();
  }}
  onfocus={openTip}
  onblur={() => {
    if (!pinned) closeTip();
  }}
  onclick={togglePin}
  onkeydown={onKeydown}
>
  {#if children}{@render children()}{:else}{name}{/if}
  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- Reason: key events are handled on the wrapper (Enter/Escape, above);
    the popup's click is delegation-only for the second-layer links. -->
    <span class="pop" role="tooltip" bind:this={popHost} onclick={onPopupClick}></span>
  {/if}
</span>

<style>
  .tip-wrap {
    position: relative;
    display: inline-flex;
    outline: none;
  }
  .tip-wrap:focus-visible {
    box-shadow: 0 0 0 1px var(--gold-dim, #b8963e);
    border-radius: 999px;
  }
  .pop {
    position: absolute;
    z-index: 60;
    left: 0;
    top: calc(100% + 6px);
    min-width: 220px;
    max-width: 320px;
    background: var(--panel2, #161a21);
    border: 1px solid var(--gold-dim, #b8963e);
    border-radius: 8px;
    padding: 8px 10px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text, #e8e4d8);
    box-shadow: 0 6px 18px rgb(0 0 0 / 45%);
    text-align: left;
    white-space: normal;
    cursor: default;
  }
  .tt-h {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-bottom: 3px;
  }
  .tt-name {
    font-weight: 700;
    color: var(--gold, #d4af5f);
  }
  .badge {
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--dim, #5c6672);
    border: 1px solid var(--muted, #8a94a3);
    border-radius: 4px;
    padding: 0 4px;
  }
  .pop :global(p) {
    margin: 3px 0;
  }
  .pop :global(p.value) {
    color: var(--muted, #8a94a3);
    font-size: 11.5px;
  }
  .pop :global(p.none) {
    color: var(--muted, #8a94a3);
    font-style: italic;
  }
  .pop :global(a[data-cond]) {
    color: var(--gold, #d4af5f);
    text-decoration: underline;
    cursor: pointer;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-top: 5px;
    font-size: 11px;
    color: var(--muted, #8a94a3);
  }
  .pop :global(.foot a) {
    color: var(--gold, #d4af5f);
  }
</style>
