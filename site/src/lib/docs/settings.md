<script>
	import Shot from '$lib/components/Shot.svelte';
</script>

Open Settings with **ctail ▸ Settings…** or <kbd>⌘,</kbd>. Changes apply when you click **Save**.

<Shot src="settings.webp" alt="The Settings window" class="max-w-md" />

## Appearance

| Setting | Description | Default |
|---|---|---|
| Theme | One of the 21 built-in themes, or a custom theme | Catppuccin |
| Mode | Dark or Light | Dark |
| Font size | Log text size in points | 13 |
| Show line numbers | Gutter with line numbers | On |
| Word wrap | Wrap long lines instead of scrolling horizontally | Off |

Line numbers and word wrap can also be toggled from the View menu without opening Settings.

## Files and performance

| Setting | Description | Default |
|---|---|---|
| Poll interval (ms) | How often each open file is checked for changes. Raise it for slow network shares | 500 |
| Buffer size (lines) | Lines kept in memory around the visible area | 10 000 |
| Scrollback (lines) | Lines fetched from disk per page while scrolling | 500 |
| Read timeout (s) | How long a single read may take before the tab reports a problem | 30 |

## Tabs

| Setting | Description | Default |
|---|---|---|
| Restore tabs on launch | Reopen the previous session's files | On |
| New tab position | Open new tabs at the end of the bar or next to the current tab | End |

## Updates

| Setting | Description | Default |
|---|---|---|
| Disable update check | Stop checking GitHub for new releases | Off |
| Check interval (h) | Hours between automatic checks | 24 |

App Store installs are updated by the App Store; the check is mainly useful for source builds.

## AI

| Setting | Description |
|---|---|
| Provider | OpenAI, Anthropic, GitHub Models, GitHub Copilot, Custom, or none |
| Endpoint | Server URL. Leave empty for the provider default |
| API key | Key or token. Hidden for Copilot, which signs in with GitHub |
| Model | Model name. **Fetch** lists what the provider offers |

See [AI providers](../ai-providers/) for setup details.

## Window state

Window position and size are saved automatically and restored on launch.
