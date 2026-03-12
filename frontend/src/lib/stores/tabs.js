import { writable, derived } from 'svelte/store';

function createTabStore() {
  const { subscribe, set, update } = writable({
    tabs: [],
    activeTabId: null
  });

  return {
    subscribe,
    addTab(id, filePath, fileName) {
      update(state => {
        state.tabs.push({
          id,
          filePath,
          fileName,
          profile: 'Common Logs',
          lines: [],
          totalLines: 0,
          hasUpdate: false,
          autoScroll: true,
          paused: false,
          loadingLines: false,
          status: 'loading', // 'loading' | 'ready' | 'error'
          errorMessage: ''
        });
        state.activeTabId = id;
        return state;
      });
    },
    removeTab(id) {
      update(state => {
        const idx = state.tabs.findIndex(t => t.id === id);
        state.tabs = state.tabs.filter(t => t.id !== id);
        if (state.activeTabId === id) {
          if (state.tabs.length > 0) {
            const newIdx = Math.min(idx, state.tabs.length - 1);
            state.activeTabId = state.tabs[newIdx].id;
          } else {
            state.activeTabId = null;
          }
        }
        return state;
      });
    },
    setActive(id) {
      update(state => {
        state.activeTabId = id;
        const tab = state.tabs.find(t => t.id === id);
        if (tab) tab.hasUpdate = false;
        return state;
      });
    },
    setLines(tabId, lines, totalLines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) {
          tab.lines = lines;
          if (totalLines !== undefined) tab.totalLines = totalLines;
          if (state.activeTabId !== tabId) {
            tab.hasUpdate = true;
          }
        }
        return state;
      });
    },
    appendLines(tabId, newLines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) {
          tab.totalLines += newLines.length;

          if (tab.autoScroll) {
            // Following: append lines and trim the top to keep window bounded
            tab.lines = [...tab.lines, ...newLines];
            const maxWindow = 500;
            if (tab.lines.length > maxWindow) {
              tab.lines = tab.lines.slice(tab.lines.length - maxWindow);
            }
          }
          // Not following: only totalLines is updated (status bar shows new count)

          if (state.activeTabId !== tabId) {
            tab.hasUpdate = true;
          }
        }
        return state;
      });
    },
    clearLines(tabId) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) {
          tab.lines = [];
        }
        return state;
      });
    },
    setProfile(tabId, profileName) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) tab.profile = profileName;
        return state;
      });
    },
    setLoadingLines(tabId, value) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) tab.loadingLines = value;
        return state;
      });
    },
    setTotalLines(tabId, total) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) tab.totalLines = total;
        return state;
      });
    },
    setStatus(tabId, status, errorMessage) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) {
          tab.status = status;
          if (errorMessage !== undefined) tab.errorMessage = errorMessage;
        }
        return state;
      });
    },
    prependLines(tabId, olderLines, maxWindow) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab && olderLines.length > 0) {
          tab.lines = [...olderLines, ...tab.lines];
          // Trim from end if exceeding window
          if (tab.lines.length > maxWindow) {
            tab.lines = tab.lines.slice(0, maxWindow);
          }
        }
        return state;
      });
    },
    appendRangeLines(tabId, newerLines, maxWindow) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab && newerLines.length > 0) {
          tab.lines = [...tab.lines, ...newerLines];
          // Trim from start if exceeding window
          if (tab.lines.length > maxWindow) {
            tab.lines = tab.lines.slice(tab.lines.length - maxWindow);
          }
        }
        return state;
      });
    },
    toggleAutoScroll(tabId) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) tab.autoScroll = !tab.autoScroll;
        return state;
      });
    },
    setAutoScroll(tabId, value) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) tab.autoScroll = value;
        return state;
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
