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
  let rulePriority = 50;

  export let selectedProfile = 'Common Logs';

  $: currentProfile = $profiles[selectedProfile];
  $: currentRules = currentProfile ? currentProfile.rules : [];

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
    rulePriority = rule.priority;
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
    rulePriority = 50;
  }

  function cancelEdit() {
    editingRule = null;
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
        priority: rulePriority
      };
      updatedRules = [...currentRules, newRule];
    } else {
      updatedRules = currentRules.map(r => {
        if (r.id === editingRule) {
          return { ...r, name: ruleName, pattern: rulePattern, matchType: ruleMatchType,
            foreground: ruleForeground, background: ruleBackground, bold: ruleBold,
            italic: ruleItalic, priority: rulePriority };
        }
        return r;
      });
    }

    const updated = { name: selectedProfile, rules: updatedRules };
    profiles.update(p => ({ ...p, [selectedProfile]: updated }));
    await SaveProfile(updated);
    editingRule = null;
  }

  async function deleteRule(ruleId) {
    const updatedRules = currentRules.filter(r => r.id !== ruleId);
    const updated = { name: selectedProfile, rules: updatedRules };
    profiles.update(p => ({ ...p, [selectedProfile]: updated }));
    await SaveProfile(updated);
  }

  async function toggleRule(ruleId) {
    const updatedRules = currentRules.map(r => {
      if (r.id === ruleId) return { ...r, enabled: !r.enabled };
      return r;
    });
    const updated = { name: selectedProfile, rules: updatedRules };
    profiles.update(p => ({ ...p, [selectedProfile]: updated }));
    await SaveProfile(updated);
  }

  async function createProfile() {
    if (!newProfileName.trim()) return;
    const p = { name: newProfileName.trim(), rules: [] };
    profiles.update(all => ({ ...all, [p.name]: p }));
    await SaveProfile(p);
    selectedProfile = p.name;
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
    selectedProfile = $profileNames.find(n => n !== selectedProfile) || $profileNames[0];
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
        <select bind:value={selectedProfile}>
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

      <div class="rules-list">
        {#each currentRules as rule (rule.id)}
          <div class="rule-item" class:disabled={!rule.enabled}>
            <div class="rule-header">
              <input type="checkbox" checked={rule.enabled} on:change={() => toggleRule(rule.id)} />
              <span class="rule-name" style="color: {rule.foreground}">{rule.name}</span>
              <span class="rule-type">{rule.matchType}</span>
              <button class="icon-btn-small" on:click={() => startEditRule(rule)}>✏</button>
              <button class="icon-btn-small danger" on:click={() => deleteRule(rule.id)}>×</button>
            </div>
            <div class="rule-pattern">{rule.pattern}</div>
          </div>
        {/each}
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
          <label>
            <span>Priority</span>
            <input type="number" min="0" max="1000" bind:value={rulePriority} />
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

  .rules-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .rule-item {
    background: var(--bg-surface);
    border-radius: 6px;
    padding: 8px;
  }

  .rule-item.disabled {
    opacity: 0.5;
  }

  .rule-header {
    display: flex;
    align-items: center;
    gap: 6px;
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
    color: var(--text-muted);
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
