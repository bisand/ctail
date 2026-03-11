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
          hasUpdate: false,
          autoScroll: true,
          paused: false
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
    setLines(tabId, lines) {
      update(state => {
        const tab = state.tabs.find(t => t.id === tabId);
        if (tab) {
          tab.lines = lines;
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
          tab.lines = [...tab.lines, ...newLines];
          // Keep buffer bounded (frontend side)
          if (tab.lines.length > 15000) {
            tab.lines = tab.lines.slice(tab.lines.length - 10000);
          }
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
