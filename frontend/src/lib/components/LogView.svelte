<script>
  import { onMount, afterUpdate, tick } from 'svelte';
  import LogLine from './LogLine.svelte';
  import { activeTab, tabStore } from '../stores/tabs.js';
  import { settings } from '../stores/settings.js';
  import { profiles } from '../stores/rules.js';

  let container;
  let isAtBottom = true;
  let searchQuery = '';
  let searchVisible = false;

  $: currentTab = $activeTab;
  $: lines = currentTab ? currentTab.lines : [];
  $: profileName = currentTab ? currentTab.profile : 'Common Logs';
  $: profile = $profiles[profileName];
  $: rules = profile ? profile.rules : [];
  $: autoScroll = currentTab ? currentTab.autoScroll : true;

  afterUpdate(() => {
    if (autoScroll && container) {
      container.scrollTop = container.scrollHeight;
    }
  });

  function handleScroll() {
    if (!container) return;
    const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 30;
    if (isAtBottom && !atBottom && currentTab) {
      tabStore.setAutoScroll(currentTab.id, false);
    }
    isAtBottom = atBottom;
  }

  function scrollToBottom() {
    if (currentTab) {
      tabStore.setAutoScroll(currentTab.id, true);
    }
    if (container) {
      container.scrollTop = container.scrollHeight;
    }
  }

  function handleKeydown(e) {
    if (e.ctrlKey && e.key === 'f') {
      e.preventDefault();
      searchVisible = !searchVisible;
      if (!searchVisible) searchQuery = '';
    }
    if (e.key === 'Escape' && searchVisible) {
      searchVisible = false;
      searchQuery = '';
    }
  }

  $: filteredLines = searchQuery
    ? lines.filter(l => l.text.toLowerCase().includes(searchQuery.toLowerCase()))
    : lines;
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="log-view" data-wordwrap={$settings.wordWrap}>
  {#if searchVisible}
    <div class="search-bar">
      <input
        type="text"
        placeholder="Search..."
        bind:value={searchQuery}
        class="search-input"
      />
      <span class="search-count">
        {#if searchQuery}
          {filteredLines.length} / {lines.length} lines
        {/if}
      </span>
      <button class="search-close" on:click={() => { searchVisible = false; searchQuery = ''; }}>×</button>
    </div>
  {/if}

  {#if currentTab}
    <div class="log-container" bind:this={container} on:scroll={handleScroll}>
      {#each filteredLines as line (line.number)}
        <LogLine
          {line}
          {rules}
          showLineNumber={$settings.showLineNumbers}
          fontSize={$settings.fontSize}
        />
      {/each}
      {#if lines.length === 0}
        <div class="empty-state">
          <p>Waiting for data...</p>
          <p class="muted">{currentTab.filePath}</p>
        </div>
      {/if}
    </div>

    {#if !autoScroll}
      <button class="scroll-bottom-btn" on:click={scrollToBottom} title="Scroll to bottom (auto-scroll)">
        ↓ Auto-scroll
      </button>
    {/if}
  {:else}
    <div class="empty-state centered">
      <p class="big">ctail</p>
      <p class="muted">Open a file to start tailing</p>
      <p class="hint">Press Ctrl+O or click the + button</p>
    </div>
  {/if}
</div>

<style>
  .log-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    position: relative;
    overflow: hidden;
  }

  .log-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: auto;
    padding: 4px 0;
  }

  .empty-state {
    padding: 40px;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-state.centered {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
  }

  .empty-state .big {
    font-size: 48px;
    font-weight: 700;
    color: var(--text-secondary);
    margin-bottom: 12px;
  }

  .empty-state .muted {
    color: var(--text-muted);
    font-size: 14px;
    margin-top: 4px;
  }

  .empty-state .hint {
    color: var(--text-muted);
    font-size: 12px;
    margin-top: 8px;
  }

  .scroll-bottom-btn {
    position: absolute;
    bottom: 16px;
    right: 24px;
    background: var(--accent);
    color: var(--bg-primary);
    padding: 6px 14px;
    border-radius: 16px;
    font-weight: 600;
    font-size: 12px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.3);
    z-index: 10;
  }

  .scroll-bottom-btn:hover {
    background: var(--accent-hover);
  }

  .search-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
  }

  .search-input {
    flex: 1;
    max-width: 300px;
  }

  .search-count {
    font-size: 12px;
    color: var(--text-muted);
  }

  .search-close {
    font-size: 16px;
    color: var(--text-muted);
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
  }

  .search-close:hover {
    background: var(--bg-hover);
  }
</style>
