import { writable } from 'svelte/store';

export const settings = writable({
  pollIntervalMs: 500,
  bufferSize: 10000,
  theme: 'dark',
  fontSize: 14,
  showLineNumbers: false,
  wordWrap: false,
});

export const settingsPanelOpen = writable(false);
