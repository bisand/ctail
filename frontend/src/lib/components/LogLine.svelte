<script module>
  function styleString(style) {
    let parts = [];
    if (style.color) parts.push(`color:${style.color}`);
    if (style.backgroundColor) parts.push(`background-color:${style.backgroundColor}`);
    if (style.fontWeight) parts.push(`font-weight:${style.fontWeight}`);
    if (style.fontStyle) parts.push(`font-style:${style.fontStyle}`);
    return parts.join(';');
  }

  /** Split a segment's text using a RegExp, preserving the
   *  original style on non-matching parts.  Returns flat array of
   *  { text, style, highlight } objects. */
  function splitOnSearch(seg, re) {
    if (!re) return [seg];
    re.lastIndex = 0;
    const parts = [];
    let pos = 0;
    let m;
    while ((m = re.exec(seg.text)) !== null) {
      if (m[0].length === 0) break;
      if (m.index > pos) parts.push({ text: seg.text.slice(pos, m.index), style: seg.style });
      parts.push({ text: seg.text.slice(m.index, m.index + m[0].length), style: seg.style, highlight: true });
      pos = m.index + m[0].length;
    }
    if (pos < seg.text.length) parts.push({ text: seg.text.slice(pos), style: seg.style });
    return parts.length ? parts : [seg];
  }
</script>

<script>
  import { highlightLine } from '../utils/highlight.js';

  let { line, rules = [], showLineNumber = false, fontSize = 14, searchRe = null, isCurrentMatch = false, searchHighlightColor = '' } = $props();

  let segments = $derived(highlightLine(line.text, rules));
  let searchSegments = $derived(
    searchRe ? segments.flatMap(seg => splitOnSearch(seg, searchRe)) : segments
  );

  // For highlighted segments, strip backgroundColor so the search highlight shows through
  function highlightStyle(style) {
    const { backgroundColor, ...rest } = style;
    return styleString(rest);
  }
</script>

<div class="log-line" class:search-current={isCurrentMatch} style="font-size: {fontSize}px{searchHighlightColor ? `; --search-hl: ${searchHighlightColor}` : ''}">
  {#if showLineNumber}
    <span class="line-number">{line.number}</span>
  {/if}
  <span class="line-content">
    {#each searchSegments as seg}
      {#if seg.highlight}
        <mark class="search-highlight" style={highlightStyle(seg.style)}>{seg.text}</mark>
      {:else}
        <span style={styleString(seg.style)}>{seg.text}</span>
      {/if}
    {/each}
  </span>
</div>

<style>
  .log-line {
    display: flex;
    font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', 'Monaco', monospace;
    line-height: 1.5;
    padding: 0 8px;
    min-height: 1.5em;
    width: fit-content;
    min-width: 100%;
    contain: content;
  }

  .log-line:hover {
    background: var(--bg-hover);
  }

  .log-line.search-current {
    background: var(--search-current-line, rgba(255, 213, 79, 0.10));
  }

  .search-highlight {
    background: var(--search-hl, var(--search-highlight, rgba(255, 213, 79, 0.35)));
    color: inherit;
    border-radius: 2px;
  }

  .search-current .search-highlight {
    background: var(--search-hl, var(--search-highlight-active, rgba(255, 152, 0, 0.55)));
    filter: brightness(1.3);
  }

  .line-number {
    color: var(--text-muted);
    min-width: 50px;
    text-align: right;
    padding-right: 12px;
    user-select: none;
    flex-shrink: 0;
  }

  .line-content {
    white-space: pre;
    flex: 1;
  }

  :global([data-wordwrap="true"]) .line-content {
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
