<script>
  import { onMount } from 'svelte';
  import { GetAppVersion, ListThemes } from '../../../wailsjs/go/main/App.js';
  import { BrowserOpenURL } from '../../../wailsjs/runtime/runtime.js';
  import { settings } from '../stores/settings.js';

  export let show = false;

  let version = '';
  let themeName = '';

  onMount(async () => {
    try {
      version = await GetAppVersion();
    } catch {
      version = '0.0.0';
    }
  });

  // Update display name when settings or dialog visibility changes
  $: if (show) {
    updateThemeName($settings.theme);
  }

  async function updateThemeName(themeId) {
    try {
      const themes = await ListThemes();
      const t = themes.find(th => th.Name === themeId);
      themeName = t ? t.DisplayName : themeId;
    } catch {
      themeName = themeId || 'Unknown';
    }
  }

  function close() {
    show = false;
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') close();
  }

  function openLink(url) {
    BrowserOpenURL(url);
  }
</script>

{#if show}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="overlay" on:click={close} on:keydown={handleKeydown}>
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="about-dialog" on:click|stopPropagation role="dialog" aria-modal="true" aria-label="About ctail">
      <div class="about-icon">
        <div class="icon-graphic">
          <span class="icon-text">ct</span><span class="icon-cursor">|</span>
        </div>
      </div>
      <h2 class="about-title">ctail</h2>
      <p class="about-version">Version {version}</p>
      <p class="about-desc">
        Cross-platform log tail viewer with regex highlighting
      </p>
      <div class="about-links">
        <button class="link-btn" on:click={() => openLink('https://github.com/bisand/ctail')}>
          GitHub Repository
        </button>
        <button class="link-btn" on:click={() => openLink('https://github.com/bisand/ctail/issues')}>
          Report Issue
        </button>
      </div>
      <div class="about-credits">
        <p>Built with <button class="inline-link" on:click={() => openLink('https://wails.io')}>Wails</button> &amp; <button class="inline-link" on:click={() => openLink('https://svelte.dev')}>Svelte</button></p>
        <p>Theme: {themeName} ({$settings.themeMode})</p>
        <p class="about-license">MIT License</p>
      </div>
      <button class="close-btn" on:click={close}>Close</button>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10000;
  }

  .about-dialog {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 32px;
    width: 360px;
    text-align: center;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
  }

  .about-icon {
    margin-bottom: 16px;
  }

  .icon-graphic {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 72px;
    height: 72px;
    border-radius: 16px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
  }

  .icon-text {
    font-family: 'DejaVu Sans Mono', monospace;
    font-size: 28px;
    font-weight: 700;
    color: var(--accent);
  }

  .icon-cursor {
    font-family: 'DejaVu Sans Mono', monospace;
    font-size: 28px;
    font-weight: 400;
    color: var(--green, #a6e3a1);
    animation: blink 1s step-end infinite;
  }

  @keyframes blink {
    50% { opacity: 0; }
  }

  .about-title {
    margin: 0 0 4px;
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .about-version {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--text-secondary);
  }

  .about-desc {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .about-links {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-bottom: 16px;
  }

  .link-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 12px;
    background: var(--bg-surface);
    color: var(--accent);
    border: 1px solid var(--border);
    cursor: pointer;
  }

  .link-btn:hover {
    background: var(--bg-hover);
  }

  .about-credits {
    margin-bottom: 16px;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.8;
  }

  .about-credits p {
    margin: 0;
  }

  .inline-link {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    padding: 0;
    text-decoration: underline;
  }

  .inline-link:hover {
    color: var(--text-primary);
  }

  .about-license {
    margin-top: 4px;
    color: var(--text-secondary);
    opacity: 0.7;
  }

  .close-btn {
    padding: 6px 24px;
    border-radius: 6px;
    font-size: 13px;
    background: var(--accent);
    color: var(--bg-primary);
    border: none;
    cursor: pointer;
    font-weight: 600;
  }

  .close-btn:hover {
    opacity: 0.9;
  }
</style>
