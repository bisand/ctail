<script>
  import { ChangeTabFilePath } from '../../../wailsjs/go/main/App.js';
  import { tabStore } from '../stores/tabs.js';

  let { show = $bindable(false), tabId = '', filePath = '' } = $props();

  let editPath = $state('');
  let error = $state('');
  let saving = $state(false);

  $effect(() => {
    if (show) {
      editPath = filePath;
      error = '';
      saving = false;
    }
  });

  function close() {
    show = false;
  }

  async function confirm() {
    const trimmed = editPath.trim();
    if (!trimmed) {
      error = 'File path cannot be empty';
      return;
    }
    if (trimmed === filePath) {
      close();
      return;
    }
    saving = true;
    error = '';
    try {
      await ChangeTabFilePath(tabId, trimmed);
      const newFileName = trimmed.split(/[/\\]/).pop();
      tabStore.setFilePath(tabId, trimmed, newFileName);
      close();
    } catch (e) {
      error = e?.message || String(e);
      saving = false;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') close();
    if (e.key === 'Enter') confirm();
  }
</script>

{#if show}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="overlay" onclick={close} onkeydown={handleKeydown}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Change file path">
      <h3 class="dialog-title">Change file path</h3>
      <p class="dialog-desc">Edit the file path to tail a different file while keeping tab settings.</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="path-input"
        type="text"
        bind:value={editPath}
        autofocus
        onkeydown={handleKeydown}
        placeholder="/path/to/logfile.log"
        disabled={saving}
      />
      {#if error}
        <p class="error">{error}</p>
      {/if}
      <div class="dialog-actions">
        <button class="btn btn-cancel" onclick={close} disabled={saving}>Cancel</button>
        <button class="btn btn-confirm" onclick={confirm} disabled={saving}>
          {saving ? 'Applying…' : 'Apply'}
        </button>
      </div>
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

  .dialog {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 24px;
    width: 500px;
    max-width: 90vw;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
  }

  .dialog-title {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .dialog-desc {
    margin: 0 0 16px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .path-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    font-family: 'DejaVu Sans Mono', monospace;
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 6px;
    outline: none;
    box-sizing: border-box;
  }

  .path-input:focus {
    border-color: var(--accent);
  }

  .path-input:disabled {
    opacity: 0.6;
  }

  .error {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--red, #f38ba8);
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .btn {
    padding: 6px 16px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    border: none;
    font-weight: 500;
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .btn-cancel {
    background: var(--bg-surface);
    color: var(--text-secondary);
    border: 1px solid var(--border);
  }

  .btn-cancel:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .btn-confirm {
    background: var(--accent);
    color: var(--bg-primary);
  }

  .btn-confirm:hover:not(:disabled) {
    opacity: 0.9;
  }
</style>
