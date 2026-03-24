<script>
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import LogView from './lib/components/LogView.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import AboutDialog from './lib/components/AboutDialog.svelte';
  import AIDialog from './lib/components/AIDialog.svelte';
  import UpdateDialog from './lib/components/UpdateDialog.svelte';
  import { tabStore, activeTab, tabs } from './lib/stores/tabs.js';
  import { settings, settingsPanelOpen } from './lib/stores/settings.js';
  import { profiles } from './lib/stores/rules.js';
  import { OpenFileDialog, OpenTab, GetTabLineRange, GetTabTotalLines, GetSettings, GetSavedTabs, SaveTabOrder, SaveSettings, ListProfiles, GetProfile, ListThemes, ManualCheckForUpdates } from '../wailsjs/go/main/App.js';
  import { EventsOn, BrowserOpenURL } from '../wailsjs/runtime/runtime.js';
  import { loadAndApplyTheme } from './lib/utils/themes.js';

  let scrollBuffer = 500;

  // About dialog state
  let showAbout = false;

  // AI dialog state
  let showAI = false;

  // Update dialog state
  let showUpdateDialog = false;
  let updateCheckResult = null;

  // Update notification state
  let updateAvailable = null; // { version, url }

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

  // Load initial lines for a tab after it becomes ready.
  // Guards against concurrent calls for the same tab (e.g. rapid ready→error→ready cycling).
  const pendingInitLoads = new Set();
  async function loadInitialLines(tabId) {
    if (pendingInitLoads.has(tabId)) return;
    pendingInitLoads.add(tabId);
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
    } finally {
      pendingInitLoads.delete(tabId);
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
          const themeName = s.theme || 'catppuccin';
          const themeMode = s.themeMode || 'dark';
          await loadAndApplyTheme(themeName, themeMode);
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

    // Listen for tailer events — batch line events per animation frame to
    // prevent store/DOM update flooding from overwhelming WebKit's renderer.
    const pendingLines = new Map();
    let lineRafScheduled = false;

    function flushPendingLines() {
      lineRafScheduled = false;
      for (const [tabId, lines] of pendingLines) {
        tabStore.appendLines(tabId, lines);
      }
      pendingLines.clear();
    }

    EventsOn('tailer:lines', (data) => {
      if (data.tabId && data.lines) {
        const existing = pendingLines.get(data.tabId) || [];
        existing.push(...data.lines);
        pendingLines.set(data.tabId, existing);
        if (!lineRafScheduled) {
          lineRafScheduled = true;
          requestAnimationFrame(flushPendingLines);
        }
      }
    });

    EventsOn('tailer:truncated', (data) => {
      if (data.tabId) {
        tabStore.clearLines(data.tabId);
      }
    });

    // Debounce error events per tab — at most one UI update per second per tab
    const errorTimers = new Map();
    EventsOn('tailer:error', (data) => {
      if (data.tabId) {
        if (errorTimers.has(data.tabId)) return;
        console.error(`Tailer error for ${data.tabId}: ${data.message}`);
        tabStore.setStatus(data.tabId, 'error', data.message || 'Unknown error');
        errorTimers.set(data.tabId, setTimeout(() => errorTimers.delete(data.tabId), 1000));
      }
    });

    EventsOn('tailer:ready', (data) => {
      if (data.tabId) {
        loadInitialLines(data.tabId);
      }
    });

    EventsOn('tailer:reconnecting', (data) => {
      if (data.tabId) {
        tabStore.setStatus(data.tabId, 'loading');
      }
    });

    EventsOn('tab:focus', (tabId) => {
      if (tabId) {
        activeTab.set(tabId);
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
      const newMode = currentSettings.themeMode === 'dark' ? 'light' : 'dark';
      const updated = { ...currentSettings, themeMode: newMode };
      settings.set(updated);
      await loadAndApplyTheme(updated.theme || 'catppuccin', newMode);
      try {
        await SaveSettings(updated);
      } catch (e) {
        console.error('Failed to save theme:', e);
      }
    });

    EventsOn('menu:about', () => {
      showAbout = true;
    });

    EventsOn('menu:copy', () => {
      document.execCommand('copy');
    });

    EventsOn('menu:select-all', () => {
      document.execCommand('selectAll');
    });

    EventsOn('menu:find', () => {
      window.dispatchEvent(new CustomEvent('ctail:find'));
    });

    EventsOn('menu:ai-assistant', () => {
      showAI = true;
    });

    EventsOn('menu:check-updates', async () => {
      try {
        const result = await ManualCheckForUpdates();
        updateCheckResult = result;
        showUpdateDialog = true;
        // Also show the silent banner if update is available
        if (result.updateAvailable) {
          updateAvailable = { version: result.latestVersion, url: result.url };
        }
      } catch (e) {
        updateCheckResult = { error: String(e), currentVersion: '' };
        showUpdateDialog = true;
      }
    });

    EventsOn('app:update-available', (data) => {
      updateAvailable = data;
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
    window.addEventListener('ctail:open-ai', () => { showAI = true; });

    // WebKit repaint recovery: when the page regains visibility (e.g. after
    // monitor switch, VPN reconnection, or compositor stall), nudge the
    // renderer to repaint by toggling a CSS transform.
    function forceRepaint() {
      const el = document.documentElement;
      el.style.transform = 'translateZ(0)';
      requestAnimationFrame(() => {
        el.style.transform = '';
      });
    }

    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) forceRepaint();
    });

    window.addEventListener('focus', forceRepaint);
    window.addEventListener('resize', forceRepaint);

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
    if (e.ctrlKey && e.shiftKey && (e.key === 'A' || e.key === 'a')) {
      e.preventDefault();
      showAI = !showAI;
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
  {#if updateAvailable}
    <div class="update-banner">
      <span>🎉 ctail v{updateAvailable.version} is available!</span>
      <button class="update-link" on:click={() => BrowserOpenURL(updateAvailable.url)}>View release</button>
      <button class="update-dismiss" on:click={() => updateAvailable = null}>✕</button>
    </div>
  {/if}
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
<AIDialog bind:show={showAI} />
<UpdateDialog bind:show={showUpdateDialog} result={updateCheckResult} />

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

  .update-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--accent);
    font-size: 12px;
    color: var(--text-primary);
  }

  .update-link {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    text-decoration: underline;
    padding: 0;
  }

  .update-link:hover {
    color: var(--text-primary);
  }

  .update-dismiss {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
  }

  .update-dismiss:hover {
    color: var(--text-primary);
  }
</style>
