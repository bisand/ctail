import { writable } from 'svelte/store';

export const settings = writable({
  pollIntervalMs: 500,
  bufferSize: 10000,
  theme: 'catppuccin',
  themeMode: 'dark',
  fontSize: 14,
  showLineNumbers: false,
  wordWrap: false,
  activeProfile: 'Common Logs',
});

export const settingsPanelOpen = writable(false);
