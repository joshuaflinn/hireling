<script>
  /* global window, document, ResizeObserver */
  // Dave's movable, resizable widget board (specs/015 slice 9). Six
  // widgets, absolutely positioned; drag a panel by its title bar, resize
  // from any edge or corner; Reset Layout restores the default. The user's
  // arrangement persists per character in sheet_ui.layout (synced). Below
  // 780px the board falls back to one stacked column (Dave's narrow mode).
  import {
    WIDGET_IDS, GAP, MINW, MINH, NARROW_BP,
    defaultLayout, validLayout, relayout, snapX, fitRight, orderForDrag, layoutPass,
  } from '../board-layout.js';

  /** @type {{ saved: any, onsave?: (layout: any) => void, resetSignal?: number,
   *   children: import('svelte').Snippet<[string]> }} */
  let { saved = null, onsave, resetSignal = 0, children } = $props();

  /** @type {HTMLElement | null} */
  let boardEl = $state(null);
  /** @type {Record<string, HTMLElement>} */
  const els = {};
  let width = $state(0);
  let narrow = $state(false);
  /** @type {Record<string, {x: number, y: number, w: number, h: number | null}>} */
  let items = $state({});
  let height = $state(0);
  let dragId = $state(/** @type {string | null} */ (null));
  let ph = $state(/** @type {{x: number, y: number, w: number, h: number} | null} */ (null));

  const heightOf = (/** @type {string} */ id) =>
    items[id]?.h ?? els[id]?.firstElementChild?.getBoundingClientRect().height ?? MINH;

  function load() {
    const base = validLayout(saved) ? structuredClone($state.snapshot(saved)) : defaultLayout(width);
    items = base;
    pack();
  }

  function pack() {
    if (narrow || !width) return;
    const next = structuredClone($state.snapshot(items));
    relayout(next, heightOf, width);
    items = next;
    height = Math.max(0, ...WIDGET_IDS.map((id) => (next[id]?.y ?? 0) + heightOf(id))) + GAP;
  }

  function save() {
    onsave?.($state.snapshot(items));
  }

  // Board width tracking + content-height reflow.
  $effect(() => {
    if (!boardEl) return;
    // No ResizeObserver (jsdom, very old browsers): stacked fallback —
    // every panel still renders, in board order.
    if (typeof ResizeObserver === 'undefined') {
      narrow = true;
      return;
    }
    const ro = new ResizeObserver(() => {
      const w = boardEl?.clientWidth ?? 0;
      narrow = window.innerWidth < NARROW_BP;
      if (w !== width) {
        width = w;
        if (!Object.keys(items).length) load();
      }
      pack();
    });
    ro.observe(boardEl);
    for (const id of WIDGET_IDS) if (els[id]?.firstElementChild) ro.observe(els[id].firstElementChild);
    return () => ro.disconnect();
  });

  // A synced layout from another device (or first load) replaces ours
  // unless we're mid-drag.
  let lastSaved = '';
  $effect(() => {
    const key = JSON.stringify(saved ?? null);
    if (key === lastSaved || dragId || !width) return;
    lastSaved = key;
    load();
  });

  // Reset Layout (header button) bumps resetSignal.
  let lastReset = 0;
  $effect(() => {
    if (resetSignal === lastReset) return;
    lastReset = resetSignal;
    items = defaultLayout(width);
    pack();
    onsave?.(null);
  });

  const interactive = (/** @type {Element} */ el) =>
    !!el.closest('button,input,select,textarea,a,[contenteditable="true"],.has-tip,[role="tab"],summary');

  /** Drag from a panel's h2 title bar. */
  function onDown(/** @type {PointerEvent} */ e, /** @type {string} */ id) {
    if (narrow || e.button !== 0) return;
    const t = /** @type {Element} */ (e.target);
    if (!t.closest('h2') || interactive(t)) return;
    const el = els[id];
    const rect = el.getBoundingClientRect();
    const offX = e.clientX - rect.left;
    const offY = e.clientY - rect.top;
    const sx = e.clientX;
    const sy = e.clientY;
    const w = items[id].w;
    const h = heightOf(id);
    let started = false;
    /** @type {any} */
    let result = null;
    const move = (/** @type {PointerEvent} */ ev) => {
      if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
      if (!started) {
        started = true;
        dragId = id;
        document.body.classList.add('noselect');
      }
      const b = /** @type {HTMLElement} */ (boardEl).getBoundingClientRect();
      const x = Math.max(0, ev.clientX - b.left - offX);
      const y = Math.max(0, ev.clientY - b.top - offY);
      el.style.left = `${x}px`;
      el.style.top = `${y}px`;
      const others = WIDGET_IDS.filter((k) => k !== id).map((k) => items[k]);
      const fit = fitRight({ x: snapX(x, others), w }, width);
      const order = orderForDrag(id, fit.x, y, fit.w, items, heightOf);
      /** @type {Record<string, {x: number, w: number, h: number}>} */
      const dims = {};
      for (const k of order) dims[k] = k === id ? { x: fit.x, w: fit.w, h } : { x: items[k].x, w: items[k].w, h: heightOf(k) };
      result = layoutPass(order, dims);
      for (const k of order) {
        if (k === id) continue;
        els[k].style.left = `${result[k].x}px`;
        els[k].style.top = `${result[k].y}px`;
      }
      ph = { x: result[id].x, y: result[id].y, w: fit.w, h };
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.body.classList.remove('noselect');
      if (!started) return;
      const next = structuredClone($state.snapshot(items));
      if (result) for (const k of Object.keys(result)) Object.assign(next[k], { x: result[k].x, y: result[k].y, w: result[k].w });
      items = next;
      dragId = null;
      ph = null;
      pack();
      save();
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  /** Resize from an edge/corner handle. */
  function onResize(/** @type {PointerEvent} */ e, /** @type {string} */ id, /** @type {string} */ dir) {
    e.preventDefault();
    e.stopPropagation();
    const sx = e.clientX;
    const sy = e.clientY;
    const start = { ...items[id], h: heightOf(id) };
    dragId = id;
    const move = (/** @type {PointerEvent} */ ev) => {
      const dx = ev.clientX - sx;
      const dy = ev.clientY - sy;
      const it = { ...items[id] };
      if (dir.includes('e')) it.w = Math.max(MINW, Math.min(width - start.x, start.w + dx));
      if (dir.includes('w')) {
        const nx = Math.max(0, Math.min(start.x + start.w - MINW, start.x + dx));
        it.w = start.w + (start.x - nx);
        it.x = nx;
      }
      if (dir.includes('s')) it.h = Math.max(MINH, start.h + dy);
      if (dir.includes('n')) it.h = Math.max(MINH, start.h - dy);
      items = { ...items, [id]: it };
      pack();
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      dragId = null;
      save();
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  /** Double-click a resize handle: back to auto height. */
  function autoHeight(/** @type {string} */ id) {
    items = { ...items, [id]: { ...items[id], h: null } };
    pack();
    save();
  }

  const HANDLES = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];
</script>

<div class="wboard" class:narrow bind:this={boardEl} style:height={narrow ? 'auto' : `${height}px`}>
  {#each WIDGET_IDS as id (id)}
    {@const it = items[id]}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- Reason: pointer drag starts only from the panel's h2 title; the
    panel's own controls stay keyboard-operable and are skipped above. -->
    <div
      class="widget"
      class:dragging={dragId === id}
      class:fixedh={!narrow && it?.h != null}
      data-w={id}
      bind:this={els[id]}
      style:left={narrow || !it ? null : `${it.x}px`}
      style:top={narrow || !it ? null : `${it.y}px`}
      style:width={narrow || !it ? null : `${it.w}px`}
      style:height={narrow || !it || it.h == null ? null : `${it.h}px`}
      onpointerdown={(e) => onDown(e, id)}
    >
      {@render children(id)}
      {#if !narrow}
        {#each HANDLES as dir (dir)}
          <span class="rz rz-{dir}" role="separator" aria-hidden="true" onpointerdown={(e) => onResize(e, id, dir)} ondblclick={() => autoHeight(id)}></span>
        {/each}
      {/if}
    </div>
  {/each}
  {#if ph}<div class="wplaceholder" style:left="{ph.x}px" style:top="{ph.y}px" style:width="{ph.w}px" style:height="{ph.h}px"></div>{/if}
</div>

<style>
  .wboard {
    position: relative;
    margin: 5px 5px 0;
  }
  .widget {
    position: absolute;
    transition: left 0.15s ease, top 0.15s ease;
  }
  .widget.dragging {
    z-index: 100;
    transition: none;
    opacity: 0.92;
  }
  .widget.fixedh > :global(*) {
    height: 100%;
    overflow: auto;
    box-sizing: border-box;
  }
  .widget > :global(*) {
    margin: 0;
  }
  .widget > :global(* > h2) {
    cursor: grab;
    user-select: none;
  }
  .widget.dragging > :global(* > h2) {
    cursor: grabbing;
  }
  .wboard.narrow .widget {
    position: static;
    margin-bottom: 5px;
  }
  .rz {
    position: absolute;
    z-index: 5;
  }
  .rz-n, .rz-s { left: 8px; right: 8px; height: 6px; cursor: ns-resize; }
  .rz-e, .rz-w { top: 8px; bottom: 8px; width: 6px; cursor: ew-resize; }
  .rz-n { top: -3px; }
  .rz-s { bottom: -3px; }
  .rz-e { right: -3px; }
  .rz-w { left: -3px; }
  .rz-ne, .rz-nw, .rz-se, .rz-sw { width: 12px; height: 12px; }
  .rz-ne { top: -3px; right: -3px; cursor: nesw-resize; }
  .rz-sw { bottom: -3px; left: -3px; cursor: nesw-resize; }
  .rz-nw { top: -3px; left: -3px; cursor: nwse-resize; }
  .rz-se { bottom: -3px; right: -3px; cursor: nwse-resize; }
  .widget:has(> .rz:hover) > :global(:first-child) {
    border-color: var(--gold);
    box-shadow: 0 0 0 1px var(--gold-dim);
  }
  .wplaceholder {
    position: absolute;
    border: 1px dashed var(--gold-dim);
    border-radius: 10px;
    background: rgb(220 178 91 / 5%);
    pointer-events: none;
  }
  :global(body.noselect) {
    user-select: none;
  }
</style>
