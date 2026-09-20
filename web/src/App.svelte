<script>
  async function checkHealth() {
    status = 'checking…';
    try {
      const response = await fetch('/healthz');
      const body = await response.json();
      status = `backend ${body.status} · version ${body.version}`;
    } catch {
      status = 'backend unreachable';
    }
  }

  let status = $state('not checked');
</script>

<main>
  <h1>Hireling</h1>
  <p class="tagline">Party-linked PF2e character tracking. The sheet is being built.</p>
  <button onclick={checkHealth}>Check backend</button>
  <p class="status">{status}</p>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #0f1117;
    color: #e6e6e6;
    font-family: system-ui, sans-serif;
  }

  main {
    max-width: 40rem;
    margin: 20vh auto 0;
    text-align: center;
  }

  h1 {
    font-size: 3rem;
    letter-spacing: 0.05em;
    margin-bottom: 0.25rem;
  }

  .tagline {
    color: #9aa4b2;
  }

  button {
    margin-top: 2rem;
    padding: 0.6rem 1.4rem;
    font-size: 1rem;
    color: #e6e6e6;
    background: #2b3245;
    border: 1px solid #465070;
    border-radius: 6px;
    cursor: pointer;
  }

  button:hover {
    background: #374060;
  }

  .status {
    margin-top: 1rem;
    color: #9aa4b2;
    font-family: ui-monospace, monospace;
    font-size: 0.9rem;
  }
</style>
