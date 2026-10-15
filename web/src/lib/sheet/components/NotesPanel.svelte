<script>
  /* global document, crypto, localStorage */
  // Notes (Dave's #pNotes, specs/015 slice 5): rich text with the
  // prototype's toolbar, named pages (Save / Load / New), #tags rendered as
  // chips, and #todo chips that toggle done. Stored per character in the
  // synced `sheet_ui.notes` (whole-row CAS) so it follows the player across
  // devices. Saved HTML is party input: it is re-adopted ONLY through the
  // inert setter (parse + scrub), never assigned as innerHTML.
  import { adoptHTML } from '../../util/inert-html.js';

  /** @type {{ sheetUi: {notes: any[], minions: any[], layout: any}, editable?: boolean,
   *   characterId?: number | null,
   *   onwrite?: (next: {notes: any[], minions: any[], layout: any}) => void }} */
  let { sheetUi, editable = true, characterId = null, onwrite } = $props();

  const RT_BTNS = [
    ['bold', 'B', 'Bold (Ctrl+B)'],
    ['italic', 'I', 'Italic (Ctrl+I)'],
    ['underline', 'U', 'Underline (Ctrl+U)'],
    ['strikeThrough', 'S', 'Strikethrough'],
    ['insertUnorderedList', '• List', 'Bulleted list'],
    ['insertOrderedList', '1. List', 'Numbered list'],
    ['formatBlock:h4', 'Heading', 'Heading'],
    ['formatBlock:p', 'Body', 'Normal text'],
    ['removeFormat', 'Clear', 'Clear formatting'],
  ];

  /** Which saved page is open ('' = scratch / unsaved) — remembered per
   * character on this device, like Dave's sheet reopened your last page. */
  const openKey = $derived(characterId !== null ? `hireling.notes.open.${characterId}` : null);
  let currentId = $state('');
  $effect(() => {
    if (!openKey) return;
    try {
      currentId = localStorage.getItem(openKey) ?? '';
    } catch {
      /* storage blocked: start on scratch */
    }
  });
  function remember(/** @type {string} */ id) {
    if (!openKey) return;
    try {
      if (id) localStorage.setItem(openKey, id);
      else localStorage.removeItem(openKey);
    } catch {
      /* storage blocked: nothing to remember */
    }
  }
  let dirty = $state(false);
  let menu = $state(/** @type {null | 'load' | 'save' | 'tags'} */ (null));
  let saveTitle = $state('');
  /** @type {HTMLElement | null} */
  let ed = $state(null);
  /** Scratch content kept in-memory (and in the first "Scratch" note when
   * saved) so a reload doesn't lose an unsaved page. */
  const SCRATCH = 'scratch';

  const notes = $derived(sheetUi?.notes ?? []);
  const current = $derived(notes.find((n) => n.id === (currentId || SCRATCH)) ?? null);
  const allTags = $derived([...new Set(notes.flatMap((n) => n.tags ?? []))].sort());
  const saved = $derived(notes.filter((n) => n.id !== SCRATCH));

  // Load content whenever the open page changes or the server copy moves
  // and we have no local edits.
  let loadedKey = '';
  $effect(() => {
    if (!ed) return;
    const key = `${currentId}|${current?.html ?? ''}`;
    if (dirty || key === loadedKey) return;
    loadedKey = key;
    adoptHTML(ed, chipify(current?.html ?? ''));
  });

  /** #word → tag chip (outside existing chips), on text nodes only. */
  function chipify(/** @type {string} */ html) {
    return html.replace(/(^|[\s>(])#([a-z][\w-]{0,30})\b(?![^<]*<\/span>)/gi, (_m, pre, tag) => {
      const t = tag.toLowerCase();
      return `${pre}<span class="ntag" data-tag="${t}" contenteditable="false">#${t}</span>`;
    });
  }

  function tagsOf(/** @type {string} */ html) {
    const out = new Set();
    for (const m of html.matchAll(/data-tag="([a-z][\w-]*)"/g)) out.add(m[1]);
    for (const m of html.matchAll(/(?:^|[\s>(])#([a-z][\w-]{0,30})\b/gi)) out.add(m[1].toLowerCase());
    return [...out];
  }

  function exec(/** @type {string} */ cmd) {
    if (!editable) return;
    ed?.focus();
    const [c, arg] = cmd.split(':');
    document.execCommand(c, false, arg ?? undefined);
    dirty = true;
  }

  function persist(/** @type {string} */ id, /** @type {string} */ title, dropScratch = false) {
    if (!ed) return;
    const html = ed.innerHTML;
    const note = { id, title, html, tags: tagsOf(html) };
    const others = notes.filter((n) => n.id !== id && !(dropScratch && n.id === SCRATCH));
    onwrite?.({ ...sheetUi, notes: [...others, note] });
    dirty = false;
    loadedKey = `${currentId}|${html}`;
  }

  let saveTimer = /** @type {any} */ (null);
  function onInput() {
    dirty = true;
    clearTimeout(saveTimer);
    // Autosave to the open page (or scratch) — Dave's sheet saved on edit.
    saveTimer = setTimeout(() => {
      const id = currentId || SCRATCH;
      persist(id, current?.title ?? (id === SCRATCH ? 'Unsaved note' : 'Untitled'));
    }, 900);
  }

  function onBlur() {
    if (!dirty) return;
    // Re-chipify tags typed since the last load, then save.
    if (ed) {
      const html = chipify(ed.innerHTML);
      if (html !== ed.innerHTML) adoptHTML(ed, html);
    }
    clearTimeout(saveTimer);
    const id = currentId || SCRATCH;
    persist(id, current?.title ?? (id === SCRATCH ? 'Unsaved note' : 'Untitled'));
  }

  function onEdClick(/** @type {MouseEvent} */ e) {
    const chip = /** @type {HTMLElement} */ (e.target).closest?.('.ntag[data-tag="todo"]');
    if (chip && editable) {
      if (chip.hasAttribute('data-done')) chip.removeAttribute('data-done');
      else chip.setAttribute('data-done', '1');
      dirty = true;
      onBlur();
    }
  }

  function doSave() {
    const title = saveTitle.trim();
    if (!title) return;
    const id = currentId && currentId !== SCRATCH ? currentId : `n-${crypto.randomUUID().slice(0, 8)}`;
    currentId = id;
    remember(id);
    persist(id, title, true);
    menu = null;
  }

  function load(/** @type {string} */ id) {
    if (dirty) onBlur();
    currentId = id;
    remember(id);
    dirty = false;
    loadedKey = '';
    menu = null;
  }

  function newPage() {
    if (dirty) onBlur();
    currentId = '';
    remember('');
    dirty = false;
    loadedKey = '';
    const others = notes.filter((n) => n.id !== SCRATCH);
    onwrite?.({ ...sheetUi, notes: others });
    if (ed) adoptHTML(ed, '');
  }

  function remove(/** @type {string} */ id) {
    onwrite?.({ ...sheetUi, notes: notes.filter((n) => n.id !== id) });
    if (currentId === id) newPage();
  }

  function insertTag(/** @type {string} */ tag) {
    ed?.focus();
    document.execCommand('insertHTML', false, `<span class="ntag" data-tag="${tag}" contenteditable="false">#${tag}</span>&nbsp;`);
    dirty = true;
    menu = null;
    onBlur();
  }

  /** Paste as plain text (Dave's rule): no foreign markup enters notes. */
  function onPaste(/** @type {ClipboardEvent} */ e) {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    document.execCommand('insertText', false, text);
    dirty = true;
  }

  const todos = $derived.by(() => {
    const html = current?.html ?? '';
    return (html.match(/data-tag="todo"(?![^>]*data-done)/g) ?? []).length;
  });
</script>

<section class="panel notes" aria-label="Notes">
  <h2>
    Notes
    {#if editable}
      <span class="npbtns">
        <button type="button" class="btn gold" onclick={() => { saveTitle = current?.id === SCRATCH ? '' : (current?.title ?? ''); menu = menu === 'save' ? null : 'save'; }}>Save</button>
        <button type="button" class="btn gold" onclick={() => (menu = menu === 'load' ? null : 'load')} disabled={!saved.length}>Load</button>
        <button type="button" class="btn gold" onclick={newPage}>New</button>
      </span>
    {/if}
  </h2>
  {#if menu === 'save'}
    <form class="npmenu" onsubmit={(e) => { e.preventDefault(); doSave(); }}>
      <input placeholder="Page name" bind:value={saveTitle} aria-label="Page name" />
      <button class="btn gold" type="submit" disabled={!saveTitle.trim()}>Save page</button>
    </form>
  {:else if menu === 'load'}
    <div class="npmenu list">
      {#each saved as n (n.id)}
        <div class="nprow">
          <button class="linkish" onclick={() => load(n.id)}>{n.title}</button>
          <span class="tags">{(n.tags ?? []).map((/** @type {string} */ t) => `#${t}`).join(' ')}</span>
          <button class="x" aria-label="Delete {n.title}" onclick={() => remove(n.id)}>×</button>
        </div>
      {/each}
    </div>
  {/if}
  {#if editable}
    <div class="rtbar">
      {#each RT_BTNS as [cmd, label, title] (cmd)}
        <button type="button" {title} class="rt" onmousedown={(e) => { e.preventDefault(); exec(cmd); }}>{label}</button>
      {/each}
      <button type="button" class="rt" disabled={!allTags.length} onclick={() => (menu = menu === 'tags' ? null : 'tags')}>#tags ▾</button>
    </div>
    {#if menu === 'tags'}
      <div class="npmenu tagpick">
        {#each allTags as t (t)}<button class="ntag" onmousedown={(e) => { e.preventDefault(); insertTag(t); }}>#{t}</button>{/each}
      </div>
    {/if}
  {/if}
  <div class="notest">
    {currentId && current ? current.title : 'Unsaved note'}{dirty ? ' · editing' : ''}{todos ? ` · ${todos} open #todo` : ''}
  </div>
  <div
    class="noteed"
    contenteditable={editable}
    spellcheck="true"
    role="textbox"
    tabindex="0"
    aria-multiline="true"
    aria-label="Notes"
    data-placeholder="Write anything here: session notes, NPC names, loot, plans. Type #tag to tag it, #todo for a checkbox."
    bind:this={ed}
    oninput={onInput}
    onblur={onBlur}
    onpaste={onPaste}
    onclick={onEdClick}
    onkeydown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); onBlur(); } }}
  ></div>
</section>

<style>
  .notes {
    display: flex;
    flex-direction: column;
  }
  .npbtns {
    float: right;
    display: flex;
    gap: 4px;
  }
  .npbtns .btn {
    font: 400 13px var(--sans);
    padding: 2px 12px;
  }
  .rtbar {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    margin: 6px 0;
  }
  .rt {
    background: none;
    border: 1px solid var(--line2);
    color: var(--muted);
    border-radius: 5px;
    padding: 2px 9px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .rt:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--gold-dim);
  }
  .notest {
    font-size: 11.5px;
    color: var(--muted);
    margin: 2px 2px 4px;
  }
  .noteed {
    min-height: 320px;
    background: var(--panel2);
    border: 1px solid var(--line2);
    border-radius: 6px;
    padding: 10px 12px;
    font-size: 14px;
    line-height: 1.5;
    text-align: left;
    outline: none;
    overflow: auto;
  }
  .noteed:focus {
    border-color: var(--gold-dim);
  }
  .noteed:empty::before {
    content: attr(data-placeholder);
    color: var(--dim);
  }
  .noteed :global(h4) {
    font-family: var(--serif);
    color: var(--gold);
    margin: 8px 0 4px;
    font-size: 16px;
  }
  .noteed :global(p) {
    margin: 4px 0;
  }
  .noteed :global(.ntag),
  .ntag {
    display: inline-block;
    font-size: 12px;
    color: #9ec0ff;
    background: rgb(80 120 220 / 15%);
    border: 1px solid rgb(120 150 240 / 35%);
    border-radius: 999px;
    padding: 0 7px;
    cursor: default;
  }
  .noteed :global(.ntag[data-tag='todo']) {
    cursor: pointer;
    color: #e9c47c;
    background: rgb(220 178 91 / 12%);
    border-color: rgb(220 178 91 / 40%);
  }
  .noteed :global(.ntag[data-tag='todo'][data-done]) {
    text-decoration: line-through;
    opacity: 0.55;
  }
  .npmenu {
    display: flex;
    gap: 6px;
    margin: 4px 0;
    flex-wrap: wrap;
  }
  .npmenu input {
    flex: 1;
    background: #2a2f38;
    border: 1px solid var(--line2);
    border-radius: 6px;
    color: var(--text);
    padding: 3px 8px;
  }
  .npmenu.list {
    flex-direction: column;
    gap: 2px;
    border: 1px solid var(--line2);
    border-radius: 6px;
    padding: 4px;
  }
  .nprow {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .nprow .linkish {
    background: none;
    border: 0;
    color: var(--gold);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .nprow .tags {
    flex: 1;
    color: var(--muted);
    font-size: 12px;
  }
  .nprow .x {
    background: none;
    border: 0;
    color: var(--muted);
    cursor: pointer;
  }
</style>
