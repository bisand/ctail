import { writable, derived } from 'svelte/store';

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
    addTab(id, filePath, fileName) {
      update(state => ({
        ...state,
        activeTabId: id,
        tabs: [...state.tabs, {
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
        }]
      }));
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
        const newState = { ...state, activeTabId: id };
        newState.tabs = state.tabs.map(t =>
          t.id === id ? { ...t, hasUpdate: false } : t
        );
        return newState;
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

        if (tab.autoScroll) {
          changes.lines = [...tab.lines, ...newLines];
        }

        if (state.activeTabId !== tabId) changes.hasUpdate = true;
        return replaceTab(state, tabId, changes);
      });
    },
    clearLines(tabId) {
      update(state => replaceTab(state, tabId, { lines: [] }));
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
      update(state => replaceTab(state, tabId, { autoScroll: value }));
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
