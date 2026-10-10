<script>
  // The about view (E9 Task 10, spec US-5 AC-3): the repo's NOTICE.md
  // rendered in-app, through the one inert setter — the same law every
  // prose surface runs under (FR-3). The notice is a build-time copy
  // (`rules/notice.md`); a test pins the copy to the repo file so the two
  // cannot drift.
  import { adoptHTML } from '../../util/inert-html.js';
  import defaultNotice from '../../rules/notice.md?raw';

  /** The notice text; the default is the bundled copy of NOTICE.md. */
  let { notice = defaultNotice, onback } = $props();

  /** @type {HTMLElement | undefined} */
  let body;

  // Markdown-lite, inert by construction: escape first, then add the
  // structural tags the notice actually uses (headings, bullets, fenced
  // block, paragraphs). No inline markup of the source survives escaping —
  // hostile input renders as visible text, never as nodes or attributes.
  function escapeHtml(/** @type {string} */ text) {
    return text
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;');
  }

  function renderNotice(/** @type {string} */ markdown) {
    const lines = markdown.split('\n');
    const out = [];
    let inFence = false;
    let inList = false;
    let paragraph = [];
    const flushParagraph = () => {
      if (paragraph.length > 0) {
        out.push(`<p>${paragraph.map(escapeHtml).join('<br>') }</p>`);
        paragraph = [];
      }
    };
    const closeList = () => {
      if (inList) {
        out.push('</ul>');
        inList = false;
      }
    };
    for (const line of lines) {
      if (/^```\w*\s*$/.test(line)) {
        flushParagraph();
        closeList();
        out.push(inFence ? '</pre>' : '<pre>');
        inFence = !inFence;
        continue;
      }
      if (inFence) {
        out.push(escapeHtml(line));
        continue;
      }
      const heading = line.match(/^(#{1,3})\s+(.*)$/);
      if (heading) {
        flushParagraph();
        closeList();
        out.push(`<h2>${escapeHtml(heading[2])}</h2>`);
        continue;
      }
      const bullet = line.match(/^-\s+(.*)$/);
      if (bullet) {
        flushParagraph();
        if (!inList) {
          out.push('<ul>');
          inList = true;
        }
        out.push(`<li>${escapeHtml(bullet[1])}</li>`);
        continue;
      }
      if (line.trim() === '') {
        flushParagraph();
        closeList();
        continue;
      }
      paragraph.push(line);
    }
    flushParagraph();
    closeList();
    if (inFence) out.push('</pre>');
    return out.join('\n');
  }

  $effect(() => {
    if (body) {
      adoptHTML(body, renderNotice(notice));
    }
  });
</script>

<main class="about">
  <button class="back" onclick={onback}>← Back</button>
  <article class="notice" bind:this={body}></article>
</main>

<style>
  .about {
    max-width: 48rem;
    margin: 2rem auto 0;
    padding: 0 1rem;
  }

  .back {
    display: block;
    margin: 0 0 1rem auto;
  }

  .notice {
    color: var(--text, #e9e5d9);
    background: var(--panel, #1a1d24);
    border: 1px solid #465070;
    border-radius: 8px;
    padding: 1.25rem;
    text-align: left;
    line-height: 1.5;
  }

  .notice :global(h2) {
    margin: 1.25rem 0 0.5rem;
    font-size: 1.1rem;
  }

  .notice :global(h2:first-child) {
    margin-top: 0;
  }

  .notice :global(pre) {
    background: #111317;
    border-radius: 6px;
    padding: 0.75rem;
    overflow-x: auto;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }

  .notice :global(ul) {
    margin: 0.5rem 0;
    padding-left: 1.25rem;
  }

  .notice :global(li) {
    margin-bottom: 0.5rem;
  }
</style>
