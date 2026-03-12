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
  const MAX_WINDOW = 500;
  let fetchTimer = null;

  // Debounce: only one fetch check per pause in scrolling
  function scheduleFetchCheck() {
    if (fetchTimer) clearTimeout(fetchTimer);
    fetchTimer = setTimeout(() => {
      fetchTimer = null;
      checkAndFetch();
    }, 100);
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
        const prevScrollTop = container.scrollTop;
        const prevBufferSize = lines.length;

        tabStore.prependLines(currentTab.id, olderLines, MAX_WINDOW);

        await tick();
        if (container) {
          // Buffer stayed ~same size (prepend N, trim N from bottom).
          // Content shifted down by addedCount lines, so move scrollTop
          // down by that many lines to keep the visible content stable.
          const lineHeight = container.scrollHeight / (lines.length || 1);
          container.scrollTop = prevScrollTop + olderLines.length * lineHeight;
        }
      }
    } catch (e) {
      console.error('Failed to load earlier lines:', e);
    } finally {
      tabStore.setLoadingLines(currentTab.id, false);
    }
  }

  async function loadLaterLines() {
    if (!currentTab || currentTab.loadingLines || !canScrollForward) return;

    tabStore.setLoadingLines(currentTab.id, true);
    try {
      const fetchStart = windowEnd + 1;
      const prevBufferSize = lines.length;
      const prevScrollTop = container.scrollTop;

      const newerLines = await GetTabLineRange(currentTab.id, fetchStart, FETCH_BATCH);
      if (newerLines && newerLines.length > 0) {
        tabStore.appendRangeLines(currentTab.id, newerLines, MAX_WINDOW);

        await tick();
        if (container) {
          // Buffer stayed ~same size (append N, trim N from top).
          // Content shifted up by trimmedCount lines, so move scrollTop
          // up to keep the visible content stable.
          const trimmed = (prevBufferSize + newerLines.length) - lines.length;
          if (trimmed > 0) {
            const lineHeight = container.scrollHeight / (lines.length || 1);
            container.scrollTop = prevScrollTop - trimmed * lineHeight;
          }
        }
      }
    } catch (e) {
      console.error('Failed to load later lines:', e);
    } finally {
      tabStore.setLoadingLines(currentTab.id, false);
    }
  }

  function handleScroll() {
    if (!container || !currentTab) return;

    const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 30;

    // Auto-scroll toggle is immediate
    if (isAtBottom && !atBottom) {
      tabStore.setAutoScroll(currentTab.id, false);
    }
    if (atBottom && !autoScroll && windowEnd >= totalLines) {
      tabStore.setAutoScroll(currentTab.id, true);
    }
    isAtBottom = atBottom;

    scheduleFetchCheck();
  }

  // Wheel events fire even when scrollTop is at 0 or max,
  // so we can detect the user wanting to scroll beyond the rendered content.
  function handleWheel(e) {
    if (!container || !currentTab || currentTab.loadingLines) return;

    if (e.deltaY < 0 && container.scrollTop <= 0 && canScrollBack) {
      scheduleFetchCheck();
    }
    if (e.deltaY > 0 && container.scrollTop + container.clientHeight >= container.scrollHeight - 5 && canScrollForward) {
      scheduleFetchCheck();
    }
  }

  function checkAndFetch() {
    if (!container || !currentTab || currentTab.loadingLines) return;

    const bufferSize = lines.length;
    if (bufferSize === 0) return;

    // Estimate which buffer lines are visible
    const lineHeight = container.scrollHeight / bufferSize;
    if (lineHeight <= 0) return;
    const firstVisibleIdx = Math.floor(container.scrollTop / lineHeight);
    const visibleCount = Math.ceil(container.clientHeight / lineHeight);
    const lastVisibleIdx = firstVisibleIdx + visibleCount;

    const triggerTop = Math.floor(bufferSize / 4);
    const triggerBottom = Math.floor(bufferSize * 3 / 4);

    // Viewport is in the top 1/3 of the buffer → load earlier lines
    if (firstVisibleIdx < triggerTop && canScrollBack) {
      loadEarlierLines();
    }
    // Viewport is in the bottom 1/3 of the buffer → load later lines
    else if (lastVisibleIdx > triggerBottom && canScrollForward) {
      loadLaterLines();
    }
  }

  function scrollToBottom() {
    if (currentTab) {
      tabStore.setAutoScroll(currentTab.id, true);
      jumpToLatest();
    }
  }

  function toggleFollow() {
    if (!currentTab) return;
    if (autoScroll) {
      tabStore.setAutoScroll(currentTab.id, false);
    } else {
      scrollToBottom();
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
    <div class="log-container" bind:this={container} on:scroll={handleScroll} on:wheel={handleWheel}>
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

    <div class="status-bar">
      <div class="status-left">
        {#if currentTab.loadingLines}
          <span class="status-loading">⟳</span>
        {/if}
        {#if tabStatus === 'loading'}
          <span class="status-text">Loading…</span>
        {:else if tabStatus === 'error'}
          <span class="status-text status-error">⚠ {tabError}</span>
        {:else if totalLines > 0}
          <span class="status-text">Lines {windowStart}–{windowEnd} of {totalLines}</span>
        {:else}
          <span class="status-text">Empty</span>
        {/if}
      </div>
      <div class="status-right">
        <label class="follow-toggle" title="Auto-scroll to new lines (per tab)">
          <input type="checkbox" checked={autoScroll} on:change={toggleFollow} />
          Follow
        </label>
      </div>
    </div>
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

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    height: 24px;
    min-height: 24px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
    user-select: none;
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-text {
    white-space: nowrap;
  }

  .status-error {
    color: var(--warning, #e5c07b);
  }

  .status-loading {
    animation: spin 1s linear infinite;
    display: inline-block;
  }

  .follow-toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-muted);
  }

  .follow-toggle:hover {
    color: var(--text-primary);
  }

  .follow-toggle input[type="checkbox"] {
    margin: 0;
    cursor: pointer;
    accent-color: var(--accent);
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
