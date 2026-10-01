<script>
  // Sync debug surface (E7 Task 12) — an internal tool for proving the
  // sync wiring by hand: connect/disconnect, the durable queue, the live
  // field table, the indicator, and a manual write. Not product UI; the
  // sheet surface itself is E6's.
  import { createSync, partySocketUrl } from './index.js';

  /** @type {{ sub: string }} */
  let { sub } = $props();

  let partyId = $state(1);
  /** @type {ReturnType<typeof createSync> | null} */
  let sync = $state(null);
  let connState = $state('offline');
  let syncing = $state(false);
  /** @type {Array<Record<string, *>>} */
  let queueView = $state([]);
  /** @type {Record<string, { target: Record<string, *>, value: *, version: number }>} */
  let fields = $state({});

  let writeCharacterId = $state(1);
  let writeValue = $state(10);

  function render() {
    if (!sync) return;
    connState = sync.connectionState();
    syncing = sync.isSyncing();
    queueView = sync.queue();
    fields = sync.state();
  }

  $effect(() => {
    const instance = createSync({
      url: partySocketUrl(partyId),
      storage: localStorage,
      accountSub: sub,
    });
    sync = instance;
    instance.subscribe(render);
    instance.connect();
    render();
    return () => instance.disconnect();
  });

  function connect() {
    sync?.connect();
    render();
  }

  function disconnect() {
    sync?.disconnect();
    render();
  }

  function writeHp() {
    if (!sync) return;
    sync.write(
      {
        kind: 'vitals',
        character_id: Number(writeCharacterId),
        field: 'hp',
      },
      Number(writeValue),
    );
    render();
  }

  /** @param {Record<string, *>} target */
  function label(target) {
    return JSON.stringify(target);
  }
</script>

<section class="sync-debug">
  <h2>Sync debug</h2>
  <p>
    connection: <strong data-testid="conn-state">{connState}</strong>
    · indicator: <strong data-testid="indicator">{syncing ? 'syncing' : 'idle'}</strong>
    · queue: <strong data-testid="queue-length">{queueView.length}</strong>
  </p>

  <p>
    party
    <input type="number" min="1" bind:value={partyId} />
    <button onclick={connect}>Connect</button>
    <button onclick={disconnect}>Disconnect</button>
  </p>

  <h3>Manual write (vitals/hp)</h3>
  <p>
    character
    <input type="number" min="1" bind:value={writeCharacterId} />
    value
    <input type="number" bind:value={writeValue} />
    <button onclick={writeHp}>Write</button>
  </p>

  <h3>Queue (durable, FIFO)</h3>
  {#if queueView.length === 0}
    <p>(empty)</p>
  {:else}
    <ul>
      {#each queueView as op (op.op_id)}
        <li>
          <code>{op.op_id}</code> base v{op.base_version}
          <code>{label(op.target)}</code> = {JSON.stringify(op.value)}
        </li>
      {/each}
    </ul>
  {/if}

  <h3>Live fields</h3>
  {#if Object.keys(fields).length === 0}
    <p>(none yet — connect to a party)</p>
  {:else}
    <table>
      <thead>
        <tr><th>target</th><th>value</th><th>version</th></tr>
      </thead>
      <tbody>
        {#each Object.values(fields) as field (label(field.target))}
          <tr>
            <td><code>{label(field.target)}</code></td>
            <td>{JSON.stringify(field.value)}</td>
            <td>{field.version}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>
