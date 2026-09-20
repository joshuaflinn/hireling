import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// The backend serves the built bundle (web/dist) and owns all routing —
// no SvelteKit, no client router at this stage.
export default defineConfig({
  plugins: [svelte()],
});
