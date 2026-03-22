<script>
  import { onMount, afterUpdate, tick } from 'svelte';
  import LogLine from './LogLine.svelte';
  import { activeTab, tabStore } from '../stores/tabs.js';
  import { settings } from '../stores/settings.js';
  import { profiles } from '../stores/rules.js';
  import { GetTabLineRange, GetTabTotalLines, GetTabFileSize, GetMemoryUsage } from '../../../wailsjs/go/main/App.js';

  // --- File size & memory stats (polled periodically) ---
  let fileSize = 0;
  let memoryMB = 0;
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

  let container;
  let isAtBottom = true;
  let searchQuery = '';
  let searchVisible = false;

  const FETCH_BATCH = 200;
  const MAX_CACHED_PAGES = 2;
  let swapping = false;
  let prefetching = false;

  // Per-tab scroll position tracking
  const scrollPositions = new Map();

  // --- Prefetch cache: per-tab pages stored ahead/behind scroll buffer ---
  // Map<tabId, { before: Array<lines[]>, after: Array<lines[]> }>
  const pageCache = new Map();

  function getCache(tabId) {
    if (!pageCache.has(tabId)) {
      pageCache.set(tabId, { before: [], after: [] });
    }
    return pageCache.get(tabId);
  }

  function clearCache(tabId) {
    pageCache.delete(tabId);
  }

  $: MAX_WINDOW = ($settings.scrollBuffer || 500);

  $: currentTab = $activeTab;
  $: lines = currentTab ? currentTab.lines : [];
  $: profileName = $settings.activeProfile || 'Common Logs';
  $: profile = $profiles[profileName];
  $: rules = profile ? profile.rules : [];

  // Two-phase render: skip highlighting on tab switch for instant feel,
  // then apply it one frame later.
  let deferHighlight = false;
  let prevTabId = null;
  $: {
    const newId = currentTab ? currentTab.id : null;
    if (newId !== prevTabId) {
      // Save scroll position for the tab we're leaving
      if (prevTabId && container) {
        scrollPositions.set(prevTabId, container.scrollTop);
      }
      // Clear prefetch cache for the tab we're leaving (deferred to avoid lag)
      if (prevTabId) { const oldId = prevTabId; setTimeout(() => clearCache(oldId), 0); }
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
            updateVisibleRange();
          }
        });
        prefetchPages(); refreshStats();
      }
    }
  }
  $: autoScroll = currentTab ? currentTab.autoScroll : true;
  $: totalLines = currentTab ? currentTab.totalLines : 0;
  $: windowStart = lines.length > 0 ? lines[0].number : 0;
  $: windowEnd = lines.length > 0 ? lines[lines.length - 1].number : 0;
  $: canScrollBack = windowStart > 1;
  $: canScrollForward = !autoScroll && windowEnd < totalLines;
  $: tabStatus = currentTab ? currentTab.status : null;
  $: tabError = currentTab ? currentTab.errorMessage : '';

  // --- Virtual scrolling ---
  const OVERSCAN = 10;
  let visibleStart = 0;
  let visibleEnd = 0;

  $: fontSize = $settings.fontSize || 14;
  $: lineHeight = fontSize * 1.5;

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

  $: visibleLines = filteredLines.slice(visibleStart, visibleEnd);
  $: topPad = visibleStart * lineHeight;
  $: bottomPad = Math.max(0, (filteredLines.length - visibleEnd) * lineHeight);
  $: totalContentHeight = filteredLines.length * lineHeight;

  // Recalculate visible range when lines change, but not during a swap
  $: if (filteredLines && !swapping) updateVisibleRange();

  onMount(() => {
    function handleMenuFind() {
      searchVisible = true;
    }
    window.addEventListener('ctail:find', handleMenuFind);
    return () => window.removeEventListener('ctail:find', handleMenuFind);
  });

  // Track previous line count to only auto-scroll when lines change
  let prevLineCount = 0;
  afterUpdate(() => {
    if (autoScroll && container) {
      const curCount = filteredLines.length;
      if (curCount !== prevLineCount) {
        prevLineCount = curCount;
        container.scrollTop = totalContentHeight;
        updateVisibleRange();
      }
    }
  });

  // --- Tier 3: Background prefetch (decoupled from scroll buffer) ---
  async function prefetchPages() {
    if (!currentTab || prefetching) return;
    prefetching = true;
    const tabId = currentTab.id;

    try {
      const cache = getCache(tabId);

      // Prefetch pages BEFORE the current scroll buffer
      while (cache.before.length < MAX_CACHED_PAGES) {
        const ws = lines.length > 0 ? lines[0].number : 0;
        // Account for already-cached pages
        const cachedBefore = cache.before.reduce((sum, p) => sum + p.length, 0);
        const targetStart = ws - cachedBefore;
        if (targetStart <= 1) break;

        const fetchStart = Math.max(1, targetStart - FETCH_BATCH);
        const fetchCount = targetStart - fetchStart;
        if (fetchCount <= 0) break;

        const page = await GetTabLineRange(tabId, fetchStart, fetchCount);
        if (!page || page.length === 0) break;
        if (!currentTab || currentTab.id !== tabId) return;

        cache.before.push(page);
      }

      // Prefetch pages AFTER the current scroll buffer
      while (cache.after.length < MAX_CACHED_PAGES) {
        const we = lines.length > 0 ? lines[lines.length - 1].number : 0;
        const cachedAfter = cache.after.reduce((sum, p) => sum + p.length, 0);
        const targetStart = we + cachedAfter + 1;

        const page = await GetTabLineRange(tabId, targetStart, FETCH_BATCH);
        if (!page || page.length === 0) break;
        if (!currentTab || currentTab.id !== tabId) return;

        cache.after.push(page);
      }
    } catch (e) {
      console.error('Prefetch error:', e);
    } finally {
      prefetching = false;
    }
  }

  // --- Swap cached pages into scroll buffer ---
  async function swapEarlierPage() {
    if (!currentTab || swapping) return false;
    const tabId = currentTab.id;
    const cache = getCache(tabId);
    if (cache.before.length === 0) return false;

    swapping = true;
    const page = cache.before.shift();
    const prevScrollTop = container ? container.scrollTop : 0;
    const adjustment = page.length * lineHeight;

    tabStore.prependLines(tabId, page, MAX_WINDOW);
    cache.after = [];
    // Set scrollTop before tick to avoid a frame with wrong position
    if (container) container.scrollTop = prevScrollTop + adjustment;
    await tick();
    if (container) updateVisibleRange();
    swapping = false;
    return true;
  }

  async function swapLaterPage() {
    if (!currentTab || swapping) return false;
    const tabId = currentTab.id;
    const cache = getCache(tabId);
    if (cache.after.length === 0) return false;

    swapping = true;
    const page = cache.after.shift();
    const prevBufferSize = lines.length;
    const prevScrollTop = container ? container.scrollTop : 0;

    tabStore.appendRangeLines(tabId, page, MAX_WINDOW);
    cache.before = [];
    // Pre-calculate trim and adjust scrollTop before tick
    if (container) {
      const newSize = Math.min(prevBufferSize + page.length, MAX_WINDOW);
      const trimmed = (prevBufferSize + page.length) - newSize;
      if (trimmed > 0) {
        container.scrollTop = prevScrollTop - trimmed * lineHeight;
      }
    }
    await tick();
    if (container) updateVisibleRange();
    swapping = false;
    return true;
  }

  // --- Fetch fallback (when cache is empty) ---
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
      tabStore.prependLines(tabId, olderLines, MAX_WINDOW);
      clearCache(tabId);
      if (container) container.scrollTop = prevScrollTop + adjustment;
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

  async function fetchLaterLines() {
    if (!currentTab || swapping) return;
    const tabId = currentTab.id;

    swapping = true;
    tabStore.setLoadingLines(tabId, true);
    try {
      const we = lines.length > 0 ? lines[lines.length - 1].number : 0;
      const fetchStart = we + 1;
      const prevBufferSize = lines.length;
      const prevScrollTop = container ? container.scrollTop : 0;

      const newerLines = await GetTabLineRange(tabId, fetchStart, FETCH_BATCH);
      if (!newerLines || newerLines.length === 0) return;
      if (!currentTab || currentTab.id !== tabId) return;

      tabStore.appendRangeLines(tabId, newerLines, MAX_WINDOW);
      clearCache(tabId);
      if (container) {
        const newSize = Math.min(prevBufferSize + newerLines.length, MAX_WINDOW);
        const trimmed = (prevBufferSize + newerLines.length) - newSize;
        if (trimmed > 0) {
          container.scrollTop = prevScrollTop - trimmed * lineHeight;
        }
      }
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
    if (!container || !currentTab) return;

    updateVisibleRange();

    const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 30;

    // Auto-scroll toggle is immediate
    if (isAtBottom && !atBottom) {
      tabStore.setAutoScroll(currentTab.id, false);
    }
    if (atBottom && !autoScroll && windowEnd >= totalLines) {
      tabStore.setAutoScroll(currentTab.id, true);
    }
    isAtBottom = atBottom;

    checkAndFetch();
  }

  $: scrollSpeed = $settings.scrollSpeed || 1;
  $: smoothScroll = $settings.smoothScroll || false;

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
      checkAndFetch();
    }
    if (e.deltaY > 0 && container.scrollTop + container.clientHeight >= container.scrollHeight - 5 && canScrollForward) {
      checkAndFetch();
    }
  }

  function checkAndFetch() {
    if (!container || !currentTab || swapping) return;

    const bufferSize = filteredLines.length;
    if (bufferSize === 0) return;

    const lh = lineHeight;
    if (lh <= 0) return;
    const firstVisibleIdx = Math.floor(container.scrollTop / lh);
    const visibleCount = Math.ceil(container.clientHeight / lh);
    const lastVisibleIdx = firstVisibleIdx + visibleCount;

    const triggerTop = Math.floor(bufferSize / 4);
    const triggerBottom = Math.floor(bufferSize * 3 / 4);

    if (firstVisibleIdx < triggerTop && canScrollBack) {
      // Try instant swap from cache first, fall back to async fetch
      swapEarlierPage().then(swapped => {
        if (!swapped) fetchEarlierLines();
        prefetchPages();
      });
    } else if (lastVisibleIdx > triggerBottom && canScrollForward) {
      swapLaterPage().then(swapped => {
        if (!swapped) fetchLaterLines();
        prefetchPages();
      });
    } else {
      // In the middle — good time to top up the cache
      prefetchPages();
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
      const startLines = await GetTabLineRange(currentTab.id, 1, MAX_WINDOW);
      if (startLines && startLines.length > 0) {
        tabStore.setLines(currentTab.id, startLines);
        clearCache(currentTab.id);
      }
      await tick();
      if (container) {
        container.scrollTop = 0;
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
      const fetchStart = Math.max(1, total - MAX_WINDOW + 1);
      const latestLines = await GetTabLineRange(currentTab.id, fetchStart, MAX_WINDOW);
      if (latestLines && latestLines.length > 0) {
        tabStore.setLines(currentTab.id, latestLines, total);
        clearCache(currentTab.id);
      }
      await tick();
      if (container) {
        container.scrollTop = totalContentHeight;
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
    checkAndFetch();
  }

  $: filteredLines = searchQuery
    ? lines.filter(l => l.text.toLowerCase().includes(searchQuery.toLowerCase()))
    : lines;

  // Context menu state
  let contextMenu = { visible: false, x: 0, y: 0 };

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
    // Extract text only from .line-content spans, skipping line numbers
    const range = sel.getRangeAt(0);
    const fragment = range.cloneContents();
    const wrapper = document.createElement('div');
    wrapper.appendChild(fragment);
    const lineNumbers = wrapper.querySelectorAll('.line-number');
    lineNumbers.forEach(el => el.remove());
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

<svelte:window on:keydown={handleKeydown} on:click={closeContextMenu} />

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
    <div class="log-container" bind:this={container} on:scroll={handleScroll} on:wheel={handleWheel} on:contextmenu={handleContextMenu} on:copy={handleCopy}>
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
          <input type="checkbox" checked={autoScroll} on:change={toggleFollow} />
          Follow
        </label>
      </div>
    </div>

    {#if contextMenu.visible}
      <div class="context-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px" role="menu" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
        <button class="ctx-item" on:click={ctxCopy} disabled={!getSelectedText()}>
          Copy <span class="ctx-key">Ctrl+C</span>
        </button>
        <button class="ctx-item" on:click={ctxCopyAll}>
          Copy all lines
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" on:click={ctxSelectAll}>
          Select all <span class="ctx-key">Ctrl+A</span>
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" on:click={ctxSearch}>
          {getSelectedText() ? 'Search selection' : 'Search'} <span class="ctx-key">Ctrl+F</span>
        </button>
        {#if searchVisible}
          <button class="ctx-item" on:click={ctxClearSearch}>
            Clear search
          </button>
        {/if}
        <div class="ctx-separator"></div>
        <button class="ctx-item" on:click={ctxScrollToBottom} disabled={autoScroll}>
          Scroll to bottom
        </button>
        <button class="ctx-item" on:click={toggleFollow}>
          {autoScroll ? 'Unfollow' : 'Follow'} tail
        </button>
        <div class="ctx-separator"></div>
        <button class="ctx-item" on:click={ctxAskAI}>
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
