import { writable } from 'svelte/store';

export const settings = writable({
  pollIntervalMs: 500,
  bufferSize: 10000,
  readTimeoutSec: 30,
  scrollSpeed: 1,
  smoothScroll: false,
  theme: 'catppuccin',
  themeMode: 'dark',
  fontSize: 14,
  showLineNumbers: false,
  wordWrap: false,
  activeProfile: 'Common Logs',
  displayBackend: 'auto',
  gpuPolicy: 'auto',
});

export const settingsPanelOpen = writable(false);
