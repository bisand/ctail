<script>
  import { activeTab } from '../stores/tabs.js';
  import { AskAI, GenerateRulesProfile, AskAIRules } from '../../../wailsjs/go/main/App.js';
  import { profiles, profileNames } from '../stores/rules.js';
  import { settings } from '../stores/settings.js';

  export let show = false;

  let question = '';
  let response = '';
  let loading = false;
  let error = '';
  let contextMode = 'buffer'; // "buffer", "selection", "last"
  let lastLineCount = 200;

  // Rule generation
  let generateProfileName = '';
  let generatingRules = false;
  let generateError = '';
  let generateSuccess = '';

  // Rules assistant
  let rulesQuestion = '';
  let rulesLoading = false;
  let rulesError = '';
  let rulesSuccess = '';

  function close() {
    show = false;
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') close();
    if (e.key === 'Enter' && !e.shiftKey && question.trim()) {
      e.preventDefault();
      askQuestion();
    }
  }

  function handleRulesKeydown(e) {
    if (e.key === 'Escape') close();
    if (e.key === 'Enter' && !e.shiftKey && rulesQuestion.trim()) {
      e.preventDefault();
      askRulesQuestion();
    }
  }

  async function askQuestion() {
    if (!question.trim()) return;
    const tab = $activeTab;
    if (!tab) {
      error = 'No active tab — open a log file first.';
      return;
    }

    loading = true;
    error = '';
    response = '';

    try {
      const startLine = 0;
      const lineCount = contextMode === 'last' ? lastLineCount : 0;
      response = await AskAI(tab.id, question.trim(), contextMode, startLine, lineCount);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function generateRules() {
    const name = generateProfileName.trim();
    if (!name) {
      generateError = 'Enter a profile name.';
      return;
    }
    const tab = $activeTab;
    if (!tab) {
      generateError = 'No active tab — open a log file first.';
      return;
    }

    generatingRules = true;
    generateError = '';
    generateSuccess = '';

    try {
      const profile = await GenerateRulesProfile(tab.id, name);
      profiles.update(p => ({ ...p, [name]: profile }));
      settings.update(s => ({ ...s, activeProfile: name }));
      generateSuccess = `Created profile "${name}" with ${profile.rules?.length || 0} rules.`;
      generateProfileName = '';
    } catch (e) {
      generateError = String(e);
    } finally {
      generatingRules = false;
    }
  }

  async function askRulesQuestion() {
    if (!rulesQuestion.trim()) return;

    rulesLoading = true;
    rulesError = '';
    rulesSuccess = '';

    try {
      const profile = await AskAIRules(rulesQuestion.trim());
      const name = profile.name || $settings.activeProfile;
      profiles.update(p => ({ ...p, [name]: profile }));
      settings.update(s => ({ ...s, activeProfile: name }));
      rulesSuccess = `Updated profile "${name}" — ${profile.rules?.length || 0} rules.`;
      rulesQuestion = '';
    } catch (e) {
      rulesError = String(e);
    } finally {
      rulesLoading = false;
    }
  }
</script>

{#if show}
  <div class="ai-overlay" on:click|self={close} on:keydown={handleKeydown}>
    <div class="ai-dialog">
      <div class="ai-header">
        <h3>🤖 AI Assistant</h3>
        <button class="close-btn" on:click={close}>✕</button>
      </div>

      {#if !$settings.aiProvider || !$settings.aiKey}
        <div class="ai-unconfigured">
          <p>AI is not configured. Set your AI provider and API key in <strong>Settings</strong>.</p>
        </div>
      {:else}
        <div class="ai-body">
        <div class="ai-section">
          <h4>Ask about logs</h4>
          <div class="context-selector">
            <label>
              <input type="radio" bind:group={contextMode} value="buffer" /> Current buffer
            </label>
            <label>
              <input type="radio" bind:group={contextMode} value="last" /> Last
              <input type="number" class="line-count" min="10" max="5000"
                bind:value={lastLineCount} disabled={contextMode !== 'last'} /> lines
            </label>
          </div>
          <div class="ask-row">
            <input type="text" class="question-input" placeholder="Ask a question about the logs..."
              bind:value={question} on:keydown={handleKeydown} disabled={loading} />
            <button class="btn-ask" on:click={askQuestion} disabled={loading || !question.trim()}>
              {loading ? '⏳' : 'Ask'}
            </button>
          </div>

          {#if error}
            <div class="ai-error">{error}</div>
          {/if}

          {#if response}
            <div class="ai-response">{response}</div>
          {/if}
        </div>

        <div class="ai-divider"></div>

        <div class="ai-section">
          <h4>Rules Assistant</h4>
          <p class="ai-hint">Ask AI to add, modify, or delete highlight rules in the active profile. All open files are included as context.</p>
          <div class="ask-row">
            <input type="text" class="question-input" placeholder="e.g. Add red foreground to all ERROR events..."
              bind:value={rulesQuestion} on:keydown={handleRulesKeydown} disabled={rulesLoading} />
            <button class="btn-ask" on:click={askRulesQuestion}
              disabled={rulesLoading || !rulesQuestion.trim()}>
              {rulesLoading ? '⏳' : 'Apply'}
            </button>
          </div>

          {#if rulesError}
            <div class="ai-error">{rulesError}</div>
          {/if}

          {#if rulesSuccess}
            <div class="ai-success">{rulesSuccess}</div>
          {/if}
        </div>

        <div class="ai-divider"></div>

        <div class="ai-section">
          <h4>Generate Rules Profile</h4>
          <p class="ai-hint">AI will analyze the current log buffer and create highlighting rules.</p>
          <div class="ask-row">
            <input type="text" placeholder="New profile name..."
              bind:value={generateProfileName} disabled={generatingRules} />
            <button class="btn-ask" on:click={generateRules}
              disabled={generatingRules || !generateProfileName.trim()}>
              {generatingRules ? '⏳' : 'Generate'}
            </button>
          </div>

          {#if generateError}
            <div class="ai-error">{generateError}</div>
          {/if}

          {#if generateSuccess}
            <div class="ai-success">{generateSuccess}</div>
          {/if}
        </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .ai-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .ai-dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 12px;
    width: 560px;
    max-width: 90vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .ai-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .ai-header h3 {
    margin: 0;
    font-size: 14px;
    color: var(--text-primary);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 16px;
    cursor: pointer;
    padding: 0 4px;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }

  .ai-unconfigured {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .ai-body {
    overflow-y: auto;
    flex: 1;
  }

  .ai-section {
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .ai-section h4 {
    margin: 0;
    font-size: 12px;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .context-selector {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .context-selector label {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-direction: row;
  }

  .line-count {
    width: 60px;
    text-align: center;
  }

  .ask-row {
    display: flex;
    gap: 6px;
  }

  .question-input {
    flex: 1;
  }

  .btn-ask {
    padding: 6px 14px;
    background: var(--accent);
    color: var(--bg-primary);
    border-radius: 4px;
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
  }

  .btn-ask:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ai-hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-muted);
  }

  .ai-error {
    padding: 8px;
    border-radius: 4px;
    background: rgba(255, 100, 100, 0.1);
    border: 1px solid rgba(255, 100, 100, 0.3);
    color: var(--red, #f38ba8);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .ai-success {
    padding: 8px;
    border-radius: 4px;
    background: rgba(100, 255, 100, 0.1);
    border: 1px solid rgba(100, 255, 100, 0.3);
    color: var(--green, #a6e3a1);
    font-size: 12px;
  }

  .ai-response {
    padding: 10px;
    border-radius: 6px;
    background: var(--bg-surface);
    color: var(--text-primary);
    font-size: 12px;
    line-height: 1.5;
    max-height: 300px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono, monospace);
  }

  .ai-divider {
    height: 1px;
    background: var(--border);
    margin: 0 16px;
  }
</style>
