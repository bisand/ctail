<script>
  import { tabs, activeTabId, tabStore } from '../stores/tabs.js';
  import { CloseTab, RevealInFileManager } from '../../../wailsjs/go/main/App.js';

  function switchTab(id) {
    tabStore.setActive(id);
  }

  function closeTab(e, id) {
    e.stopPropagation();
    CloseTab(id);
    tabStore.removeTab(id);
  }

  let { onAddTab } = $props();

  // Drag and drop reordering
  let dragIndex = $state(-1);
  let dropIndex = $state(-1);

  function handleDragStart(e, index) {
    dragIndex = index;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(index));
    e.currentTarget.classList.add('dragging');
  }

  function handleDragOver(e, index) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (index !== dragIndex) {
      dropIndex = index;
    }
  }

  function handleDragLeave() {
    dropIndex = -1;
  }

  function handleDrop(e, index) {
    e.preventDefault();
    if (dragIndex >= 0 && dragIndex !== index) {
      tabStore.moveTab(dragIndex, index);
    }
    dragIndex = -1;
    dropIndex = -1;
  }

  function handleDragEnd(e) {
    e.currentTarget.classList.remove('dragging');
    dragIndex = -1;
    dropIndex = -1;
  }

  // Tab context menu
  let ctxMenu = $state({ visible: false, x: 0, y: 0, tabId: null, tabIndex: -1 });

  function handleTabContext(e, tab, index) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { visible: true, x: e.clientX, y: e.clientY, tabId: tab.id, tabIndex: index };
  }

  function closeCtxMenu() {
    ctxMenu = { ...ctxMenu, visible: false };
  }

  let ctxTab = $derived(ctxMenu.tabId ? $tabs.find(t => t.id === ctxMenu.tabId) : null);

  function ctxClose() {
    if (ctxMenu.tabId) {
      CloseTab(ctxMenu.tabId);
      tabStore.removeTab(ctxMenu.tabId);
    }
    closeCtxMenu();
  }

  function ctxCloseOthers() {
    const keepId = ctxMenu.tabId;
    const toClose = $tabs.filter(t => t.id !== keepId);
    if (toClose.length === 0) { closeCtxMenu(); return; }
    if (!confirm(`Close ${toClose.length} other tab${toClose.length > 1 ? 's' : ''}?`)) { closeCtxMenu(); return; }
    for (const t of toClose) {
      CloseTab(t.id);
      tabStore.removeTab(t.id);
    }
    closeCtxMenu();
  }

  function ctxCloseToRight() {
    const idx = ctxMenu.tabIndex;
    const toClose = $tabs.slice(idx + 1);
    if (toClose.length === 0) { closeCtxMenu(); return; }
    if (!confirm(`Close ${toClose.length} tab${toClose.length > 1 ? 's' : ''} to the right?`)) { closeCtxMenu(); return; }
    for (const t of toClose) {
      CloseTab(t.id);
      tabStore.removeTab(t.id);
    }
    closeCtxMenu();
  }

  function ctxCopyPath() {
    if (ctxTab) {
      navigator.clipboard.writeText(ctxTab.filePath);
    }
    closeCtxMenu();
  }

  function ctxReveal() {
    if (ctxTab) {
      RevealInFileManager(ctxTab.filePath);
    }
    closeCtxMenu();
  }
</script>

<svelte:window onclick={closeCtxMenu} />

<div class="tab-bar">
  <div class="tabs-scroll">
    {#each $tabs as tab, i (tab.id)}
      <div
        class="tab"
        class:active={$activeTabId === tab.id}
        class:loading={tab.status === 'loading'}
        class:error={tab.status === 'error'}
        class:drop-before={dropIndex === i && dragIndex > i}
        class:drop-after={dropIndex === i && dragIndex < i}
        draggable="true"
        role="tab"
        tabindex="0"
        onclick={() => switchTab(tab.id)}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') switchTab(tab.id); }}
        ondragstart={(e) => handleDragStart(e, i)}
        ondragover={(e) => handleDragOver(e, i)}
        ondragleave={handleDragLeave}
        ondrop={(e) => handleDrop(e, i)}
        ondragend={handleDragEnd}
        oncontextmenu={(e) => handleTabContext(e, tab, i)}
        title={tab.status === 'error' ? `${tab.filePath}\n⚠ ${tab.errorMessage}` : tab.filePath}
      >
        {#if tab.status === 'loading'}
          <span class="tab-spinner"></span>
        {:else if tab.status === 'error'}
          <span class="tab-error-icon" title={tab.errorMessage}>⚠</span>
        {/if}
        <span class="tab-name">{tab.fileName}</span>
        {#if tab.hasUpdate}
          <span class="badge"></span>
        {/if}
        <button class="close-btn" onclick={(e) => closeTab(e, tab.id)} title="Close tab">×</button>
      </div>
    {/each}
  </div>
  <button class="add-tab-btn" onclick={onAddTab} title="Open file">+</button>

  {#if ctxMenu.visible}
    <div class="tab-ctx-menu" style="left: {ctxMenu.x}px; top: {ctxMenu.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <button class="ctx-item" onclick={ctxClose}>
        Close <span class="ctx-key">Ctrl+W</span>
      </button>
      <button class="ctx-item" onclick={ctxCloseOthers} disabled={$tabs.length < 2}>
        Close others
      </button>
      <button class="ctx-item" onclick={ctxCloseToRight} disabled={ctxMenu.tabIndex >= $tabs.length - 1}>
        Close to the right
      </button>
      <div class="ctx-separator"></div>
      <button class="ctx-item" onclick={ctxCopyPath}>
        Copy file path
      </button>
      <button class="ctx-item" onclick={ctxReveal}>
        Reveal in file manager
      </button>
    </div>
  {/if}
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    min-height: 34px;
    user-select: none;
    --wails-draggable: drag;
  }

  .tabs-scroll {
    display: flex;
    flex: 1;
    overflow-x: auto;
    overflow-y: hidden;
  }

  .tabs-scroll::-webkit-scrollbar {
    height: 0;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    min-width: 120px;
    max-width: 200px;
    background: var(--tab-inactive);
    border-right: 1px solid var(--border);
    cursor: pointer;
    position: relative;
    white-space: nowrap;
    transition: opacity 0.15s;
    --wails-draggable: no-drag;
  }

  .tab:hover {
    background: var(--bg-hover);
  }

  .tab.drop-before {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .tab.drop-after {
    box-shadow: inset -3px 0 0 var(--accent);
  }

  .tab.active {
    background: var(--tab-active);
    border-bottom: 2px solid var(--accent);
  }

  .tab-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
  }

  .tab-spinner {
    width: 10px;
    height: 10px;
    border: 2px solid var(--text-muted);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .tab-error-icon {
    color: var(--warning, #e5c07b);
    font-size: 12px;
    flex-shrink: 0;
  }

  .tab.error .tab-name {
    opacity: 0.6;
  }

  .badge {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--badge-color);
    flex-shrink: 0;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .close-btn {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    font-size: 14px;
    color: var(--text-muted);
    flex-shrink: 0;
    --wails-draggable: no-drag;
  }

  .close-btn:hover {
    background: var(--danger);
    color: white;
  }

  .add-tab-btn {
    width: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    color: var(--text-muted);
    border-left: 1px solid var(--border);
    --wails-draggable: no-drag;
  }

  .add-tab-btn:hover {
    background: var(--bg-hover);
    color: var(--accent);
  }

  .tab-ctx-menu {
    position: fixed;
    z-index: 1000;
    background: var(--bg-surface, var(--bg-secondary));
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 0;
    min-width: 180px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
    user-select: none;
  }

  .ctx-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 6px 12px;
    font-size: 12px;
    color: var(--text-primary);
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    gap: 8px;
  }

  .ctx-item:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .ctx-item:disabled {
    color: var(--text-muted);
    cursor: default;
    opacity: 0.5;
  }

  .ctx-key {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-muted);
  }

  .ctx-separator {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }
</style>
