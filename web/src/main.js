/* global document */
import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { registerPwa } from './lib/pwa/register.js';

const target = document.getElementById('app');

if (!target) {
  throw new Error('mount point #app not found in index.html');
}

const app = mount(App, { target });

// The PWA shell (E10): PROD-only registration; dev stays SW-free.
registerPwa();

export default app;
