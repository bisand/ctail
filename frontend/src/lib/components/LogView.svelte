<script>
  import { onMount, afterUpdate, tick } from 'svelte';
  import LogLine from './LogLine.svelte';
  import { activeTab, tabStore } from '../stores/tabs.js';
  import { settings } from '../stores/settings.js';
  import { profiles } from '../stores/rules.js';
  import { GetTabLineRange, GetTabTotalLines } from '../../../wailsjs/go/main/App.js';

  let container;
  let isAtBottom = true;
  let searchQuery = '';
  let searchVisible = false;

  const FETCH_BATCH = 200;
  const MAX_WINDOW = 1000;
  let scrollCheckTimer = null;

  function scheduleScrollCheck() {
    if (scrollCheckTimer) clearTimeout(scrollCheckTimer);
    scrollCheckTimer = setTimeout(() => {
      scrollCheckTimer = null;
      if (container && currentTab && !currentTab.loadingLines) {
        handleScroll();
      }
    }, 50);
  }

  $: currentTab = $activeTab;
  $: lines = currentTab ? currentTab.lines : [];
  $: profileName = currentTab ? currentTab.profile : 'Common Logs';
  $: profile = $profiles[profileName];
  $: rules = profile ? profile.rules : [];
  $: autoScroll = currentTab ? currentTab.autoScroll : true;
  $: totalLines = currentTab ? currentTab.totalLines : 0;
  $: windowStart = lines.length > 0 ? lines[0].number : 0;
  $: windowEnd = lines.length > 0 ? lines[lines.length - 1].number : 0;
  $: canScrollBack = windowStart > 1;
  $: canScrollForward = !autoScroll && windowEnd < totalLines;
  $: tabStatus = currentTab ? currentTab.status : null;
  $: tabError = currentTab ? currentTab.errorMessage : '';

  afterUpdate(() => {
    if (autoScroll && container) {
      container.scrollTop = container.scrollHeight;
    }
  });

  async function loadEarlierLines() {
    if (!currentTab || currentTab.loadingLines || !canScrollBack) return;

    tabStore.setLoadingLines(currentTab.id, true);
    try {
      const fetchStart = Math.max(1, windowStart - FETCH_BATCH);
      const fetchCount = windowStart - fetchStart;
      if (fetchCount <= 0) return;

      const olderLines = await GetTabLineRange(currentTab.id, fetchStart, fetchCount);
      if (olderLines && olderLines.length > 0) {
        const prevScrollHeight = container.scrollHeight;
        const prevScrollTop = container.scrollTop;

        tabStore.prependLines(currentTab.id, olderLines, MAX_WINDOW);

        await tick();
        if (container) {
          const newScrollHeight = container.scrollHeight;
          container.scrollTop = prevScrollTop + (newScrollHeight - prevScrollHeight);
        }
      }
    } catch (e) {
      console.error('Failed to load earlier lines:', e);
    } finally {
      tabStore.setLoadingLines(currentTab.id, false);
      // Re-check: if still near the top, keep fetching
      scheduleScrollCheck();
    }
  }

  async function loadLaterLines() {
    if (!currentTab || currentTab.loadingLines || !canScrollForward) return;

    tabStore.setLoadingLines(currentTab.id, true);
    try {
      const fetchStart = windowEnd + 1;
      const newerLines = await GetTabLineRange(currentTab.id, fetchStart, FETCH_BATCH);
      if (newerLines && newerLines.length > 0) {
        tabStore.appendRangeLines(currentTab.id, newerLines, MAX_WINDOW);
      }
    } catch (e) {
      console.error('Failed to load later lines:', e);
    } finally {
      tabStore.setLoadingLines(currentTab.id, false);
      scheduleScrollCheck();
    }
  }

  function handleScroll() {
    if (!container || !currentTab) return;

    const scrollRatio = container.scrollTop / (container.scrollHeight - container.clientHeight || 1);
    const nearTop = scrollRatio < 0.15;
    const nearBottom = scrollRatio > 0.85;
    const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 30;

    if (isAtBottom && !atBottom) {
      tabStore.setAutoScroll(currentTab.id, false);
    }
    isAtBottom = atBottom;

    // Prefetch earlier lines when approaching the top
    if (nearTop && canScrollBack && !currentTab.loadingLines) {
      loadEarlierLines();
    }

    // Prefetch later lines when approaching the bottom (only when not in live-tail)
    if (nearBottom && canScrollForward && !currentTab.loadingLines) {
      loadLaterLines();
    }
  }

  function scrollToBottom() {
    if (currentTab) {
      tabStore.setAutoScroll(currentTab.id, true);
      // When re-enabling auto-scroll, jump to latest lines from the file
      jumpToLatest();
    }
  }

  async function jumpToLatest() {
    if (!currentTab) return;
    try {
      const total = await GetTabTotalLines(currentTab.id);
      tabStore.setTotalLines(currentTab.id, total);
      const fetchStart = Math.max(1, total - MAX_WINDOW + 1);
      const latestLines = await GetTabLineRange(currentTab.id, fetchStart, MAX_WINDOW);
      if (latestLines && latestLines.length > 0) {
        tabStore.setLines(currentTab.id, latestLines, total);
      }
      await tick();
      if (container) {
        container.scrollTop = container.scrollHeight;
      }
    } catch (e) {
      console.error('Failed to jump to latest:', e);
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
          {#if tabStatus === 'loading'}
            <div class="loading-indicator">
              <div class="spinner"></div>
              <p>Loading file...</p>
              <p class="muted">{currentTab.filePath}</p>
            </div>
          {:else if tabStatus === 'error'}
            <p class="error-msg">⚠ {tabError}</p>
            <p class="muted">{currentTab.filePath}</p>
            <p class="hint">The file may be on an unreachable mount. It will reload when available.</p>
          {:else}
            <p>Waiting for data...</p>
            <p class="muted">{currentTab.filePath}</p>
          {/if}
        </div>
      {/if}
    </div>

    {#if !autoScroll}
      <button class="scroll-bottom-btn" on:click={scrollToBottom} title="Scroll to bottom (auto-scroll)">
        ↓ Auto-scroll
      </button>
    {/if}
    {#if totalLines > 0}
      <div class="window-indicator">
        {windowStart}–{windowEnd} / {totalLines}
      </div>
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

  .loading-indicator {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-msg {
    color: var(--warning, #e5c07b);
    font-size: 14px;
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

  .window-indicator {
    position: absolute;
    bottom: 16px;
    left: 16px;
    background: var(--bg-surface);
    color: var(--text-muted);
    padding: 3px 10px;
    border-radius: 10px;
    font-size: 11px;
    opacity: 0.8;
    pointer-events: none;
    z-index: 10;
  }
</style>
