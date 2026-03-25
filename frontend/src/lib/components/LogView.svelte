<script>
  import { onMount, tick } from 'svelte';
  import LogLine from './LogLine.svelte';
  import { activeTab, tabStore } from '../stores/tabs.js';
  import { settings } from '../stores/settings.js';
  import { profiles } from '../stores/rules.js';
  import { GetTabLineRange, GetTabTotalLines, GetTabFileSize, GetMemoryUsage } from '../../../wailsjs/go/main/App.js';

  // --- File size & memory stats (polled periodically) ---
  let fileSize = $state(0);
  let memoryMB = $state(0);
  let statsTimer = null;

  function formatSize(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
    return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
  }

  async function refreshStats() {
    try {
      if (currentTab) {
        fileSize = await GetTabFileSize(currentTab.id);
      }
      const mem = await GetMemoryUsage();
      memoryMB = mem.alloc;
    } catch (_) {}
  }

  onMount(() => {
    refreshStats();
    statsTimer = setInterval(refreshStats, 3000);
    return () => { if (statsTimer) clearInterval(statsTimer); };
  });

  let container = $state(null);
  let isAtBottom = true;
  let searchQuery = $state('');
  let searchVisible = $state(false);

  const FETCH_BATCH = 1000;
  let swapping = false;
  let programmaticScroll = false;
  let lastScrollTop = 0;

  // Per-tab scroll position tracking
  const scrollPositions = new Map();

  let currentTab = $derived($activeTab);
  let lines = $derived(currentTab ? currentTab.lines : []);
  let profileName = $derived($settings.activeProfile || 'Common Logs');
  let profile = $derived($profiles[profileName]);
  let rules = $derived(profile ? profile.rules : []);

  // Two-phase render: skip highlighting on tab switch for instant feel,
  // then apply it one frame later.
  let deferHighlight = $state(false);
  let prevTabId = null;
  $effect(() => {
    const newId = currentTab ? currentTab.id : null;
    if (newId !== prevTabId) {
      // Save scroll position for the tab we're leaving
      if (prevTabId && container) {
        scrollPositions.set(prevTabId, container.scrollTop);
      }
      // Clear prefetch cache for the tab we're leaving (deferred to avoid lag)
      prevTabId = newId;
      deferHighlight = true;
      requestAnimationFrame(() => { deferHighlight = false; });
      // Restore scroll position for the new tab and force repaint
      if (newId) {
        tick().then(() => {
          if (container) {
            const savedScroll = scrollPositions.get(newId);
            if (currentTab && currentTab.autoScroll) {
              container.scrollTop = container.scrollHeight;
            } else if (savedScroll !== undefined) {
              container.scrollTop = savedScroll;
            } else {
              container.scrollTop = container.scrollHeight;
            }
            lastScrollTop = container.scrollTop;
            updateVisibleRange();
          }
        });
        refreshStats();
      }
    }
  });
  let autoScroll = $derived(currentTab ? currentTab.autoScroll : true);
  let totalLines = $derived(currentTab ? currentTab.totalLines : 0);
  let windowStart = $derived(lines.length > 0 ? lines[0].number : 0);
  let windowEnd = $derived(lines.length > 0 ? lines[lines.length - 1].number : 0);
  let canScrollBack = $derived(windowStart > 1);
  let canScrollForward = $derived(!autoScroll && windowEnd > 0 && windowEnd < totalLines);
  let tabStatus = $derived(currentTab ? currentTab.status : null);
  let tabError = $derived(currentTab ? currentTab.errorMessage : '');

  // --- Virtual scrolling ---
  const OVERSCAN = 10;
  let visibleStart = $state(0);
  let visibleEnd = $state(0);

  let fontSize = $derived($settings.fontSize || 14);
  let lineHeight = $derived(fontSize * 1.5);

  let filteredLines = $derived(searchQuery
    ? lines.filter(l => l.text.toLowerCase().includes(searchQuery.toLowerCase()))
    : lines);

  // Compute which slice of filteredLines to render
  function updateVisibleRange() {
    if (!container || filteredLines.length === 0) {
      visibleStart = 0;
      visibleEnd = 0;
      return;
    }
    const scrollTop = container.scrollTop;
    const viewHeight = container.clientHeight;
    const first = Math.floor(scrollTop / lineHeight);
    const count = Math.ceil(viewHeight / lineHeight);
    visibleStart = Math.max(0, first - OVERSCAN);
    visibleEnd = Math.min(filteredLines.length, first + count + OVERSCAN);
  }

  let visibleLines = $derived(filteredLines.slice(visibleStart, visibleEnd));
  let topPad = $derived(visibleStart * lineHeight);
  let bottomPad = $derived(Math.max(0, (filteredLines.length - visibleEnd) * lineHeight));
  let totalContentHeight = $derived(filteredLines.length * lineHeight);

  // Recalculate visible range when lines change, but not during a swap
  $effect.pre(() => {
    const _len = filteredLines.length;
    if (!swapping) updateVisibleRange();
  });

  onMount(() => {
    function handleMenuFind() {
      searchVisible = true;
    }
    window.addEventListener('ctail:find', handleMenuFind);
    return () => window.removeEventListener('ctail:find', handleMenuFind);
  });

  // Track previous line count to only auto-scroll when lines change
  let prevLineCount = 0;
  $effect(() => {
    const curCount = filteredLines.length;
    if (autoScroll && container && curCount !== prevLineCount) {
      prevLineCount = curCount;
      container.scrollTop = totalContentHeight;
      updateVisibleRange();
    }
  });

  // --- Fetch earlier lines when user scrolls near top ---
  async function fetchEarlierLines() {
    if (!currentTab || swapping) return;
    const tabId = currentTab.id;

    swapping = true;
    tabStore.setLoadingLines(tabId, true);
    try {
      const ws = lines.length > 0 ? lines[0].number : 0;
      const fetchStart = Math.max(1, ws - FETCH_BATCH);
      const fetchCount = ws - fetchStart;
      if (fetchCount <= 0) return;

      const olderLines = await GetTabLineRange(tabId, fetchStart, fetchCount);
      if (!olderLines || olderLines.length === 0) return;
      if (!currentTab || currentTab.id !== tabId) return;

      const prevScrollTop = container ? container.scrollTop : 0;
      const adjustment = olderLines.length * lineHeight;
      tabStore.prependLines(tabId, olderLines);
      if (container) {
        programmaticScroll = true;
        container.scrollTop = prevScrollTop + adjustment;
        lastScrollTop = container.scrollTop;
        programmaticScroll = false;
      }
      await tick();
      if (container) updateVisibleRange();
    } catch (e) {
      console.error('Failed to load earlier lines:', e);
    } finally {
      swapping = false;
      if (currentTab && currentTab.id === tabId) {
        tabStore.setLoadingLines(tabId, false);
      }
    }
  }

  // --- Fetch later lines when user scrolls near bottom (not in auto-scroll) ---
  async function fetchLaterLines() {
    if (!currentTab || swapping) return;
    const tabId = currentTab.id;

    swapping = true;
    tabStore.setLoadingLines(tabId, true);
    try {
      const we = lines.length > 0 ? lines[lines.length - 1].number : 0;
      const fetchStart = we + 1;

      const newerLines = await GetTabLineRange(tabId, fetchStart, FETCH_BATCH);
      if (!newerLines || newerLines.length === 0) return;
      if (!currentTab || currentTab.id !== tabId) return;

      const merged = [...lines, ...newerLines];
      tabStore.setLines(tabId, merged);
      await tick();
      if (container) updateVisibleRange();
    } catch (e) {
      console.error('Failed to load later lines:', e);
    } finally {
      swapping = false;
      if (currentTab && currentTab.id === tabId) {
        tabStore.setLoadingLines(tabId, false);
      }
    }
  }

  function handleScroll() {
    if (!container || !currentTab || programmaticScroll) return;

    updateVisibleRange();

    const scrollDelta = container.scrollTop - lastScrollTop;
    lastScrollTop = container.scrollTop;

    const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 30;

    if (isAtBottom && !atBottom) {
      tabStore.setAutoScroll(currentTab.id, false);
    }
    if (atBottom && !autoScroll && windowEnd >= totalLines) {
      tabStore.setAutoScroll(currentTab.id, true);
    }
    isAtBottom = atBottom;

    if (scrollDelta < 0) {
      checkAndFetchUp();
    } else if (scrollDelta > 0) {
      checkAndFetchDown();
    }
  }

  let scrollSpeed = $derived($settings.scrollSpeed || 1);
  let smoothScroll = $derived($settings.smoothScroll || false);

  // Always take over wheel scrolling to eliminate browser-imposed
  // deceleration near scroll edges (unless smooth scroll is enabled).
  function handleWheel(e) {
    if (!container || !currentTab) return;

    // Shift+wheel → horizontal scroll
    if (e.shiftKey && e.deltaY !== 0) {
      e.preventDefault();
      container.scrollLeft += e.deltaY * scrollSpeed;
      return;
    }

    // Pure horizontal scroll — let browser handle it
    if (e.deltaX !== 0 && e.deltaY === 0) return;

    if (!smoothScroll) {
      e.preventDefault();
      container.scrollTop += e.deltaY * scrollSpeed;
      updateVisibleRange();
    } else if (scrollSpeed > 1) {
      e.preventDefault();
      container.scrollTop += e.deltaY * scrollSpeed;
      updateVisibleRange();
    }

    if (e.deltaY < 0 && container.scrollTop <= 0 && canScrollBack) {
      checkAndFetchUp();
    }
    if (e.deltaY > 0 && container.scrollTop + container.clientHeight >= container.scrollHeight - 5 && canScrollForward) {
      checkAndFetchDown();
    }
  }

  // Fetch earlier lines when user scrolls near the top of the buffer
  function checkAndFetchUp() {
    if (!container || !currentTab || swapping || !canScrollBack) return;

    const triggerZone = container.scrollHeight * 0.2;
    if (container.scrollTop < triggerZone) {
      fetchEarlierLines();
    }
  }

  // Fetch later lines when user scrolls near the bottom of the buffer
  function checkAndFetchDown() {
    if (!container || !currentTab || swapping || !canScrollForward) return;

    const bottomDistance = container.scrollHeight - container.scrollTop - container.clientHeight;
    const triggerZone = container.scrollHeight * 0.2;
    if (bottomDistance < triggerZone) {
      fetchLaterLines();
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

  async function jumpToStart() {
    if (!currentTab) return;
    try {
      tabStore.setAutoScroll(currentTab.id, false);
      const startLines = await GetTabLineRange(currentTab.id, 1, FETCH_BATCH);
      if (startLines && startLines.length > 0) {
        tabStore.setLines(currentTab.id, startLines);
      }
      await tick();
      if (container) {
        container.scrollTop = 0;
        lastScrollTop = 0;
        updateVisibleRange();
      }
    } catch (e) {
      console.error('Failed to jump to start:', e);
    }
  }

  async function jumpToLatest() {
    if (!currentTab) return;
    try {
      const total = await GetTabTotalLines(currentTab.id);
      tabStore.setTotalLines(currentTab.id, total);
      const fetchStart = Math.max(1, total - FETCH_BATCH + 1);
      const latestLines = await GetTabLineRange(currentTab.id, fetchStart, FETCH_BATCH);
      if (latestLines && latestLines.length > 0) {
        tabStore.setLines(currentTab.id, latestLines, total);
      }
      await tick();
      if (container) {
        container.scrollTop = totalContentHeight;
        lastScrollTop = container.scrollTop;
        updateVisibleRange();
      }
    } catch (e) {
      console.error('Failed to jump to latest:', e);
    }
  }

  async function handleKeydown(e) {
    if (e.ctrlKey && e.key === 'f') {
      e.preventDefault();
      searchVisible = !searchVisible;
      if (!searchVisible) searchQuery = '';
      return;
    }
    if (e.key === 'Escape' && searchVisible) {
      searchVisible = false;
      searchQuery = '';
      return;
    }

    // Keyboard scrolling (only when log container is available and focused area)
    if (!container || !currentTab) return;
    // Skip if user is typing in an input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    const pageSize = container.clientHeight;
    let scrollDelta = 0;

    switch (e.key) {
      case 'ArrowUp':
        scrollDelta = -lineHeight;
        break;
      case 'ArrowDown':
        scrollDelta = lineHeight;
        break;
      case 'ArrowLeft':
        e.preventDefault();
        container.scrollLeft -= 40 * scrollSpeed;
        return;
      case 'ArrowRight':
        e.preventDefault();
        container.scrollLeft += 40 * scrollSpeed;
        return;
      case 'PageUp':
        scrollDelta = -pageSize;
        break;
      case 'PageDown':
        scrollDelta = pageSize;
        break;
      case 'Home':
        e.preventDefault();
        jumpToStart();
        return;
      case 'End':
        e.preventDefault();
        scrollToBottom();
        return;
      default:
        return;
    }

    e.preventDefault();
    container.scrollTop += scrollDelta * scrollSpeed;
    updateVisibleRange();
    // Force Svelte to flush DOM updates (spacers) before the browser paints,
    // preventing blank gaps after large jumps like PgUp/PgDn.
    await tick();
    if (scrollDelta < 0) checkAndFetchUp();
    else if (scrollDelta > 0) checkAndFetchDown();
  }

  // Context menu state
  let contextMenu = $state({ visible: false, x: 0, y: 0 });

  function handleContextMenu(e) {
    e.preventDefault();
    contextMenu = { visible: true, x: e.clientX, y: e.clientY };
  }

  function closeContextMenu() {
    contextMenu = { ...contextMenu, visible: false };
  }

  function getSelectedText() {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return '';
    // Extract text only from .line-content spans, skipping line numbers,
    // and join with newlines so each log line is preserved.
    const range = sel.getRangeAt(0);
    const fragment = range.cloneContents();
    const wrapper = document.createElement('div');
    wrapper.appendChild(fragment);
    const lineNumbers = wrapper.querySelectorAll('.line-number');
    lineNumbers.forEach(el => el.remove());
    const logLines = wrapper.querySelectorAll('.log-line');
    if (logLines.length > 0) {
      return Array.from(logLines).map(el => el.textContent).join('\n');
    }
    // Fallback for partial selection within a single line
    return wrapper.textContent || '';
  }

  function handleCopy(e) {
    const text = getSelectedText();
    if (text) {
      e.preventDefault();
      e.clipboardData.setData('text/plain', text);
    }
  }

  function ctxCopy() {
    const text = getSelectedText();
    if (text) navigator.clipboard.writeText(text);
    closeContextMenu();
  }

  function ctxCopyAll() {
    const text = filteredLines.map(l => l.text).join('\n');
    navigator.clipboard.writeText(text);
    closeContextMenu();
  }

  function ctxSelectAll() {
    if (!container) return;
    const range = document.createRange();
    range.selectNodeContents(container);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
    closeContextMenu();
  }

  function ctxSearch() {
    const text = getSelectedText();
    searchVisible = true;
    if (text) searchQuery = text;
    closeContextMenu();
  }

  function ctxScrollToBottom() {
    scrollToBottom();
    closeContextMenu();
  }

  function ctxClearSearch() {
    searchQuery = '';
    searchVisible = false;
    closeContextMenu();
  }

  function ctxAskAI() {
    window.dispatchEvent(new CustomEvent('ctail:open-ai'));
    closeContextMenu();
  }
</script>

<svelte:window onkeydown={handleKeydown} onclick={closeContextMenu} />

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
      <button class="search-close" onclick={() => { searchVisible = false; searchQuery = ''; }}>×</button>
    </div>
  {/if}

  {#if currentTab}
    <div class="log-container" bind:this={container} onscroll={handleScroll} onwheel={handleWheel} oncontextmenu={handleContextMenu} oncopy={handleCopy}>
      {#if filteredLines.length > 0}
        <div class="virtual-spacer" style="height: {topPad}px"></div>
        {#key currentTab?.id}
          {#each visibleLines as line (line.number)}
            <LogLine
              {line}
              rules={deferHighlight ? [] : rules}
              showLineNumber={$settings.showLineNumbers}
              fontSize={$settings.fontSize}
            />
          {/each}
        {/key}
        <div class="virtual-spacer" style="height: {bottomPad}px"></div>
      {/if}
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
        {#if tabStatus === 'error'}
          <span class="status-text status-error">⚠ {tabError}</span>
        {:else}
          <span class="status-text" title={currentTab.filePath}>{currentTab.filePath}</span>
        {/if}
        {#if fileSize > 0}
          <span class="status-dim">{formatSize(fileSize)}</span>
        {/if}
      </div>
      <div class="status-right">
        <span class="status-dim" title="Process memory">🗄 {formatSize(memoryMB)}</span>
        <span class="status-sep">│</span>
        {#if tabStatus === 'loading'}
          <span class="status-text">Loading…</span>
        {:else if totalLines > 0}
          <span class="status-text">Lines {windowStart}–{windowEnd} of {totalLines}</span>
        {:else}
          <span class="status-text">Empty</span>
        {/if}
        <label class="follow-toggle" title="Auto-scroll to new lines (per tab)">
          <input type="checkbox" checked={autoScroll} onchange={toggleFollow} />
          Follow
        </label>
      </div>
    </div>

    {#if contextMenu.visible}
      <div class="context-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
        <button class="ctx-item" onclick={ctxCopy} disabled={!getSelectedText()}>
          Copy <span class="ctx-key">Ctrl+C</span>
        </button>
        <button class="ctx-item" onclick={ctxCopyAll}>
          Copy all lines
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" onclick={ctxSelectAll}>
          Select all <span class="ctx-key">Ctrl+A</span>
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" onclick={ctxSearch}>
          {getSelectedText() ? 'Search selection' : 'Search'} <span class="ctx-key">Ctrl+F</span>
        </button>
        {#if searchVisible}
          <button class="ctx-item" onclick={ctxClearSearch}>
            Clear search
          </button>
        {/if}
        <div class="ctx-separator"></div>
        <button class="ctx-item" onclick={ctxScrollToBottom} disabled={autoScroll}>
          Scroll to bottom
        </button>
        <button class="ctx-item" onclick={toggleFollow}>
          {autoScroll ? 'Unfollow' : 'Follow'} tail
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" onclick={ctxAskAI}>
          🤖 Ask AI about logs <span class="ctx-key">Ctrl+Shift+A</span>
        </button>
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
    overflow-anchor: none;
    overscroll-behavior: none;
    padding: 4px 0;
    contain: size;
  }

  .virtual-spacer {
    width: 100%;
    pointer-events: none;
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
    overflow: hidden;
    min-width: 0;
  }

  .status-left .status-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .status-text {
    white-space: nowrap;
  }

  .status-dim {
    white-space: nowrap;
    opacity: 0.6;
    font-size: 0.85em;
  }

  .status-sep {
    opacity: 0.3;
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

  .context-menu {
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
    font-family: inherit;
  }

  .ctx-separator {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }
</style>
