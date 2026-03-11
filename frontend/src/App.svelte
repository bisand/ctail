<script>
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import LogView from './lib/components/LogView.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import { tabStore, activeTab } from './lib/stores/tabs.js';
  import { settings, settingsPanelOpen } from './lib/stores/settings.js';
  import { profiles } from './lib/stores/rules.js';
  import { OpenFileDialog, OpenTab, GetTabLines, GetSettings, ListProfiles, GetProfile } from '../wailsjs/go/main/App.js';
  import { EventsOn } from '../wailsjs/runtime/runtime.js';

  let selectedProfile = 'Common Logs';

  onMount(async () => {
    // Load settings
    try {
      const s = await GetSettings();
      if (s) {
        settings.set(s);
        if (s.theme) {
          document.documentElement.setAttribute('data-theme', s.theme);
        }
      }
    } catch (e) {
      console.error('Failed to load settings:', e);
    }

    // Load profiles
    try {
      const names = await ListProfiles();
      const allProfiles = {};
      for (const name of names) {
        const p = await GetProfile(name);
        if (p) allProfiles[name] = p;
      }
      profiles.set(allProfiles);
      if (names.length > 0) selectedProfile = names[0];
    } catch (e) {
      console.error('Failed to load profiles:', e);
    }

    // Listen for tailer events
    EventsOn('tailer:lines', (data) => {
      if (data.tabId && data.lines) {
        tabStore.appendLines(data.tabId, data.lines);
      }
    });

    EventsOn('tailer:truncated', (data) => {
      if (data.tabId) {
        tabStore.clearLines(data.tabId);
      }
    });

    EventsOn('tailer:error', (data) => {
      console.error(`Tailer error for ${data.tabId}: ${data.message}`);
    });

    // Keyboard shortcuts
    window.addEventListener('keydown', handleGlobalKeydown);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown);
    };
  });

  function handleGlobalKeydown(e) {
    if (e.ctrlKey && e.key === 'o') {
      e.preventDefault();
      openFile();
    }
    if (e.ctrlKey && e.key === 'w') {
      e.preventDefault();
      const tab = $activeTab;
      if (tab) {
        const { CloseTab } = import('../wailsjs/go/main/App.js');
        // Dynamic import won't work well here, use direct store removal
        tabStore.removeTab(tab.id);
      }
    }
  }

  async function openFile() {
    try {
      const path = await OpenFileDialog();
      if (!path) return;
      const tabId = await OpenTab(path);
      const fileName = path.split(/[/\\]/).pop();
      tabStore.addTab(tabId, path, fileName);

      // Lines will arrive via events, but get initial batch
      const lines = await GetTabLines(tabId);
      if (lines && lines.length > 0) {
        tabStore.setLines(tabId, lines);
      }
    } catch (e) {
      console.error('Failed to open file:', e);
    }
  }
</script>

<div class="app">
  <Toolbar onOpenFile={openFile} />
  <TabBar onAddTab={openFile} />
  <div class="main-area">
    <LogView />
    {#if $settingsPanelOpen}
      <SettingsPanel bind:selectedProfile />
    {/if}
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .main-area {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
</style>
