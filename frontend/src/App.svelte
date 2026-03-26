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
  import { OpenFileDialog, OpenTab, GetTabLineRange, GetTabTotalLines, GetSettings, GetSavedTabs, GetPendingFiles, SaveSettings, ListProfiles, GetProfile, ListThemes, ManualCheckForUpdates } from '../wailsjs/go/main/App.js';
  import { EventsOn, BrowserOpenURL } from '../wailsjs/runtime/runtime.js';
  import { loadAndApplyTheme } from './lib/utils/themes.js';

  const INITIAL_TAIL = 1000;

  // About dialog state
  let showAbout = $state(false);

  // AI dialog state
  let showAI = $state(false);

  // Update dialog state
  let showUpdateDialog = $state(false);
  let updateCheckResult = $state(null);

  // Update notification state
  let updateAvailable = $state(null);

  // Ctrl+Tab cycling state
  let isCycling = false;
  let cycleIndex = -1;
  let tabIdBeforeCycle = null;
  let lastCycleEndTime = 0;
  const TOGGLE_THRESHOLD_MS = 1000;

  // Track the previous tab for toggle
  let previousTabId = null;

  // If the previous tab gets closed, clear it
  $effect(() => {
    const ids = new Set($tabs.map(t => t.id));
    if (previousTabId && !ids.has(previousTabId)) {
      previousTabId = null;
    }
  });

  // Load initial lines for a tab after it becomes ready.
  // Guards against concurrent calls for the same tab (e.g. rapid ready→error→ready cycling).
  const pendingInitLoads = new Set();
  async function loadInitialLines(tabId) {
    if (pendingInitLoads.has(tabId)) return;
    pendingInitLoads.add(tabId);
    try {
      const total = await GetTabTotalLines(tabId);
      const fetchStart = Math.max(1, total - INITIAL_TAIL + 1);
      const lines = await GetTabLineRange(tabId, fetchStart, INITIAL_TAIL);
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

    // Listen for tailer events — batch line events and flush via a timer
    // instead of requestAnimationFrame, because WebKit2GTK stops firing
    // RAF callbacks when the window is hidden/backgrounded, which causes
    // events to pile up and freeze the UI on return.
    const pendingLines = new Map();
    let lineFlushTimer = null;
    const FLUSH_INTERVAL_MS = 100;

    function flushPendingLines() {
      lineFlushTimer = null;
      for (const [tabId, lines] of pendingLines) {
        tabStore.appendLines(tabId, lines);
      }
      pendingLines.clear();
    }

    EventsOn('tailer:lines', (data) => {
      if (data.tabId && data.lines) {
        // When backgrounded, discard events for inactive tabs and cap
        // the pending buffer to avoid unbounded memory growth.
        if (document.hidden) {
          // Only keep lines for the active tab, and cap at 2000
          const activeId = tabStore.getActiveTabId();
          if (data.tabId !== activeId) return;
          const existing = pendingLines.get(data.tabId) || [];
          existing.push(...data.lines);
          if (existing.length > 2000) existing.splice(0, existing.length - 2000);
          pendingLines.set(data.tabId, existing);
          return;
        }
        const existing = pendingLines.get(data.tabId) || [];
        existing.push(...data.lines);
        pendingLines.set(data.tabId, existing);
        if (!lineFlushTimer) {
          lineFlushTimer = setTimeout(flushPendingLines, FLUSH_INTERVAL_MS);
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

    // Files opened externally (macOS Finder, or emitted from Go)
    EventsOn('file:open-external', async (filePath) => {
      try {
        const fileName = filePath.split(/[/\\]/).pop();
        const tabId = await OpenTab(filePath);
        tabStore.addTab(tabId, filePath, fileName);
      } catch (e) {
        console.warn('Failed to open external file:', filePath, e);
      }
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

    // Restore saved tabs and open CLI files concurrently — fire all OpenTab
    // calls without awaiting each one so the UI stays responsive.  Tab order
    // is preserved by sorting before we start, and addTab is synchronous.
    try {
      const savedTabs = await GetSavedTabs();
      const pending = await GetPendingFiles();

      if (savedTabs && savedTabs.length > 0) {
        const sorted = [...savedTabs].sort((a, b) => (a.position || 0) - (b.position || 0));
        for (const tab of sorted) {
          OpenTab(tab.filePath).then(tabId => {
            tabStore.addTab(tabId, tab.filePath, tab.filePath.split(/[/\\]/).pop());
            if (tab.profileId) tabStore.setProfile(tabId, tab.profileId);
            if (tab.label) tabStore.setLabel(tabId, tab.label);
            if (tab.color) tabStore.setColor(tabId, tab.color);
          }).catch(e => console.warn('Failed to restore tab:', tab.filePath, e));
        }
      }

      if (pending && pending.length > 0) {
        for (const filePath of pending) {
          OpenTab(filePath).then(tabId => {
            tabStore.addTab(tabId, filePath, filePath.split(/[/\\]/).pop());
          }).catch(e => console.warn('Failed to open CLI file:', filePath, e));
        }
      }
    } catch (e) {
      console.error('Failed to restore tabs:', e);
    }

    // Keyboard shortcuts — use capture phase so WebKit doesn't swallow
    // key combos like Ctrl+Shift+Tab before they reach our handler.
    window.addEventListener('keydown', handleGlobalKeydown, true);
    window.addEventListener('keyup', handleGlobalKeyup, true);
    window.addEventListener('ctail:open-ai', () => { showAI = true; });

    // WebKit2GTK repaint recovery.  The compositor can stall when the
    // window loses focus — DOM updates still apply but nothing paints.
    // Instead of periodic CSS nudges on the root element (which cause
    // visible flickering), we use a hidden 1×1 sentinel element and only
    // trigger repaints on actual user interaction or window focus.

    // Create an invisible sentinel for repaint nudges — toggling a
    // property on this element forces a micro-repaint without touching
    // any visible content.
    const sentinel = document.createElement('div');
    sentinel.setAttribute('aria-hidden', 'true');
    sentinel.style.cssText = 'position:fixed;width:1px;height:1px;top:-1px;left:-1px;opacity:0;pointer-events:none;z-index:-1;';
    document.body.appendChild(sentinel);
    let sentinelToggle = false;

    function nudgeRepaint() {
      sentinelToggle = !sentinelToggle;
      sentinel.style.transform = sentinelToggle ? 'translateX(1px)' : '';
      void sentinel.offsetHeight;
    }

    function flushAndRepaint() {
      if (pendingLines.size > 0) {
        if (lineFlushTimer) { clearTimeout(lineFlushTimer); lineFlushTimer = null; }
        flushPendingLines();
      }
      nudgeRepaint();
    }

    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) flushAndRepaint();
    });
    window.addEventListener('focus', flushAndRepaint);

    // User interaction recovery — mousemove, mousedown, and keydown all
    // prove the user is at the window.  Throttled so we nudge at most
    // once per 500ms across all interaction types.
    let lastInteractionNudge = 0;
    function onInteraction() {
      const now = Date.now();
      if (now - lastInteractionNudge > 500) {
        lastInteractionNudge = now;
        nudgeRepaint();
      }
    }
    window.addEventListener('mousemove', onInteraction);
    window.addEventListener('mousedown', onInteraction);
    window.addEventListener('keydown', onInteraction);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown, true);
      window.removeEventListener('keyup', handleGlobalKeyup, true);
      window.removeEventListener('mousemove', onInteraction);
      window.removeEventListener('mousedown', onInteraction);
      window.removeEventListener('keydown', onInteraction);
      if (lineFlushTimer) clearTimeout(lineFlushTimer);
      sentinel.remove();
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
      <button class="update-link" onclick={() => BrowserOpenURL(updateAvailable.url)}>View release</button>
      <button class="update-dismiss" onclick={() => updateAvailable = null}>✕</button>
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
