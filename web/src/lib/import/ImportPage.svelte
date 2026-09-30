<script>
  // The minimal import page (E5 FR-18): paste or upload an export, submit,
  // and see the result with the post-import diff. Functional only — the
  // sheet surface itself is E6's.
  import { buildBody, describeOutcome, errorFor, readFileToText, submit } from '../import.js';

  let text = $state('');
  let fileName = $state('');
  let busy = $state(false);
  /** @type {{ lines: string[] } | null} */
  let result = $state(null);
  let errorMessage = $state('');

  /** @param {Event} event */
  async function onFileChange(event) {
    const input = /** @type {HTMLInputElement} */ (event.target);
    const file = input.files?.[0];
    if (!file) return;
    fileName = file.name;
    text = await readFileToText(file);
  }

  async function onSubmit() {
    busy = true;
    result = null;
    errorMessage = '';
    try {
      const outcome = await submit(buildBody(text));
      if (outcome.ok && outcome.payload) {
        result = { lines: describeOutcome(outcome.payload) };
      } else {
        errorMessage = errorFor(outcome);
      }
    } finally {
      busy = false;
    }
  }
</script>

<section class="import-page">
  <h2>Import a character</h2>
  <p class="hint">
    In Pathbuilder: Share → Export JSON, then paste the whole document here
    (or upload the .json file).
  </p>

  <textarea
    rows="10"
    placeholder={'{"success":true,"build":{…}}'}
    bind:value={text}
  ></textarea>

  <div class="row">
    <label class="file">
      {fileName || 'Upload a .json export…'}
      <input type="file" accept=".json,application/json" onchange={onFileChange} />
    </label>
    <button onclick={onSubmit} disabled={busy || text.length === 0}>
      {busy ? 'Importing…' : 'Import'}
    </button>
  </div>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  {#if result}
    <ul class="result">
      {#each result.lines as line (line)}
        <li>{line}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .import-page {
    text-align: left;
  }

  .hint {
    color: #9aa4b2;
    font-size: 0.9rem;
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    background: #171a24;
    color: #e6e6e6;
    border: 1px solid #465070;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
    padding: 0.5rem;
  }

  .row {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 0.75rem;
  }

  .file {
    color: #9aa4b2;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .file input {
    display: none;
  }

  .error {
    margin-top: 1rem;
    color: #e07a6a;
    font-family: ui-monospace, monospace;
    font-size: 0.9rem;
  }

  .result {
    margin-top: 1rem;
    color: #c8d0dc;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
    line-height: 1.6;
  }
</style>
