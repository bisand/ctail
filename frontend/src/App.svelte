<script>
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import LogView from './lib/components/LogView.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import { tabStore, activeTab, tabs } from './lib/stores/tabs.js';
  import { settings, settingsPanelOpen } from './lib/stores/settings.js';
  import { profiles } from './lib/stores/rules.js';
  import { OpenFileDialog, OpenTab, GetTabLines, GetTabTotalLines, GetSettings, GetSavedTabs, ListProfiles, GetProfile } from '../wailsjs/go/main/App.js';
  import { EventsOn } from '../wailsjs/runtime/runtime.js';

  let selectedProfile = 'Common Logs';

  // Load initial lines for a tab after it becomes ready
  async function loadInitialLines(tabId) {
    try {
      const [lines, total] = await Promise.all([
        GetTabLines(tabId),
        GetTabTotalLines(tabId)
      ]);
      if (lines && lines.length > 0) {
        tabStore.setLines(tabId, lines, total);
      }
      tabStore.setStatus(tabId, 'ready');
    } catch (e) {
      console.error('Failed to load initial lines for', tabId, e);
      tabStore.setStatus(tabId, 'error', String(e));
    }
  }

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
      if (data.tabId) {
        tabStore.setStatus(data.tabId, 'error', data.message || 'Unknown error');
      }
    });

    EventsOn('tailer:ready', (data) => {
      if (data.tabId) {
        loadInitialLines(data.tabId);
      }
    });

    // Restore previously open tabs (non-blocking — tabs appear immediately)
    try {
      const savedTabs = await GetSavedTabs();
      if (savedTabs && savedTabs.length > 0) {
        for (const tab of savedTabs) {
          try {
            const fileName = tab.filePath.split(/[/\\]/).pop();
            const tabId = await OpenTab(tab.filePath);
            tabStore.addTab(tabId, tab.filePath, fileName);
            if (tab.profileId) {
              tabStore.setProfile(tabId, tab.profileId);
            }
            // Lines will arrive via tailer:ready → loadInitialLines
          } catch (e) {
            console.warn('Failed to restore tab:', tab.filePath, e);
          }
        }
      }
    } catch (e) {
      console.error('Failed to restore tabs:', e);
    }

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
        tabStore.removeTab(tab.id);
      }
    }
    if (e.ctrlKey && e.key === 'Tab') {
      e.preventDefault();
      const allTabs = $tabs;
      if (allTabs.length < 2) return;
      const currentIdx = allTabs.findIndex(t => t.id === $activeTab?.id);
      const nextIdx = e.shiftKey
        ? (currentIdx - 1 + allTabs.length) % allTabs.length
        : (currentIdx + 1) % allTabs.length;
      tabStore.setActive(allTabs[nextIdx].id);
    }
  }

  async function openFile() {
    try {
      const path = await OpenFileDialog();
      if (!path) return;
      const tabId = await OpenTab(path);
      const fileName = path.split(/[/\\]/).pop();
      tabStore.addTab(tabId, path, fileName);
      // Lines will arrive via tailer:ready → loadInitialLines
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
