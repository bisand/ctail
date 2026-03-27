import { writable, derived, get } from 'svelte/store';
import { settings } from './settings.js';

// Immutable helper: replace a tab by id, producing new state/array/object refs
// so Svelte 5 $derived() detects the change via Object.is().
function replaceTab(state, tabId, changes) {
  return {
    ...state,
    tabs: state.tabs.map(t => t.id === tabId ? { ...t, ...changes } : t)
  };
}

function createTabStore() {
  const { subscribe, set, update } = writable({
    tabs: [],
    activeTabId: null
  });

  let currentState = { tabs: [], activeTabId: null };
  subscribe(s => { currentState = s; });

  return {
    subscribe,
    getActiveTabId() {
      return currentState.activeTabId;
    },
    addTab(id, filePath, fileName, position) {
      update(state => {
        const newTab = {
          id,
          filePath,
          fileName,
          label: '',
          color: '',
          profile: 'Common Logs',
          lines: [],
          totalLines: 0,
          hasUpdate: false,
          autoScroll: true,
          paused: false,
          loadingLines: false,
          status: 'loading',
          errorMessage: ''
        };
        const tabs = [...state.tabs];
        if (position != null && position >= 0 && position <= tabs.length) {
          tabs.splice(position, 0, newTab);
        } else {
          tabs.push(newTab);
        }
        return { ...state, activeTabId: id, tabs };
      });
    },
    removeTab(id) {
      update(state => {
        const idx = state.tabs.findIndex(t => t.id === id);
        const newTabs = state.tabs.filter(t => t.id !== id);
        let newActiveId = state.activeTabId;
        if (state.activeTabId === id) {
          if (newTabs.length > 0) {
            const newIdx = Math.min(idx, newTabs.length - 1);
            newActiveId = newTabs[newIdx].id;
          } else {
            newActiveId = null;
          }
        }
        return { ...state, tabs: newTabs, activeTabId: newActiveId };
      });
    },
    setActive(id) {
      update(state => {
        if (state.activeTabId === id) return state;
        const tab = state.tabs.find(t => t.id === id);
        // Only clone the tabs array if we need to clear the update badge
        const tabs = tab && tab.hasUpdate
          ? state.tabs.map(t => t.id === id ? { ...t, hasUpdate: false } : t)
          : state.tabs;
        return { ...state, activeTabId: id, tabs };
      });
    },
    setLines(tabId, lines, totalLines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab) return state;
        const changes = { lines };
        if (totalLines !== undefined) changes.totalLines = totalLines;
        if (state.activeTabId !== tabId) changes.hasUpdate = true;
        return replaceTab(state, tabId, changes);
      });
    },
    appendLines(tabId, newLines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab || newLines.length === 0) return state;
        const changes = { totalLines: tab.totalLines + newLines.length };

        const combined = [...tab.lines, ...newLines];
        if (tab.autoScroll) {
          // Follow mode: evict old lines from the top to keep memory bounded
          changes.lines = combined.length > tab.lines.length
            ? combined.slice(newLines.length)
            : combined;
        } else {
          // Not following: append lines but never evict from top.
          // Hard cap at 3x bufferSize to prevent unbounded memory growth.
          const hardCap = get(settings).bufferSize * 3;
          changes.lines = combined.length > hardCap
            ? combined.slice(combined.length - hardCap)
            : combined;
        }

        if (state.activeTabId !== tabId) changes.hasUpdate = true;
        return replaceTab(state, tabId, changes);
      });
    },
    clearLines(tabId) {
      update(state => replaceTab(state, tabId, { lines: [] }));
    },
    markHasUpdate(tabId) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab || tab.hasUpdate || state.activeTabId === tabId) return state;
        return replaceTab(state, tabId, { hasUpdate: true });
      });
    },
    setProfile(tabId, profileName) {
      update(state => replaceTab(state, tabId, { profile: profileName }));
    },
    setLabel(tabId, label) {
      update(state => replaceTab(state, tabId, { label }));
    },
    setColor(tabId, color) {
      update(state => replaceTab(state, tabId, { color }));
    },
    setFilePath(tabId, filePath, fileName) {
      update(state => replaceTab(state, tabId, { filePath, fileName, lines: [], totalLines: 0, status: 'loading' }));
    },
    setLoadingLines(tabId, value) {
      update(state => replaceTab(state, tabId, { loadingLines: value }));
    },
    setTotalLines(tabId, total) {
      update(state => replaceTab(state, tabId, { totalLines: total }));
    },
    setStatus(tabId, status, errorMessage) {
      const changes = { status };
      if (errorMessage !== undefined) changes.errorMessage = errorMessage;
      update(state => replaceTab(state, tabId, changes));
    },
    prependLines(tabId, olderLines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab || olderLines.length === 0) return state;
        return replaceTab(state, tabId, { lines: [...olderLines, ...tab.lines] });
      });
    },
    toggleAutoScroll(tabId) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab) return state;
        return replaceTab(state, tabId, { autoScroll: !tab.autoScroll });
      });
    },
    setAutoScroll(tabId, value) {
      update(state => {
        const changes = { autoScroll: value };
        if (value) {
          // Re-enabling follow: trim buffer back to bounded size
          const tab = state.tabs.find(t => t.id === tabId);
          if (tab && tab.lines.length > get(settings).bufferSize) {
            changes.lines = tab.lines.slice(tab.lines.length - get(settings).bufferSize);
          }
        }
        return replaceTab(state, tabId, changes);
      });
    },
    moveTab(fromIndex, toIndex) {
      update(state => {
        if (fromIndex < 0 || fromIndex >= state.tabs.length) return state;
        if (toIndex < 0 || toIndex >= state.tabs.length) return state;
        const newTabs = [...state.tabs];
        const [tab] = newTabs.splice(fromIndex, 1);
        newTabs.splice(toIndex, 0, tab);
        return { ...state, tabs: newTabs };
      });
    },
    reset() {
      set({ tabs: [], activeTabId: null });
    }
  };
}

export const tabStore = createTabStore();

export const activeTab = derived(tabStore, $store => {
  return $store.tabs.find(t => t.id === $store.activeTabId) || null;
});

export const tabs = derived(tabStore, $store => $store.tabs);
export const activeTabId = derived(tabStore, $store => $store.activeTabId);
