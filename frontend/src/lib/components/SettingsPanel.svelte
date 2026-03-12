<script>
  import { settings } from '../stores/settings.js';
  import { profiles, profileNames } from '../stores/rules.js';
  import { SaveSettings, SaveProfile, DeleteProfile, GetProfile } from '../../../wailsjs/go/main/App.js';

  let activeSection = 'settings';
  let editingRule = null;
  let editingProfileName = '';
  let newProfileName = '';
  let showNewProfile = false;

  // Rule editor state
  let ruleName = '';
  let rulePattern = '';
  let ruleMatchType = 'match';
  let ruleForeground = '#89b4fa';
  let ruleBackground = '';
  let ruleBold = false;
  let ruleItalic = false;

  $: selectedProfile = $settings.activeProfile || 'Common Logs';
  $: currentProfile = $profiles[selectedProfile];
  $: currentRules = currentProfile
    ? [...currentProfile.rules].sort((a, b) => a.priority - b.priority)
    : [];

  // Drag state (pointer-based for WebKit compatibility)
  let dragIndex = null;
  let dragOverIndex = null;
  let isDragging = false;
  let dragEl = null;
  let dragStartY = 0;

  function selectSection(section) {
    activeSection = section;
  }

  async function saveSettings() {
    try {
      await SaveSettings($settings);
    } catch (e) {
      console.error('Failed to save settings:', e);
    }
  }

  function updateSetting(key, value) {
    settings.update(s => ({ ...s, [key]: value }));
    saveSettings();
  }

  function startEditRule(rule) {
    editingRule = rule.id;
    ruleName = rule.name;
    rulePattern = rule.pattern;
    ruleMatchType = rule.matchType;
    ruleForeground = rule.foreground || '#89b4fa';
    ruleBackground = rule.background || '';
    ruleBold = rule.bold;
    ruleItalic = rule.italic;
  }

  function startNewRule() {
    editingRule = '__new__';
    ruleName = '';
    rulePattern = '';
    ruleMatchType = 'match';
    ruleForeground = '#89b4fa';
    ruleBackground = '';
    ruleBold = false;
    ruleItalic = false;
  }

  function cancelEdit() {
    editingRule = null;
  }

  // Reassign priorities based on array position and persist
  async function saveRules(rules) {
    const reindexed = rules.map((r, i) => ({ ...r, priority: i }));
    const updated = { name: selectedProfile, rules: reindexed };
    profiles.update(p => ({ ...p, [selectedProfile]: updated }));
    await SaveProfile(updated);
  }

  async function moveRule(index, direction) {
    const target = index + direction;
    if (target < 0 || target >= currentRules.length) return;
    const reordered = [...currentRules];
    [reordered[index], reordered[target]] = [reordered[target], reordered[index]];
    await saveRules(reordered);
  }

  function handlePointerDown(e, index) {
    if (e.target.closest('.order-btn') || e.target.closest('.icon-btn-small') || e.target.tagName === 'INPUT') return;
    dragIndex = index;
    dragStartY = e.clientY;
    dragEl = e.currentTarget;
    dragEl.setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e) {
    if (dragIndex === null) return;
    if (!isDragging && Math.abs(e.clientY - dragStartY) > 5) {
      isDragging = true;
    }
    if (!isDragging) return;

    const listEl = dragEl.parentElement;
    const items = [...listEl.children];
    for (let i = 0; i < items.length; i++) {
      const rect = items[i].getBoundingClientRect();
      if (e.clientY < rect.top + rect.height / 2) {
        dragOverIndex = i;
        return;
      }
    }
    dragOverIndex = items.length;
  }

  async function handlePointerUp(e) {
    if (dragIndex === null) return;
    const wasDragging = isDragging;
    const fromIndex = dragIndex;
    let toIndex = dragOverIndex;
    dragIndex = null;
    dragOverIndex = null;
    isDragging = false;
    dragEl = null;

    if (!wasDragging || toIndex === null || toIndex === fromIndex || toIndex === fromIndex + 1) return;
    const reordered = [...currentRules];
    const [moved] = reordered.splice(fromIndex, 1);
    if (toIndex > fromIndex) toIndex--;
    reordered.splice(toIndex, 0, moved);
    await saveRules(reordered);
  }

  async function saveRule() {
    if (!ruleName || !rulePattern) return;
    
    let updatedRules;
    if (editingRule === '__new__') {
      const newRule = {
        id: 'rule-' + Date.now(),
        name: ruleName,
        pattern: rulePattern,
        matchType: ruleMatchType,
        foreground: ruleForeground,
        background: ruleBackground,
        bold: ruleBold,
        italic: ruleItalic,
        enabled: true,
        priority: currentRules.length
      };
      updatedRules = [...currentRules, newRule];
    } else {
      updatedRules = currentRules.map(r => {
        if (r.id === editingRule) {
          return { ...r, name: ruleName, pattern: rulePattern, matchType: ruleMatchType,
            foreground: ruleForeground, background: ruleBackground, bold: ruleBold,
            italic: ruleItalic };
        }
        return r;
      });
    }

    await saveRules(updatedRules);
    editingRule = null;
  }

  async function deleteRule(ruleId) {
    const updatedRules = currentRules.filter(r => r.id !== ruleId);
    await saveRules(updatedRules);
  }

  async function toggleRule(ruleId) {
    const updatedRules = currentRules.map(r => {
      if (r.id === ruleId) return { ...r, enabled: !r.enabled };
      return r;
    });
    await saveRules(updatedRules);
  }

  async function createProfile() {
    if (!newProfileName.trim()) return;
    const p = { name: newProfileName.trim(), rules: [] };
    profiles.update(all => ({ ...all, [p.name]: p }));
    await SaveProfile(p);
    updateSetting('activeProfile', p.name);
    showNewProfile = false;
    newProfileName = '';
  }

  async function deleteCurrentProfile() {
    if ($profileNames.length <= 1) return;
    await DeleteProfile(selectedProfile);
    profiles.update(all => {
      const copy = { ...all };
      delete copy[selectedProfile];
      return copy;
    });
    const remaining = $profileNames.find(n => n !== selectedProfile) || $profileNames[0];
    updateSetting('activeProfile', remaining);
  }
</script>

<div class="settings-panel">
  <div class="panel-header">
    <button class:active={activeSection === 'settings'} on:click={() => selectSection('settings')}>Settings</button>
    <button class:active={activeSection === 'rules'} on:click={() => selectSection('rules')}>Rules</button>
  </div>

  {#if activeSection === 'settings'}
    <div class="section">
      <label>
        <span>Poll Interval (ms)</span>
        <input type="number" min="100" max="10000" step="100"
          value={$settings.pollIntervalMs}
          on:change={e => updateSetting('pollIntervalMs', parseInt(e.target.value))} />
      </label>
      <label>
        <span>Scroll Buffer (lines)</span>
        <input type="number" min="100" max="5000" step="100"
          value={$settings.scrollBuffer || 500}
          on:change={e => updateSetting('scrollBuffer', parseInt(e.target.value))} />
      </label>
      <label>
        <span>Font Size</span>
        <input type="number" min="10" max="24"
          value={$settings.fontSize}
          on:change={e => updateSetting('fontSize', parseInt(e.target.value))} />
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.showLineNumbers}
          on:change={e => updateSetting('showLineNumbers', e.target.checked)} />
        <span>Show Line Numbers</span>
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.wordWrap}
          on:change={e => updateSetting('wordWrap', e.target.checked)} />
        <span>Word Wrap</span>
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.restoreTabs}
          on:change={e => updateSetting('restoreTabs', e.target.checked)} />
        <span>Restore Tabs on Startup</span>
      </label>
      <label>
        <span>Theme</span>
        <select value={$settings.theme}
          on:change={e => {
            updateSetting('theme', e.target.value);
            document.documentElement.setAttribute('data-theme', e.target.value);
          }}>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
    </div>

  {:else if activeSection === 'rules'}
    <div class="section">
      <div class="profile-selector">
        <select value={selectedProfile} on:change={e => updateSetting('activeProfile', e.target.value)}>
          {#each $profileNames as name}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <button class="icon-btn" on:click={() => showNewProfile = !showNewProfile} title="New profile">+</button>
        {#if $profileNames.length > 1}
          <button class="icon-btn danger" on:click={deleteCurrentProfile} title="Delete profile">🗑</button>
        {/if}
      </div>

      {#if showNewProfile}
        <div class="new-profile-form">
          <input type="text" placeholder="Profile name" bind:value={newProfileName} />
          <button class="btn-small" on:click={createProfile}>Create</button>
        </div>
      {/if}

      <p class="precedence-hint">Rules are applied top to bottom. Rules lower in the list take precedence over earlier ones.</p>

      <div class="rules-list">
        {#each currentRules as rule, index (rule.id)}
          {#if isDragging && dragOverIndex === index}
            <div class="drop-indicator"></div>
          {/if}
          <div class="rule-item" class:disabled={!rule.enabled} class:dragging={isDragging && dragIndex === index}
            on:pointerdown={e => handlePointerDown(e, index)}
            on:pointermove={handlePointerMove}
            on:pointerup={handlePointerUp}
            style="background: {rule.background || 'var(--bg-primary)'}; color: {rule.foreground || 'var(--text-primary)'}; {rule.bold ? 'font-weight:700;' : ''}{rule.italic ? 'font-style:italic;' : ''}">
            <div class="rule-header">
              <div class="rule-order-buttons">
                <button class="order-btn" on:click={() => moveRule(index, -1)} disabled={index === 0} title="Move up">▲</button>
                <button class="order-btn" on:click={() => moveRule(index, 1)} disabled={index === currentRules.length - 1} title="Move down">▼</button>
              </div>
              <input type="checkbox" checked={rule.enabled} on:change={() => toggleRule(rule.id)} />
              <span class="rule-name">{rule.name}</span>
              <span class="rule-type" style="color: var(--text-muted); background: {rule.background ? 'rgba(0,0,0,0.25)' : 'var(--bg-primary)'}">{rule.matchType}</span>
              <button class="icon-btn-small" on:click={() => startEditRule(rule)}>✏</button>
              <button class="icon-btn-small danger" on:click={() => deleteRule(rule.id)}>×</button>
            </div>
            <div class="rule-pattern">{rule.pattern}</div>
          </div>
        {/each}
        {#if isDragging && dragOverIndex >= currentRules.length}
          <div class="drop-indicator"></div>
        {/if}
      </div>

      <button class="btn-add-rule" on:click={startNewRule}>+ Add Rule</button>

      {#if editingRule}
        <div class="rule-editor">
          <h4>{editingRule === '__new__' ? 'New Rule' : 'Edit Rule'}</h4>
          <label>
            <span>Name</span>
            <input type="text" bind:value={ruleName} placeholder="Rule name" />
          </label>
          <label>
            <span>Pattern (regex)</span>
            <input type="text" bind:value={rulePattern} placeholder="e.g. \\bERROR\\b" />
          </label>
          <label>
            <span>Match Type</span>
            <select bind:value={ruleMatchType}>
              <option value="match">Match only</option>
              <option value="line">Entire line</option>
            </select>
          </label>
          <label>
            <span>Foreground</span>
            <div class="color-input">
              <input type="color" bind:value={ruleForeground} />
              <input type="text" bind:value={ruleForeground} />
            </div>
          </label>
          <label>
            <span>Background</span>
            <div class="color-input">
              <input type="color" bind:value={ruleBackground} />
              <input type="text" bind:value={ruleBackground} />
            </div>
          </label>
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ruleBold} />
            <span>Bold</span>
          </label>
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ruleItalic} />
            <span>Italic</span>
          </label>
          <div class="editor-actions">
            <button class="btn-save" on:click={saveRule}>Save</button>
            <button class="btn-cancel" on:click={cancelEdit}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .settings-panel {
    width: 320px;
    min-width: 320px;
    background: var(--bg-secondary);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .panel-header {
    display: flex;
    border-bottom: 1px solid var(--border);
  }

  .panel-header button {
    flex: 1;
    padding: 10px;
    font-weight: 600;
    color: var(--text-muted);
    border-bottom: 2px solid transparent;
  }

  .panel-header button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .panel-header button:hover {
    background: var(--bg-hover);
  }

  .section {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label span {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .toggle-label {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .profile-selector {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .profile-selector select {
    flex: 1;
  }

  .icon-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg-surface);
    font-size: 16px;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
  }

  .icon-btn.danger:hover {
    background: var(--danger);
    color: white;
  }

  .new-profile-form {
    display: flex;
    gap: 6px;
  }

  .new-profile-form input {
    flex: 1;
  }

  .btn-small {
    padding: 4px 10px;
    background: var(--accent);
    color: var(--bg-primary);
    border-radius: 4px;
    font-weight: 600;
    font-size: 12px;
  }

  .precedence-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin: 0;
    line-height: 1.4;
  }

  .rules-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .rule-item {
    border-radius: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    transition: opacity 0.15s;
    cursor: grab;
  }

  .rule-item.dragging {
    opacity: 0.4;
  }

  .rule-item:active {
    cursor: grabbing;
  }

  .drop-indicator {
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
    margin: -1px 0;
  }

  .rule-item.disabled {
    opacity: 0.5;
  }

  .rule-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .rule-order-buttons {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .order-btn {
    width: 18px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    border-radius: 2px;
    color: inherit;
    opacity: 0.6;
    background: rgba(128, 128, 128, 0.15);
    border: none;
    cursor: pointer;
    padding: 0;
    line-height: 1;
  }

  .order-btn:hover:not(:disabled) {
    opacity: 1;
    background: rgba(128, 128, 128, 0.3);
  }

  .order-btn:disabled {
    opacity: 0.2;
    cursor: default;
  }

  .rule-name {
    flex: 1;
    font-weight: 600;
    font-size: 12px;
  }

  .rule-type {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-primary);
    padding: 1px 6px;
    border-radius: 3px;
  }

  .rule-pattern {
    font-size: 11px;
    opacity: 0.7;
    font-family: monospace;
    margin-top: 4px;
    padding-left: 24px;
  }

  .icon-btn-small {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .icon-btn-small:hover {
    background: var(--bg-hover);
  }

  .icon-btn-small.danger:hover {
    background: var(--danger);
    color: white;
  }

  .btn-add-rule {
    padding: 8px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .btn-add-rule:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .rule-editor {
    background: var(--bg-surface);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }

  .rule-editor h4 {
    margin: 0 0 4px;
    font-size: 13px;
    color: var(--accent);
  }

  .color-input {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .color-input input[type="color"] {
    width: 32px;
    height: 28px;
    padding: 0;
    border: none;
    cursor: pointer;
  }

  .color-input input[type="text"] {
    flex: 1;
  }

  .editor-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-save {
    flex: 1;
    padding: 6px;
    background: var(--accent);
    color: var(--bg-primary);
    border-radius: 4px;
    font-weight: 600;
  }

  .btn-cancel {
    flex: 1;
    padding: 6px;
    background: var(--bg-hover);
    border-radius: 4px;
  }
</style>
