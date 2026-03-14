<script>
  import { BrowserOpenURL } from '../../../wailsjs/runtime/runtime.js';

  export let show = false;
  export let result = null; // { updateAvailable, latestVersion, currentVersion, url, error }

  function close() {
    show = false;
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') close();
  }

  function openRelease() {
    if (result?.url) {
      BrowserOpenURL(result.url);
    }
  }
</script>

{#if show && result}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="overlay" on:click={close} on:keydown={handleKeydown}>
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="update-dialog" on:click|stopPropagation role="dialog" aria-modal="true" aria-label="Update check">
      {#if result.error}
        <div class="update-icon error">⚠️</div>
        <h3>Update Check Failed</h3>
        <p class="update-message">{result.error}</p>
      {:else if result.updateAvailable}
        <div class="update-icon available">🎉</div>
        <h3>Update Available!</h3>
        <p class="update-message">
          A new version of ctail is available.
        </p>
        <div class="version-info">
          <span class="version-label">Current:</span>
          <span class="version-value">v{result.currentVersion}</span>
        </div>
        <div class="version-info">
          <span class="version-label">Latest:</span>
          <span class="version-value highlight">v{result.latestVersion}</span>
        </div>
        <div class="button-row">
          <button class="btn primary" on:click={openRelease}>View Release</button>
          <button class="btn secondary" on:click={close}>Later</button>
        </div>
      {:else}
        <div class="update-icon uptodate">✅</div>
        <h3>You're Up to Date</h3>
        <p class="update-message">
          ctail v{result.currentVersion} is the latest version.
        </p>
        <button class="btn primary" on:click={close}>OK</button>
      {/if}
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

  .update-dialog {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 28px 32px;
    width: 340px;
    text-align: center;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
  }

  .update-icon {
    font-size: 40px;
    margin-bottom: 12px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .update-message {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .version-info {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 13px;
  }

  .version-label {
    color: var(--text-secondary);
  }

  .version-value {
    color: var(--text-primary);
    font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace;
    font-weight: 600;
  }

  .version-value.highlight {
    color: var(--accent);
  }

  .button-row {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 16px;
  }

  .btn {
    padding: 6px 20px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    font-weight: 600;
    border: none;
  }

  .btn.primary {
    background: var(--accent);
    color: var(--bg-primary);
    margin-top: 16px;
  }

  .button-row .btn.primary {
    margin-top: 0;
  }

  .btn.primary:hover {
    opacity: 0.9;
  }

  .btn.secondary {
    background: var(--bg-surface);
    color: var(--text-primary);
    border: 1px solid var(--border);
  }

  .btn.secondary:hover {
    background: var(--bg-hover);
  }
</style>
