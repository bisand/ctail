<script>
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import LogView from './lib/components/LogView.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import AboutDialog from './lib/components/AboutDialog.svelte';
  import AIDialog from './lib/components/AIDialog.svelte';
  import UpdateDialog from './lib/components/UpdateDialog.svelte';
  import { tabStore, activeTab, tabs, pendingInitLoads } from './lib/stores/tabs.js';
  import { settings, settingsPanelOpen } from './lib/stores/settings.js';
  import { profiles } from './lib/stores/rules.js';
  import { OpenFileDialog, OpenTab, CloseTab, ReopenTab, RefreshTab, GetTabLineRange, GetTabTotalLines, GetSettings, GetSavedTabs, GetPendingFiles, SaveSettings, ListProfiles, GetProfile, ListThemes, ManualCheckForUpdates, SetEventsPaused, SetActiveTab, FixMaximize } from '../wailsjs/go/main/App.js';
  import { EventsOn, BrowserOpenURL, ScreenGetAll } from '../wailsjs/runtime/runtime.js';
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

  // Notify Go when the active tab changes so it only sends full line data
  // for the active tab (inactive tabs get lightweight activity notifications).
  // Also reload lines from Go if the tab had pending updates while inactive.
  //
  // pendingReload tracks tabs that need a content reload when next activated.
  // We can't rely on tab.hasUpdate here because setActive() clears hasUpdate
  // atomically — by the time this effect fires, hasUpdate is already false.
  const pendingReload = new Set();
  let lastNotifiedTabId = null;
  $effect(() => {
    const tab = $activeTab;
    if (!tab || tab.id === lastNotifiedTabId) return;
    lastNotifiedTabId = tab.id;
    SetActiveTab(tab.id).catch(() => {});
    if (pendingReload.has(tab.id)) {
      pendingReload.delete(tab.id);
      loadInitialLines(tab.id);
    }
  });

  // Load initial lines for a tab after it becomes ready.
  // Guards against concurrent calls for the same tab (e.g. rapid ready→error→ready cycling).
  // fromReadyEvent: true when called from tailer:ready or tailer:indexed — the tailer is
  // definitely done, so we mark the tab ready even if the file is empty.  When false
  // (called after a file path change), we skip setting ready if the tailer hasn't finished
  // yet (total === 0), leaving the tab in 'loading' state so tailer:ready finishes the job.
  //
  // pendingReadyTabs: when a tailer:ready arrives while an earlier non-ready call is still
  // awaiting GetTabTotalLines (which blocks the guard), we record it here so the finally
  // block can retry — BUT only if the tab is still loading (if the first call already
  // succeeded, the retry would cause a double-load and scroll-to-top artifact).
  const pendingReadyTabs = new Set();
  // Tracks tailer:ready events that arrived before addTab created the tab in
  // the store.  After addTab, we check this and replay the missed event.
  const missedReadyTabs = new Map();

  async function loadInitialLines(tabId, fromReadyEvent = false) {
    if (pendingInitLoads.has(tabId)) {
      // Another call is in-flight for this tab.  If this one came from tailer:ready we
      // must not drop it — record so the in-flight call retries when it finishes.
      if (fromReadyEvent) pendingReadyTabs.add(tabId);
      return;
    }
    pendingInitLoads.add(tabId);
    try {
      const total = await GetTabTotalLines(tabId);
      const fetchStart = Math.max(1, total - INITIAL_TAIL + 1);
      const lines = await GetTabLineRange(tabId, fetchStart, INITIAL_TAIL);
      if (lines && lines.length > 0) {
        tabStore.setLines(tabId, lines, total);
        tabStore.setStatus(tabId, 'ready');
      } else if (fromReadyEvent || total > 0) {
        // fromReadyEvent: tailer is done (empty file is still 'ready')
        // total > 0: file has lines but ReadRange returned nil (tail-first indexing in progress)
        tabStore.setStatus(tabId, 'ready');
      }
      // else: total === 0 and not from ready event — tailer not done yet, stay in 'loading'
    } catch (e) {
      console.error('Failed to load initial lines for', tabId, e);
      tabStore.setStatus(tabId, 'error', String(e));
    } finally {
      pendingInitLoads.delete(tabId);
      // Only retry if tailer:ready arrived while blocked AND the first call didn't already
      // succeed (i.e. tab is still loading).  A successful first call already loaded lines
      // correctly; retrying would replace the buffer when scrollTop may still be 0 and
      // the auto-scroll tick hasn't fired yet, causing a visible scroll-to-top.
      if (pendingReadyTabs.delete(tabId)) {
        const tab = $tabs.find(t => t.id === tabId);
        if (tab && tab.status !== 'ready') {
          loadInitialLines(tabId, true);
        }
      }
    }
  }

  // Register a new tab in the store and kick off initial line loading.
  // Handles the race where tailer:ready fires before addTab by checking
  // missedReadyTabs.  addTab is idempotent (returns early for existing IDs).
  function registerAndLoad(tabId, filePath, fileName, position) {
    tabStore.addTab(tabId, filePath, fileName, position);
    // If the tab already existed (addTab was a no-op), skip initial load.
    const tab = $tabs.find(t => t.id === tabId);
    if (tab && tab.status !== 'loading') return;
    // Check for a tailer:ready that arrived before the tab was in the store.
    const missed = missedReadyTabs.get(tabId);
    if (missed) {
      missedReadyTabs.delete(tabId);
      if (!missed.indexingComplete) {
        tabStore.setIsIndexing(tabId, true);
      }
      loadInitialLines(tabId, true);
    } else {
      loadInitialLines(tabId);
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
    const MAX_PENDING_PER_TAB = 5000;

    function flushPendingLines() {
      lineFlushTimer = null;
      for (const [tabId, lines] of pendingLines) {
        // Skip tabs with an in-flight loadInitialLines — the snapshot will
        // replace these lines anyway, and appending them first creates gaps.
        if (!pendingInitLoads.has(tabId)) {
          tabStore.appendLines(tabId, lines);
        }
      }
      pendingLines.clear();
    }

    // Notify Go backend when window visibility changes so it can skip
    // EventsEmit calls while the frontend can't process them.
    document.addEventListener('visibilitychange', () => {
      SetEventsPaused(document.hidden).catch(() => {});
      if (!document.hidden) {
        // Window regained focus — flush any pending lines
        if (lineFlushTimer) {
          clearTimeout(lineFlushTimer);
        }
        flushPendingLines();
      }
    });

    EventsOn('tailer:lines', (data) => {
      if (data.tabId && data.lines) {
        // While loadInitialLines is in-flight for this tab, discard streaming
        // lines.  They would be overwritten by the upcoming setLines snapshot
        // anyway, and appending them first causes gaps (snapshot is stale by
        // the time it's applied → next streaming batch skips the gap).
        if (pendingInitLoads.has(data.tabId)) return;

        const existing = pendingLines.get(data.tabId) || [];
        existing.push(...data.lines);
        // Always cap the pending buffer to prevent unbounded growth
        if (existing.length > MAX_PENDING_PER_TAB) {
          existing.splice(0, existing.length - MAX_PENDING_PER_TAB);
        }
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
        const tab = $tabs.find(t => t.id === data.tabId);
        if (!tab) {
          // Tab not yet in store (tailer:ready arrived before addTab).
          // Record so the post-addTab loadInitialLines handles it.
          missedReadyTabs.set(data.tabId, data);
          return;
        }
        if (!data.indexingComplete) {
          tabStore.setIsIndexing(data.tabId, true);
        }
        // Skip reload if the explicit loadInitialLines after addTab already
        // succeeded — a second setLines would trigger a DOM reflow that can
        // spuriously disable autoScroll after pendingInitLoads is cleared.
        if (tab.status === 'ready' && tab.lines.length > 0) return;
        loadInitialLines(data.tabId, true);
      }
    });

    EventsOn('tailer:reconnecting', (data) => {
      if (data.tabId) {
        tabStore.setStatus(data.tabId, 'loading');
      }
    });

    EventsOn('tailer:indexed', (data) => {
      if (data.tabId) {
        tabStore.setIsIndexing(data.tabId, false);
        if (data.totalLines) tabStore.setTotalLines(data.tabId, data.totalLines);
        // Reload lines to get correct line numbers, but only if the user is still
        // following (autoScroll). If they've scrolled up, don't jump their position.
        const tab = $tabs.find(t => t.id === data.tabId);
        if (tab && tab.autoScroll) {
          loadInitialLines(data.tabId, true);
        }
      }
    });

    // Lightweight notification: inactive tabs set the update badge.
    // Also fired on resume for tabs that had lines while the window was hidden —
    // including the active tab, which needs a content reload (not just a badge).
    EventsOn('tailer:activity', (data) => {
      if (data.tabId) {
        if (data.tabId === tabStore.getActiveTabId()) {
          // Active tab was stale while hidden — reload content from Go.
          loadInitialLines(data.tabId);
        } else {
          tabStore.markHasUpdate(data.tabId);
          // Remember to reload content when this tab is next activated.
          // (tab.hasUpdate is cleared by setActive before the $effect runs)
          pendingReload.add(data.tabId);
        }
      }
    });

    EventsOn('tab:focus', (tabId) => {
      if (tabId) {
        tabStore.setActive(tabId);
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
    EventsOn('file:open-external', async (...args) => {
      const filePath = args[0];
      if (!filePath || typeof filePath !== 'string') return;
      try {
        const fileName = filePath.split(/[/\\]/).pop();
        const tabId = await OpenTab(filePath);
        registerAndLoad(tabId, filePath, fileName);
      } catch (e) {
        console.warn('Failed to open external file:', filePath, e);
      }
    });

    EventsOn('menu:close-tab', () => {
      const tab = $activeTab;
      if (tab) {
        CloseTab(tab.id);
        tabStore.removeTab(tab.id);
        pendingReload.delete(tab.id);
      }
    });

    EventsOn('menu:reopen-tab', () => {
      reopenClosedTab();
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

    // Restore saved tabs concurrently — each inserts at its saved position
    // as soon as its promise resolves, so the UI populates progressively
    // while maintaining the correct order.
    try {
      const savedTabs = await GetSavedTabs();
      const pending = await GetPendingFiles();

      if (savedTabs && savedTabs.length > 0) {
        const sorted = [...savedTabs].sort((a, b) => (a.position || 0) - (b.position || 0));
        for (const tab of sorted) {
          try {
            const tabId = await OpenTab(tab.filePath);
            registerAndLoad(tabId, tab.filePath, tab.filePath.split(/[/\\]/).pop());
            if (tab.profileId) tabStore.setProfile(tabId, tab.profileId);
            if (tab.label) tabStore.setLabel(tabId, tab.label);
            if (tab.color) tabStore.setColor(tabId, tab.color);
          } catch (e) {
            console.warn('Failed to restore tab:', tab.filePath, e);
          }
        }
      }

      // CLI files always go at the end
      if (pending && pending.length > 0) {
        for (const filePath of pending) {
          try {
            const tabId = await OpenTab(filePath);
            registerAndLoad(tabId, filePath, filePath.split(/[/\\]/).pop());
          } catch (e) {
            console.warn('Failed to open CLI file:', filePath, e);
          }
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

    // Heavy repaint for compositor surface corruption (e.g. monitor
    // hotplug). Briefly forces the body off-screen and back, which makes
    // WebKit2GTK tear down and recreate its GPU compositing surface.
    function forceCompositingReset() {
      const root = document.documentElement;
      root.style.display = 'none';
      void root.offsetHeight;
      root.style.display = '';
      void root.offsetHeight;
      nudgeRepaint();
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

    // Monitor hotplug / screen change detection.  When monitors are
    // added or removed, WebKit2GTK's compositor surface can become
    // corrupted (transparent background, content bleeding outside the
    // window).  We detect this via resize events and devicePixelRatio
    // changes, then force a full compositing reset.
    let lastDPR = window.devicePixelRatio;
    let lastScreenW = window.screen.width;
    let lastScreenH = window.screen.height;
    let resetDebounce = null;

    function scheduleCompositingReset() {
      if (resetDebounce) clearTimeout(resetDebounce);
      resetDebounce = setTimeout(() => {
        resetDebounce = null;
        forceCompositingReset();
      }, 300);
    }

    // Detect screen configuration changes via resize event (window
    // managers often reposition/resize windows when monitors change).
    // Also detect maximize and correct wrong dimensions on Wayland.
    let maximizeFixTimer = null;
    function onWindowResize() {
      const dpr = window.devicePixelRatio;
      const sw = window.screen.width;
      const sh = window.screen.height;
      if (dpr !== lastDPR || sw !== lastScreenW || sh !== lastScreenH) {
        lastDPR = dpr;
        lastScreenW = sw;
        lastScreenH = sh;
        scheduleCompositingReset();
      }

      // On Wayland multi-monitor, maximize may use the wrong screen's dimensions.
      // Detect when the window fills the viewport (likely maximize) and ask Go to verify.
      if (maximizeFixTimer) clearTimeout(maximizeFixTimer);
      maximizeFixTimer = setTimeout(() => {
        maximizeFixTimer = null;
        const iw = window.innerWidth;
        const ih = window.innerHeight;
        // If window fills most of the screen, it was likely just maximized
        if (iw >= sw * 0.9 && ih >= sh * 0.85) {
          FixMaximize().catch(() => {});
        }
      }, 300);
    }
    window.addEventListener('resize', onWindowResize);

    // Also poll for DPR/screen geometry changes that may not trigger
    // a resize event (e.g. monitor plugged in without affecting the
    // current window).
    const screenPollId = setInterval(() => {
      const dpr = window.devicePixelRatio;
      const sw = window.screen.width;
      const sh = window.screen.height;
      if (dpr !== lastDPR || sw !== lastScreenW || sh !== lastScreenH) {
        lastDPR = dpr;
        lastScreenW = sw;
        lastScreenH = sh;
        scheduleCompositingReset();
      }
    }, 2000);

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
      window.removeEventListener('resize', onWindowResize);
      window.removeEventListener('mousemove', onInteraction);
      window.removeEventListener('mousedown', onInteraction);
      window.removeEventListener('keydown', onInteraction);
      if (lineFlushTimer) clearTimeout(lineFlushTimer);
      if (resetDebounce) clearTimeout(resetDebounce);
      clearInterval(screenPollId);
      sentinel.remove();
    };
  });

  async function reopenClosedTab() {
    try {
      const id = await ReopenTab();
      if (id) {
        // ReopenTab returns the file path as the second element when called from Go.
        // We need to get tab info to add it to the store. The Go side already created
        // the tab; we need the filePath to derive the fileName.
        // Since OpenTab already ran, the tailer:ready event will fire and loadInitialLines.
        // But we need to register the tab in the frontend store first.
        // ReopenTab → OpenTab → returns id. We need the filePath.
        // Let's use GetTabs to find the new tab's info.
        const { GetTabs } = await import('../wailsjs/go/main/App.js');
        const allTabs = await GetTabs();
        const tab = allTabs.find(t => t.id === id);
        if (tab) {
          const fileName = tab.filePath.split(/[/\\]/).pop();
          registerAndLoad(id, tab.filePath, fileName);
          if (tab.label) tabStore.setLabel(id, tab.label);
          if (tab.color) tabStore.setColor(id, tab.color);
          if (tab.profile) tabStore.setProfile(id, tab.profile);
          tabStore.setActive(id);
        }
      }
    } catch (e) {
      console.warn('No closed tab to reopen:', e);
    }
  }

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
        CloseTab(tab.id);
        tabStore.removeTab(tab.id);
        pendingReload.delete(tab.id);
      }
    }
    if (e.ctrlKey && e.shiftKey && (e.key === 'T' || e.key === 't')) {
      e.preventDefault();
      reopenClosedTab();
    }
    if (e.ctrlKey && (e.key === 'r' || e.key === 'R') && !e.shiftKey) {
      e.preventDefault();
      const tab = $activeTab;
      if (tab) RefreshTab(tab.id).catch(e => console.error('Refresh failed:', e));
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
      registerAndLoad(tabId, path, fileName);
    } catch (e) {
      console.error('Failed to open file:', e);
    }
  }

  async function openRecentFile(filePath) {
    try {
      const tabId = await OpenTab(filePath);
      const fileName = filePath.split(/[/\\]/).pop();
      registerAndLoad(tabId, filePath, fileName);
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
  <TabBar onAddTab={openFile} onReopenTab={reopenClosedTab} />
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
