<script>
  import { highlightLine } from '../utils/highlight.js';

  export let line;
  export let rules = [];
  export let showLineNumber = false;
  export let fontSize = 14;

  $: segments = highlightLine(line.text, rules);
</script>

<div class="log-line" style="font-size: {fontSize}px">
  {#if showLineNumber}
    <span class="line-number">{line.number}</span>
  {/if}
  <span class="line-content">
    {#each segments as seg}
      <span style={styleString(seg.style)}>{seg.text}</span>
    {/each}
  </span>
</div>

<script context="module">
  function styleString(style) {
    let parts = [];
    if (style.color) parts.push(`color:${style.color}`);
    if (style.backgroundColor) parts.push(`background-color:${style.backgroundColor}`);
    if (style.fontWeight) parts.push(`font-weight:${style.fontWeight}`);
    if (style.fontStyle) parts.push(`font-style:${style.fontStyle}`);
    return parts.join(';');
  }
</script>

<style>
  .log-line {
    display: flex;
    font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', 'Monaco', monospace;
    line-height: 1.5;
    padding: 0 8px;
    min-height: 1.5em;
    width: fit-content;
    min-width: 100%;
  }

  .log-line:hover {
    background: var(--bg-hover);
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
