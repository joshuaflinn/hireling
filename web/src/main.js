/* global document */
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');

if (!target) {
  throw new Error('mount point #app not found in index.html');
}

const app = mount(App, { target });

export default app;
