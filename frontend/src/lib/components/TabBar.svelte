<script>
  import { tabs, activeTabId, tabStore } from '../stores/tabs.js';
  import { CloseTab } from '../../../wailsjs/go/main/App.js';

  function switchTab(id) {
    tabStore.setActive(id);
  }

  function closeTab(e, id) {
    e.stopPropagation();
    CloseTab(id);
    tabStore.removeTab(id);
  }

  export let onAddTab;
</script>

<div class="tab-bar">
  <div class="tabs-scroll">
    {#each $tabs as tab (tab.id)}
      <div
        class="tab"
        class:active={$activeTabId === tab.id}
        class:loading={tab.status === 'loading'}
        class:error={tab.status === 'error'}
        on:click={() => switchTab(tab.id)}
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
        <button class="close-btn" on:click={(e) => closeTab(e, tab.id)} title="Close tab">×</button>
      </div>
    {/each}
  </div>
  <button class="add-tab-btn" on:click={onAddTab} title="Open file">+</button>
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
    --wails-draggable: no-drag;
  }

  .tab:hover {
    background: var(--bg-hover);
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
</style>
