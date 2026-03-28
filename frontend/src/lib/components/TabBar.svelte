<script>
  import { tabs, activeTabId, tabStore } from '../stores/tabs.js';
  import { CloseTab, ReopenTab, GetTabs, RevealInFileManager, SetTabLabel, SetTabColor, SaveTabOrder, ChangeTabFilePath, RefreshTab } from '../../../wailsjs/go/main/App.js';

  let { onAddTab, onReopenTab } = $props();

  const TAB_COLORS = [
    '', '#ef4444', '#f97316', '#eab308', '#22c55e', '#06b6d4', '#3b82f6', '#8b5cf6', '#ec4899'
  ];

  function switchTab(id) {
    tabStore.setActive(id);
  }

  function closeTab(e, id) {
    e.stopPropagation();
    CloseTab(id);
    tabStore.removeTab(id);
  }

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
      // Persist new order
      const tabStates = $tabs.map((t, i) => ({
        filePath: t.filePath,
        profileId: t.profile || '',
        autoScroll: t.autoScroll || true,
        label: t.label || '',
        color: t.color || '',
        position: i,
      }));
      SaveTabOrder(tabStates).catch(() => {});
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

  function ctxReopenClosed() {
    closeCtxMenu();
    if (onReopenTab) onReopenTab();
  }

  function ctxCopyPath() {
    if (ctxTab) {
      navigator.clipboard.writeText(ctxTab.filePath);
    }
    closeCtxMenu();
  }

  function ctxRefresh() {
    if (ctxMenu.tabId) {
      RefreshTab(ctxMenu.tabId).catch(e => console.error('Refresh failed:', e));
    }
    closeCtxMenu();
  }

  function ctxReveal() {
    if (ctxTab) {
      RevealInFileManager(ctxTab.filePath);
    }
    closeCtxMenu();
  }

  // Rename tab
  let renameTabId = $state(null);
  let renameValue = $state('');

  function ctxRename() {
    if (ctxTab) {
      renameTabId = ctxTab.id;
      renameValue = ctxTab.label || ctxTab.fileName;
    }
    closeCtxMenu();
  }

  function commitRename() {
    if (renameTabId) {
      const label = renameValue.trim();
      const tab = $tabs.find(t => t.id === renameTabId);
      // Clear label if it matches the filename (revert to default)
      const finalLabel = (tab && label === tab.fileName) ? '' : label;
      tabStore.setLabel(renameTabId, finalLabel);
      SetTabLabel(renameTabId, finalLabel).catch(() => {});
    }
    renameTabId = null;
    renameValue = '';
  }

  function cancelRename() {
    renameTabId = null;
    renameValue = '';
  }

  // Color picker
  let colorPickerTabId = $state(null);
  let colorPickerPos = $state({ x: 0, y: 0 });

  function ctxSetColor() {
    if (ctxTab) {
      colorPickerTabId = ctxTab.id;
      colorPickerPos = { x: ctxMenu.x, y: ctxMenu.y };
    }
    closeCtxMenu();
  }

  function pickColor(color) {
    if (colorPickerTabId) {
      tabStore.setColor(colorPickerTabId, color);
      SetTabColor(colorPickerTabId, color).catch(() => {});
    }
    colorPickerTabId = null;
  }

  function closeColorPicker() {
    colorPickerTabId = null;
  }

  async function ctxChangeFile() {
    if (!ctxTab) { closeCtxMenu(); return; }
    const tabId = ctxTab.id;
    closeCtxMenu();
    try {
      const newPath = await ChangeTabFilePath(tabId);
      if (newPath) {
        const newFileName = newPath.split(/[/\\]/).pop();
        tabStore.setFilePath(tabId, newPath, newFileName);
      }
    } catch (e) {
      console.error('Failed to change file:', e);
    }
  }
  // Tooltip hover state
  let tooltipTab = $state(null);
  let tooltipPos = $state({ x: 0, y: 0 });
  let tooltipTimer = null;

  function showTooltip(e, tab) {
    clearTimeout(tooltipTimer);
    const rect = e.currentTarget.getBoundingClientRect();
    tooltipTimer = setTimeout(() => {
      tooltipPos = { x: rect.left, y: rect.bottom + 4 };
      tooltipTab = tab;
    }, 400);
  }

  function hideTooltip() {
    clearTimeout(tooltipTimer);
    tooltipTab = null;
  }
</script>

<svelte:window onclick={(e) => { closeCtxMenu(); closeColorPicker(); }} />

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
        ondblclick={() => { renameTabId = tab.id; renameValue = tab.label || tab.fileName; }}
        onmouseenter={(e) => showTooltip(e, tab)}
        onmouseleave={hideTooltip}
      >
        {#if tab.color}
          <span class="tab-color" style="background: {tab.color}"></span>
        {/if}
        {#if tab.status === 'loading'}
          <span class="tab-spinner"></span>
        {:else if tab.status === 'error'}
          <span class="tab-error-icon" title={tab.errorMessage}>⚠</span>
        {/if}
        {#if renameTabId === tab.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="tab-rename-input"
            type="text"
            bind:value={renameValue}
            autofocus
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') cancelRename(); e.stopPropagation(); }}
            onblur={commitRename}
          />
        {:else}
          <span class="tab-name">{tab.label || tab.fileName}</span>
        {/if}
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
      <button class="ctx-item" onclick={ctxRename}>
        Rename
      </button>
      <button class="ctx-item" onclick={ctxSetColor}>
        Set color
      </button>
      <div class="ctx-separator"></div>
      <button class="ctx-item" onclick={ctxClose}>
        Close <span class="ctx-key">Ctrl+W</span>
      </button>
      <button class="ctx-item" onclick={ctxCloseOthers} disabled={$tabs.length < 2}>
        Close others
      </button>
      <button class="ctx-item" onclick={ctxCloseToRight} disabled={ctxMenu.tabIndex >= $tabs.length - 1}>
        Close to the right
      </button>
      <button class="ctx-item" onclick={ctxReopenClosed}>
        Reopen closed tab <span class="ctx-key">Ctrl+Shift+T</span>
      </button>
      <div class="ctx-separator"></div>
      <button class="ctx-item" onclick={ctxRefresh}>
        Refresh
      </button>
      <button class="ctx-item" onclick={ctxChangeFile}>
        Change file path…
      </button>
      <button class="ctx-item" onclick={ctxCopyPath}>
        Copy file path
      </button>
      <button class="ctx-item" onclick={ctxReveal}>
        Reveal in file manager
      </button>
    </div>
  {/if}

  {#if colorPickerTabId}
    <div class="color-picker" style="left: {colorPickerPos.x}px; top: {colorPickerPos.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      {#each TAB_COLORS as c}
        <button
          class="color-swatch"
          class:active={($tabs.find(t => t.id === colorPickerTabId)?.color || '') === c}
          style={c ? `background: ${c}` : ''}
          title={c || 'No color'}
          onclick={() => pickColor(c)}
        >
          {#if !c}✕{/if}
        </button>
      {/each}
    </div>
  {/if}

  {#if tooltipTab}
    <div class="tab-tooltip" style="left: {tooltipPos.x}px; top: {tooltipPos.y}px">
      <div class="tab-tooltip-content">
        {#if tooltipTab.color}
          <span class="tab-tooltip-color" style="background: {tooltipTab.color}"></span>
        {/if}
        <div class="tab-tooltip-text">
          {#if tooltipTab.label}
            <span class="tab-tooltip-label">{tooltipTab.label}</span>
          {/if}
          <span class="tab-tooltip-path">{tooltipTab.filePath}</span>
          {#if tooltipTab.status === 'error'}
            <span class="tab-tooltip-error">⚠ {tooltipTab.errorMessage}</span>
          {/if}
        </div>
      </div>
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

  .tab-color {
    width: 4px;
    height: 16px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .tab-rename-input {
    flex: 1;
    min-width: 60px;
    max-width: 160px;
    font-size: 12px;
    padding: 1px 4px;
    border: 1px solid var(--accent);
    border-radius: 3px;
    background: var(--bg-primary);
    color: var(--text-primary);
    outline: none;
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

  .color-picker {
    position: fixed;
    z-index: 1001;
    background: var(--bg-surface, var(--bg-secondary));
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px;
    display: flex;
    gap: 4px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  .color-swatch {
    width: 22px;
    height: 22px;
    border-radius: 4px;
    border: 2px solid transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-primary);
  }

  .color-swatch:hover {
    border-color: var(--text-primary);
  }

  .color-swatch.active {
    border-color: var(--accent);
  }

  .tab-tooltip {
    position: fixed;
    z-index: 1100;
    pointer-events: none;
    animation: tooltip-fade 0.15s ease-out;
  }

  @keyframes tooltip-fade {
    from { opacity: 0; transform: translateY(-2px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .tab-tooltip-content {
    display: flex;
    align-items: stretch;
    background: rgba(30, 30, 46, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    overflow: hidden;
    max-width: 400px;
    backdrop-filter: blur(8px);
  }

  .tab-tooltip-color {
    width: 4px;
    flex-shrink: 0;
  }

  .tab-tooltip-text {
    display: flex;
    flex-direction: column;
    padding: 6px 10px;
    gap: 2px;
  }

  .tab-tooltip-label {
    font-size: 12px;
    font-weight: 600;
    color: #cdd6f4;
  }

  .tab-tooltip-path {
    font-size: 11px;
    color: #a6adc8;
    word-break: break-all;
  }

  .tab-tooltip-error {
    font-size: 11px;
    color: var(--error, #ef4444);
  }
</style>
