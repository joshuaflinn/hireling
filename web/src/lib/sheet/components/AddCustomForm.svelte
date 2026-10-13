<script>
  // The shared "Add custom" form (E9 FR-4): minimal fields per kind —
  // name + description always; Rank for spells (0..10), optional Value for
  // conditions (1..20), nothing for items (the server refuses item values;
  // the form never sends one). The client's caps run BEFORE onsubmit — an
  // invalid row never leaves the client (the server re-validates; a thrown
  // {field, reason} from it lands in the same inline slot). No store talk
  // here: the parent owns the create.
  import { validateCustom } from '../../rules/custom-store.js';

  /** @type {{ kind: 'item' | 'spell' | 'condition',
    onsubmit?: (fields: {name: string, description: string, value_or_rank: number | null}) => any,
    oncancel?: () => void }} */
  let { kind, onsubmit, oncancel } = $props();

  let name = $state('');
  let description = $state('');
  // `kind` freezes at mount by design, here and in `valueLabel` below:
  // SheetView gates this form behind `{#if customFormKind}` and Dialog opens
  // it with showModal(), so the instance unmounts before `kind` could change
  // (svelte-check state_referenced_locally — ruled not-a-defect, MOR-129).
  let valueOrRank = $state(kind === 'spell' ? '1' : '');
  /** @type {Record<string, string>} */
  let errors = $state({});
  let submitting = $state(false);

  const valueLabel = kind === 'spell' ? 'Rank' : 'Value (optional)';

  /** Wire the field's value by kind (spell rank is required, condition value optional). */
  const valuePayload = $derived.by(() => {
    if (kind === 'spell') return valueOrRank === '' ? null : Number(valueOrRank);
    if (kind === 'condition') return valueOrRank === '' ? null : Number(valueOrRank);
    return null;
  });

  /** @param {SubmitEvent} event */
  async function handleSubmit(event) {
    event.preventDefault();
    const fields = { name, description, value_or_rank: valuePayload };
    const invalid = validateCustom(kind, fields);
    if (invalid) {
      errors = { [invalid.field]: invalid.reason };
      return;
    }
    errors = {};
    submitting = true;
    try {
      await onsubmit?.(fields);
    } catch (problem) {
      const failure = /** @type {{field?: string, reason?: string}} */ (problem);
      const inline = failure && failure.field ? { [failure.field]: String(failure.reason ?? 'refused') } : null;
      errors = inline ?? { name: 'the create failed — try again' };
    } finally {
      submitting = false;
    }
  }
</script>

<form class="add-custom" onsubmit={handleSubmit}>
  <h4>Add custom {kind}</h4>
  <label>
    <span>Name</span>
    <input name="name" bind:value={name} maxlength="80" aria-label="Name" placeholder="1..64 characters" />
  </label>
  <label>
    <span>Description</span>
    <input name="description" bind:value={description} maxlength="300" aria-label="Description" placeholder="one line, up to 280 characters" />
  </label>
  {#if kind !== 'item'}
    <label>
      <span>{valueLabel}</span>
      <input name="value_or_rank" type="number" bind:value={valueOrRank} aria-label={valueLabel} />
    </label>
  {/if}
  {#if errors.name}
    <p class="op-error" role="alert">Name: {errors.name}</p>
  {/if}
  {#if errors.description}
    <p class="op-error" role="alert">Description: {errors.description}</p>
  {/if}
  {#if errors.value_or_rank}
    <p class="op-error" role="alert">{kind === 'spell' ? 'Rank' : 'Value'}: {errors.value_or_rank}</p>
  {/if}
  <div class="actions">
    <button type="submit" class="btn" disabled={submitting}>
      {submitting ? 'Creating…' : 'Create'}
    </button>
    {#if oncancel}
      <button type="button" class="btn" onclick={oncancel}>Cancel</button>
    {/if}
  </div>
</form>

<style>
  .add-custom {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 260px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: var(--muted, #8a94a3);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12.5px;
    color: var(--muted, #8a94a3);
  }
  input {
    background: var(--panel, #10141b);
    border: 1px solid var(--edge, #2c3444);
    border-radius: 6px;
    color: var(--text, #e8e4d8);
    padding: 5px 8px;
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 2px;
  }
  .op-error {
    margin: 0;
    color: var(--danger, #d06a5a);
    font-size: 12.5px;
  }
</style>
