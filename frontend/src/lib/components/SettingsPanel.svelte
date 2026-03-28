<script>
  import { settings } from '../stores/settings.js';
  import { profiles, profileNames } from '../stores/rules.js';
  import { SaveSettings, SaveProfile, DeleteProfile, GetProfile, ListThemes, StartCopilotAuth, CompleteCopilotAuth } from '../../../wailsjs/go/main/App.js';
  import { loadAndApplyTheme } from '../utils/themes.js';
  import { onMount } from 'svelte';
  import { BrowserOpenURL } from '../../../wailsjs/runtime/runtime.js';

  let activeSection = $state('settings');
  let editingRule = $state(null);
  let editingProfileName = $state('');
  let newProfileName = $state('');
  let showNewProfile = $state(false);
  let availableThemes = $state([]);

  // Copilot OAuth device flow state
  let copilotAuthState = $state('idle');
  let copilotUserCode = $state('');
  let copilotVerifyURL = $state('');
  let copilotError = $state('');

  async function startCopilotSignIn() {
    copilotAuthState = 'authorizing';
    copilotError = '';
    try {
      const dcr = await StartCopilotAuth();
      copilotUserCode = dcr.user_code;
      copilotVerifyURL = dcr.verification_uri;
      BrowserOpenURL(dcr.verification_uri);

      const ok = await CompleteCopilotAuth(dcr.device_code, dcr.interval);
      if (ok) {
        copilotAuthState = 'success';
        const { GetSettings } = await import('../../../wailsjs/go/main/App.js');
        const s = await GetSettings();
        settings.set(s);
      } else {
        copilotAuthState = 'error';
        copilotError = 'Authorization failed';
      }
    } catch (e) {
      copilotAuthState = 'error';
      copilotError = e?.message || String(e);
    }
  }

  function disconnectCopilot() {
    if (!confirm('Disconnect from GitHub Copilot?')) return;
    updateSetting('aiProvider', '');
    updateSetting('aiKey', '');
    copilotAuthState = 'idle';
    copilotUserCode = '';
  }

  onMount(async () => {
    try {
      availableThemes = await ListThemes();
      availableThemes.sort((a, b) => {
        if (a.builtIn !== b.builtIn) return b.builtIn ? 1 : -1;
        return (a.displayName || a.name).localeCompare(b.displayName || b.name);
      });
    } catch (e) {
      console.error('Failed to load themes:', e);
    }
  });

  // Rule editor state
  let ruleName = $state('');
  let rulePattern = $state('');
  let ruleMatchType = $state('match');
  let ruleForeground = $state('#89b4fa');
  let ruleBackground = $state('');
  let ruleBold = $state(false);
  let ruleItalic = $state(false);

  let selectedProfile = $derived($settings.activeProfile || 'Common Logs');
  let currentProfile = $derived($profiles[selectedProfile]);
  let currentRules = $derived(currentProfile
    ? [...currentProfile.rules].sort((a, b) => a.priority - b.priority)
    : []);

  // Drag state (pointer-based for WebKit compatibility)
  let dragIndex = $state(null);
  let dragOverIndex = $state(null);
  let isDragging = $state(false);
  let dragEl = $state(null);
  let dragStartY = $state(0);

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
    if (editingRule === rule.id) {
      editingRule = null;
      return;
    }
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

  function scrollIntoViewAction(node) {
    requestAnimationFrame(() => {
      node.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    });
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
    const rule = currentRules.find(r => r.id === ruleId);
    if (!confirm(`Delete rule "${rule?.name || ruleId}"?`)) return;
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
    if (!confirm(`Delete profile "${selectedProfile}"? This cannot be undone.`)) return;
    await DeleteProfile(selectedProfile);
    profiles.update(all => {
      const copy = { ...all };
      delete copy[selectedProfile];
      return copy;
    });
    const remaining = $profileNames.find(n => n !== selectedProfile) || $profileNames[0];
    updateSetting('activeProfile', remaining);
  }

  function handlePanelWheel(e) {
    if ($settings.smoothScroll) return;
    if (e.deltaX !== 0 && e.deltaY === 0) return;
    e.preventDefault();
    const panel = e.currentTarget;
    panel.scrollTop += e.deltaY * ($settings.scrollSpeed || 1);
  }
</script>

<div class="settings-panel" onwheel={handlePanelWheel}>
  <div class="panel-header">
    <button class:active={activeSection === 'settings'} onclick={() => selectSection('settings')}>Settings</button>
    <button class:active={activeSection === 'rules'} onclick={() => selectSection('rules')}>Rules</button>
    <button class:active={activeSection === 'ai'} onclick={() => selectSection('ai')}>AI</button>
  </div>

  {#if activeSection === 'settings'}
    <div class="section">
      <label>
        <span>Poll Interval (ms)</span>
        <input type="number" min="100" max="10000" step="100"
          value={$settings.pollIntervalMs}
          onchange={e => updateSetting('pollIntervalMs', parseInt(e.target.value))} />
      </label>
      <label>
        <span>Scroll Speed</span>
        <input type="range" min="1" max="10" step="1"
          value={$settings.scrollSpeed || 1}
          oninput={e => updateSetting('scrollSpeed', parseInt(e.target.value))} />
        <span class="range-value">{$settings.scrollSpeed || 1}</span>
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.smoothScroll}
          onchange={e => updateSetting('smoothScroll', e.target.checked)} />
        <span>Smooth Scroll (deceleration at edges)</span>
      </label>
      <label>
        <span>Font Size</span>
        <input type="number" min="10" max="24"
          value={$settings.fontSize}
          onchange={e => updateSetting('fontSize', parseInt(e.target.value))} />
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.showLineNumbers}
          onchange={e => updateSetting('showLineNumbers', e.target.checked)} />
        <span>Show Line Numbers</span>
      </label>
      <label>
        <span>Search Highlight Color</span>
        <div class="color-input-row">
          <input type="color"
            value={$settings.searchHighlightColor || '#ffd54f'}
            onchange={e => updateSetting('searchHighlightColor', e.target.value)} />
          <button class="reset-btn" title="Reset to theme default"
            onclick={() => updateSetting('searchHighlightColor', '')}>Reset</button>
        </div>
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.wordWrap}
          onchange={e => updateSetting('wordWrap', e.target.checked)} />
        <span>Word Wrap</span>
      </label>
      <label class="toggle-label">
        <input type="checkbox" checked={$settings.restoreTabs}
          onchange={e => updateSetting('restoreTabs', e.target.checked)} />
        <span>Restore Tabs on Startup</span>
      </label>
      <label>
        <span>Theme</span>
        <select value={$settings.theme || 'catppuccin'}
          onchange={async (e) => {
            const themeName = e.target.value;
            const mode = $settings.themeMode || 'dark';
            updateSetting('theme', themeName);
            await loadAndApplyTheme(themeName, mode);
          }}>
          {#each availableThemes as theme}
            <option value={theme.name}>{theme.displayName || theme.name}{theme.builtIn ? '' : ' ✦'}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Mode</span>
        <select value={$settings.themeMode || 'dark'}
          onchange={async (e) => {
            const mode = e.target.value;
            const themeName = $settings.theme || 'catppuccin';
            updateSetting('themeMode', mode);
            await loadAndApplyTheme(themeName, mode);
          }}>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      {#if navigator.platform?.toLowerCase().includes('linux')}
      <label>
        <span>Display Backend <small>(requires restart)</small></span>
        <select value={$settings.displayBackend || 'auto'}
          onchange={(e) => updateSetting('displayBackend', e.target.value)}>
          <option value="auto">Auto-detect</option>
          <option value="x11">X11</option>
          <option value="wayland">Wayland</option>
        </select>
      </label>
      <label>
        <span>GPU Rendering <small>(requires restart)</small></span>
        <select value={$settings.gpuPolicy || 'auto'}
          onchange={(e) => updateSetting('gpuPolicy', e.target.value)}>
          <option value="auto">Auto (GPU accelerated)</option>
          <option value="software">Software rendering</option>
        </select>
      </label>
      {/if}
      <label class="toggle-label">
        <input type="checkbox" checked={!$settings.disableUpdateCheck}
          onchange={(e) => updateSetting('disableUpdateCheck', !e.target.checked)} />
        <span>Check for updates automatically</span>
      </label>
      {#if !$settings.disableUpdateCheck}
        <label>
          <span>Update check interval</span>
          <select value={$settings.updateCheckIntervalHours || 24}
            onchange={(e) => updateSetting('updateCheckIntervalHours', parseInt(e.target.value))}>
            <option value={1}>Every hour</option>
            <option value={6}>Every 6 hours</option>
            <option value={12}>Every 12 hours</option>
            <option value={24}>Every 24 hours</option>
            <option value={72}>Every 3 days</option>
            <option value={168}>Every week</option>
          </select>
        </label>
      {/if}
    </div>

  {:else if activeSection === 'rules'}
    <div class="section">
      <div class="profile-selector">
        <select value={selectedProfile} onchange={e => updateSetting('activeProfile', e.target.value)}>
          {#each $profileNames as name}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <button class="icon-btn" onclick={() => showNewProfile = !showNewProfile} title="New profile">+</button>
        {#if $profileNames.length > 1}
          <button class="icon-btn danger" onclick={deleteCurrentProfile} title="Delete profile">🗑</button>
        {/if}
      </div>

      {#if showNewProfile}
        <div class="new-profile-form">
          <input type="text" placeholder="Profile name" bind:value={newProfileName} />
          <button class="btn-small" onclick={createProfile}>Create</button>
        </div>
      {/if}

      <p class="precedence-hint">Rules are applied top to bottom. Rules lower in the list take precedence over earlier ones.</p>

      <div class="rules-list">
        {#each currentRules as rule, index (rule.id)}
          {#if isDragging && dragOverIndex === index}
            <div class="drop-indicator"></div>
          {/if}
          <div class="rule-item" class:disabled={!rule.enabled} class:dragging={isDragging && dragIndex === index}
            onpointerdown={e => handlePointerDown(e, index)}
            onpointermove={handlePointerMove}
            onpointerup={handlePointerUp}
            style="background: {rule.background || 'var(--bg-primary)'}; color: {rule.foreground || 'var(--text-primary)'}; {rule.bold ? 'font-weight:700;' : ''}{rule.italic ? 'font-style:italic;' : ''}">
            <div class="rule-header">
              <div class="rule-order-buttons">
                <button class="order-btn" onclick={() => moveRule(index, -1)} disabled={index === 0} title="Move up">▲</button>
                <button class="order-btn" onclick={() => moveRule(index, 1)} disabled={index === currentRules.length - 1} title="Move down">▼</button>
              </div>
              <input type="checkbox" checked={rule.enabled} onchange={() => toggleRule(rule.id)} />
              <span class="rule-name">{rule.name}</span>
              <span class="rule-type" style="color: var(--text-muted); background: {rule.background ? 'rgba(0,0,0,0.25)' : 'var(--bg-primary)'}">{rule.matchType}</span>
              <button class="edit-btn" onclick={() => startEditRule(rule)} title="Edit rule">✏ Edit</button>
              <button class="icon-btn-small danger" onclick={() => deleteRule(rule.id)}>×</button>
            </div>
            <div class="rule-pattern">{rule.pattern}</div>
          </div>
          {#if editingRule === rule.id}
            <div class="rule-editor" use:scrollIntoViewAction>
              <h4>Edit Rule</h4>
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
                  <input type="color" value={ruleForeground || '#ffffff'} oninput={e => ruleForeground = e.target.value} />
                  <input type="text" bind:value={ruleForeground} placeholder="transparent" />
                  <button class="color-clear" onclick={() => ruleForeground = ''} title="Clear (transparent)">✕</button>
                </div>
              </label>
              <label>
                <span>Background</span>
                <div class="color-input">
                  <input type="color" value={ruleBackground || '#000000'} oninput={e => ruleBackground = e.target.value} />
                  <input type="text" bind:value={ruleBackground} placeholder="transparent" />
                  <button class="color-clear" onclick={() => ruleBackground = ''} title="Clear (transparent)">✕</button>
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
                <button class="btn-save" onclick={saveRule}>Save</button>
                <button class="btn-cancel" onclick={cancelEdit}>Cancel</button>
              </div>
            </div>
          {/if}
        {/each}
        {#if isDragging && dragOverIndex >= currentRules.length}
          <div class="drop-indicator"></div>
        {/if}
      </div>

      <button class="btn-add-rule" onclick={startNewRule}>+ Add Rule</button>
      <button class="btn-add-rule ai-generate" onclick={() => window.dispatchEvent(new CustomEvent('ctail:open-ai'))} title="Use AI to generate rules from your logs">🤖 AI Generate Rules</button>

      {#if editingRule === '__new__'}
        <div class="rule-editor" use:scrollIntoViewAction>
          <h4>New Rule</h4>
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
              <input type="color" value={ruleForeground || '#ffffff'} oninput={e => ruleForeground = e.target.value} />
              <input type="text" bind:value={ruleForeground} placeholder="transparent" />
              <button class="color-clear" onclick={() => ruleForeground = ''} title="Clear (transparent)">✕</button>
            </div>
          </label>
          <label>
            <span>Background</span>
            <div class="color-input">
              <input type="color" value={ruleBackground || '#000000'} oninput={e => ruleBackground = e.target.value} />
              <input type="text" bind:value={ruleBackground} placeholder="transparent" />
              <button class="color-clear" onclick={() => ruleBackground = ''} title="Clear (transparent)">✕</button>
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
            <button class="btn-save" onclick={saveRule}>Save</button>
            <button class="btn-cancel" onclick={cancelEdit}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>

  {:else if activeSection === 'ai'}
    <div class="section">
      <label>
        <span>AI Provider</span>
        <select value={$settings.aiProvider || ''}
          onchange={e => updateSetting('aiProvider', e.target.value)}>
          <option value="">Not configured</option>
          <option value="openai">OpenAI</option>
          <option value="copilot">GitHub Copilot</option>
          <option value="github">GitHub Models (PAT)</option>
          <option value="custom">Custom (OpenAI-compatible)</option>
        </select>
      </label>

      {#if $settings.aiProvider}
        {#if $settings.aiProvider === 'copilot'}
          <!-- Copilot: device flow sign-in -->
          {#if $settings.aiKey && copilotAuthState !== 'authorizing'}
            <div class="copilot-status">
              <span class="copilot-connected">✓ Connected to Copilot</span>
              <button class="btn-small btn-danger" onclick={disconnectCopilot}>Disconnect</button>
            </div>
            <label>
              <span>Model <small>(leave empty for default: gpt-4o)</small></span>
              <input type="text" placeholder="gpt-4o"
                value={$settings.aiModel || ''}
                onchange={e => updateSetting('aiModel', e.target.value.trim())} />
            </label>
          {:else if copilotAuthState === 'authorizing'}
            <div class="copilot-code">
              <p>Enter this code on GitHub:</p>
              <div class="code-row">
                <span class="user-code">{copilotUserCode}</span>
                <button class="btn-copy" title="Copy code" onclick={() => { navigator.clipboard.writeText(copilotUserCode); }}>📋</button>
              </div>
              <p class="copilot-hint">Waiting for authorization…</p>
            </div>
          {:else}
            <button class="btn-copilot" onclick={startCopilotSignIn}>
              Sign in with GitHub
            </button>
            <p class="ai-settings-hint-small">Requires an active <a href="https://github.com/features/copilot" target="_blank" rel="noopener noreferrer">Copilot subscription</a>.</p>
          {/if}
          {#if copilotAuthState === 'error'}
            <p class="ai-error">{copilotError}</p>
          {/if}
        {:else if $settings.aiProvider === 'github'}
          <label>
            <span>GitHub Personal Access Token</span>
            <input type="password" placeholder="ghp_..."
              value={$settings.aiKey || ''}
              onchange={e => updateSetting('aiKey', e.target.value.trim())} />
          </label>
          <p class="ai-settings-hint-small">
            Create a <a href="https://github.com/settings/tokens?type=beta" target="_blank" rel="noopener noreferrer">PAT</a> with <code>models:read</code> scope.
          </p>
          <label>
            <span>Model <small>(leave empty for default)</small></span>
            <input type="text" placeholder="gpt-4o-mini"
              value={$settings.aiModel || ''}
              onchange={e => updateSetting('aiModel', e.target.value.trim())} />
          </label>
        {:else}
          <label>
            <span>API Endpoint {#if $settings.aiProvider === 'openai'}<small>(default: api.openai.com)</small>{/if}</span>
            <input type="text" placeholder={$settings.aiProvider === 'openai' ? 'api.openai.com' : 'https://your-server.com'}
              value={$settings.aiEndpoint || ''}
              onchange={e => updateSetting('aiEndpoint', e.target.value.trim())} />
          </label>
          <label>
            <span>API Key</span>
            <input type="password" placeholder="sk-..."
              value={$settings.aiKey || ''}
              onchange={e => updateSetting('aiKey', e.target.value.trim())} />
          </label>
          <label>
            <span>Model <small>(leave empty for default)</small></span>
            <input type="text" placeholder="gpt-4o-mini"
              value={$settings.aiModel || ''}
              onchange={e => updateSetting('aiModel', e.target.value.trim())} />
          </label>
        {/if}
        <p class="ai-settings-hint">Use <strong>Ctrl+Shift+A</strong> to open the AI assistant dialog.</p>
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
    overscroll-behavior: none;
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

  label input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }

  .range-value {
    font-size: 11px;
    color: var(--text-muted);
    text-align: right;
  }

  .color-input-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .color-input-row input[type="color"] {
    width: 36px;
    height: 28px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-primary);
    cursor: pointer;
  }

  .reset-btn {
    font-size: 11px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .reset-btn:hover {
    background: var(--bg-hover);
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

  .edit-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    border: 1px solid var(--accent);
    background: transparent;
    cursor: pointer;
    white-space: nowrap;
  }

  .edit-btn:hover {
    background: var(--accent);
    color: var(--bg-primary);
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

  .btn-add-rule.ai-generate {
    margin-top: 4px;
    border-style: solid;
    border-color: var(--accent);
    opacity: 0.7;
  }

  .btn-add-rule.ai-generate:hover {
    opacity: 1;
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

  .color-clear {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-surface);
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .color-clear:hover {
    background: var(--bg-hover);
    color: var(--red, #f38ba8);
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

  .ai-settings-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin: 4px 0 0;
    line-height: 1.4;
  }

  .ai-settings-hint-small {
    font-size: 11px;
    color: var(--text-muted);
    margin: 2px 0 4px;
    line-height: 1.4;
  }
  .ai-settings-hint-small a {
    color: var(--accent);
  }
  .ai-settings-hint-small code {
    background: var(--bg-primary);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 10px;
  }

  .btn-copilot {
    padding: 8px 12px;
    background: var(--accent);
    color: var(--bg-primary);
    border-radius: 6px;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    border: none;
  }

  .btn-copilot:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .copilot-status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .copilot-connected {
    font-size: 12px;
    color: var(--green, #a6e3a1);
    font-weight: 600;
  }

  .copilot-code {
    text-align: center;
    padding: 10px;
    background: var(--bg-surface);
    border-radius: 6px;
    border: 1px solid var(--border);
  }

  .copilot-code p {
    margin: 0 0 6px;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .code-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .btn-copy {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
  }

  .btn-copy:hover {
    background: var(--bg-hover, var(--border));
  }

  .user-code {
    font-size: 20px;
    font-weight: 700;
    font-family: monospace;
    letter-spacing: 3px;
    color: var(--accent);
  }

  .copilot-hint {
    margin-top: 8px !important;
    color: var(--text-muted) !important;
    font-size: 10px !important;
  }

  .ai-error {
    color: var(--red, #f38ba8);
    font-size: 11px;
    margin: 4px 0;
  }

</style>
