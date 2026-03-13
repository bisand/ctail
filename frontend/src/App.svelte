<script>
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import LogView from './lib/components/LogView.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import AboutDialog from './lib/components/AboutDialog.svelte';
  import { tabStore, activeTab, tabs } from './lib/stores/tabs.js';
  import { settings, settingsPanelOpen } from './lib/stores/settings.js';
  import { profiles } from './lib/stores/rules.js';
  import { OpenFileDialog, OpenTab, GetTabLineRange, GetTabTotalLines, GetSettings, GetSavedTabs, SaveTabOrder, SaveSettings, ListProfiles, GetProfile } from '../wailsjs/go/main/App.js';
  import { EventsOn } from '../wailsjs/runtime/runtime.js';

  let scrollBuffer = 500;

  // About dialog state
  let showAbout = false;

  // Ctrl+Tab cycling state
  let isCycling = false;
  let cycleIndex = -1;
  let tabIdBeforeCycle = null; // tab we were on when a cycle session started
  let lastCycleEndTime = 0;   // timestamp when Ctrl was released after cycling
  const TOGGLE_THRESHOLD_MS = 1000; // quick re-press window for toggle

  // Track the previous tab for toggle: set when a cycle session ends
  let previousTabId = null;

  // Persist tab order to backend whenever tabs change
  let tabsInitialized = false;
  $: if (tabsInitialized && $tabs) {
    const tabStates = $tabs.map(t => ({
      filePath: t.filePath,
      profileId: t.profile || '',
      autoScroll: t.autoScroll || true,
    }));
    SaveTabOrder(tabStates).catch(() => {});
  }

  // If the previous tab gets closed, clear it
  $: {
    const ids = new Set($tabs.map(t => t.id));
    if (previousTabId && !ids.has(previousTabId)) {
      previousTabId = null;
    }
  }

  // Load initial lines for a tab after it becomes ready
  async function loadInitialLines(tabId) {
    try {
      const total = await GetTabTotalLines(tabId);
      const fetchStart = Math.max(1, total - scrollBuffer + 1);
      const lines = await GetTabLineRange(tabId, fetchStart, scrollBuffer);
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
        scrollBuffer = s.scrollBuffer || 500;
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

    // Menu bar events
    EventsOn('menu:open-file', () => {
      openFile();
    });

    EventsOn('menu:open-recent', (filePath) => {
      openRecentFile(filePath);
    });

    EventsOn('menu:close-tab', () => {
      const tab = $activeTab;
      if (tab) {
        tabStore.removeTab(tab.id);
      }
    });

    EventsOn('menu:toggle-settings', () => {
      settingsPanelOpen.update(v => !v);
    });

    EventsOn('menu:toggle-theme', async () => {
      const currentSettings = $settings;
      const newTheme = currentSettings.theme === 'dark' ? 'light' : 'dark';
      const updated = { ...currentSettings, theme: newTheme };
      settings.set(updated);
      document.documentElement.setAttribute('data-theme', newTheme);
      try {
        await SaveSettings(updated);
      } catch (e) {
        console.error('Failed to save theme:', e);
      }
    });

    EventsOn('menu:about', () => {
      showAbout = true;
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

    // Enable tab order persistence now that restoration is complete
    tabsInitialized = true;

    // Keyboard shortcuts — use capture phase so WebKit doesn't swallow
    // key combos like Ctrl+Shift+Tab before they reach our handler.
    window.addEventListener('keydown', handleGlobalKeydown, true);
    window.addEventListener('keyup', handleGlobalKeyup, true);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown, true);
      window.removeEventListener('keyup', handleGlobalKeyup, true);
    };
  });

  function handleGlobalKeyup(e) {
    if ((e.key === 'Control' || !e.ctrlKey) && isCycling) {
      isCycling = false;
      previousTabId = tabIdBeforeCycle;
      lastCycleEndTime = Date.now();
      cycleIndex = -1;
      tabIdBeforeCycle = null;
    }
  }

  function handleGlobalKeydown(e) {
    if (e.ctrlKey && e.key === 'o') {
      e.preventDefault();
      openFile();
    }
    if (e.ctrlKey && (e.key === 'w' || e.key === 'F4')) {
      e.preventDefault();
      const tab = $activeTab;
      if (tab) {
        const { CloseTab } = import('../wailsjs/go/main/App.js');
        tabStore.removeTab(tab.id);
      }
    }
    if (e.ctrlKey && e.key === 'Tab') {
      e.preventDefault();
      handleTabCycle(e.shiftKey);
    }
    // Ctrl+PageDown / Ctrl+PageUp as alternative tab cycling keys
    // (Ctrl+Shift+Tab is intercepted by WebKit on some platforms)
    if (e.ctrlKey && e.key === 'PageDown') {
      e.preventDefault();
      handleTabCycle(false);
    }
    if (e.ctrlKey && e.key === 'PageUp') {
      e.preventDefault();
      handleTabCycle(true);
    }
  }

  function handleTabCycle(reverse) {
    const allTabs = $tabs;
    if (allTabs.length < 2) return;

    if (!isCycling) {
      isCycling = true;
      tabIdBeforeCycle = $activeTab?.id;

      // Quick re-press after a previous cycle: toggle back
      const elapsed = Date.now() - lastCycleEndTime;
      if (previousTabId && elapsed < TOGGLE_THRESHOLD_MS) {
        tabStore.setActive(previousTabId);
        cycleIndex = allTabs.findIndex(t => t.id === previousTabId);
        return;
      }

      // Normal first press: move to next/prev in visual order
      const currentIdx = allTabs.findIndex(t => t.id === $activeTab?.id);
      cycleIndex = reverse
        ? (currentIdx - 1 + allTabs.length) % allTabs.length
        : (currentIdx + 1) % allTabs.length;
      tabStore.setActive(allTabs[cycleIndex].id);
    } else {
      // Subsequent presses while Ctrl held: cycle in visual order
      if (reverse) {
        cycleIndex = (cycleIndex - 1 + allTabs.length) % allTabs.length;
      } else {
        cycleIndex = (cycleIndex + 1) % allTabs.length;
      }
      tabStore.setActive(allTabs[cycleIndex].id);
    }
  }

  async function openFile() {
    try {
      // Default to the active tab's directory if one is open
      let defaultDir = '';
      const tab = $activeTab;
      if (tab && tab.filePath) {
        const sep = tab.filePath.includes('\\') ? '\\' : '/';
        defaultDir = tab.filePath.substring(0, tab.filePath.lastIndexOf(sep));
      }
      const path = await OpenFileDialog(defaultDir);
      if (!path) return;
      const tabId = await OpenTab(path);
      const fileName = path.split(/[/\\]/).pop();
      tabStore.addTab(tabId, path, fileName);
      // Lines will arrive via tailer:ready → loadInitialLines
    } catch (e) {
      console.error('Failed to open file:', e);
    }
  }

  async function openRecentFile(filePath) {
    try {
      const tabId = await OpenTab(filePath);
      const fileName = filePath.split(/[/\\]/).pop();
      tabStore.addTab(tabId, filePath, fileName);
    } catch (e) {
      console.error('Failed to open recent file:', e);
    }
  }
</script>

<div class="app">
  <Toolbar onOpenFile={openFile} />
  <TabBar onAddTab={openFile} />
  <div class="main-area">
    <LogView />
    {#if $settingsPanelOpen}
      <SettingsPanel />
    {/if}
  </div>
</div>

<AboutDialog bind:show={showAbout} />

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
