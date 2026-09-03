<script>
  import { settingsPanelOpen } from '../stores/settings.js';

  let { onOpenFile } = $props();
</script>

<div class="toolbar">
  <div class="toolbar-left">
    <span class="app-title">ctail</span>
  </div>
  <div class="toolbar-right">
    <button class="toolbar-btn" onclick={onOpenFile} title="Open file (Ctrl+O)">
      📁 Open
    </button>
    <button class="toolbar-btn" class:active={$settingsPanelOpen}
      onclick={() => settingsPanelOpen.update(v => !v)}
      title="Settings & Rules">
      ⚙ Settings
    </button>
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    height: 38px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    user-select: none;
    --wails-draggable: drag;
  }

  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .app-title {
    font-weight: 700;
    font-size: 14px;
    color: var(--accent);
    letter-spacing: 0.5px;
  }

  .toolbar-right {
    display: flex;
    gap: 4px;
    --wails-draggable: no-drag;
  }

  .toolbar-btn {
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .toolbar-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .toolbar-btn.active {
    background: var(--bg-surface);
    color: var(--accent);
  }
</style>
