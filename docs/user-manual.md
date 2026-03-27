# ctail User Manual

**ctail** (color tail) is a cross-platform desktop log file viewer with real-time tailing and regex-based color highlighting.

## Table of Contents

- [Getting Started](#getting-started)
- [Opening Files](#opening-files)
- [Recent Files](#recent-files)
- [Tabs](#tabs)
- [Following & Scrolling](#following--scrolling)
- [Search](#search)
- [Highlighting Rules](#highlighting-rules)
- [Rule Profiles](#rule-profiles)
- [AI Assistant](#ai-assistant)
- [Settings](#settings)
- [Themes](#themes)
- [Menu Bar](#menu-bar)
- [Context Menus](#context-menus)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Check for Updates](#check-for-updates)
- [Linux Installation](#linux-installation)
- [Configuration Files](#configuration-files)
- [Troubleshooting](#troubleshooting)

---

## Getting Started

ctail is a cross-platform desktop log viewer. Launch the application and open any text or log file to start tailing it in real-time.

On first launch, ctail creates a default configuration with the "Common Logs" highlighting profile and sensible defaults. If you had files open previously, they are automatically restored.

### Command-Line Flags

| Flag | Description |
|------|-------------|
| `--x11` | Force the X11 backend (Linux only). |
| `--wayland` | Force the native Wayland backend (Linux only). |
| `--software-render` | Disable GPU compositing entirely (Linux only). Fixes rendering corruption on some hardware. |
| `--disable-dmabuf` | Disable DMA-BUF renderer only (Linux only). A lighter alternative to `--software-render`. |

When no display flag is given, GTK auto-detects the native display backend. Display backend and GPU rendering can also be configured persistently in Settings (requires restart).

## Opening Files

- Press **Ctrl+O** or use **File → Open** from the menu bar to open the native file dialog.
- The file dialog opens at the directory of the currently active tab (if one is open).
- Select any text file — it opens in a new tab and immediately starts tailing.
- Files on network mounts (NFS, CIFS, SSHFS) are supported. If the connection is slow or unavailable, the tab shows a warning indicator and the UI remains responsive.

## Recent Files

The **File → Open Recent** menu shows the last 10 files you opened. Click any entry to reopen it. Use **Clear Recent Files** at the bottom of the submenu to reset the list.

Recent files are persisted across application restarts.

## Tabs

Each open file gets its own tab in the tab bar at the top of the window.

- **Switch tabs** by clicking on them or pressing **Ctrl+Tab** (next) / **Ctrl+Shift+Tab** (previous).
- **Toggle between tabs** — A quick Ctrl+Tab press (release, then press again) toggles between the two most recently active tabs.
- **Reorder tabs** — Drag and drop tabs to rearrange them.
- **Rename tabs** — Double-click a tab or use the right-click context menu to give it a custom label.
- **Color-code tabs** — Right-click a tab and choose **Set color** to assign one of 9 colors (red, orange, yellow, green, cyan, blue, purple, pink) for visual organization.
- **Close a tab** by clicking the × button or pressing **Ctrl+W**.
- **Right-click context menu** — Right-click a tab for Close, Close Others, Close to the Right, Refresh, Change file path, Copy file path, and Reveal in file manager.
- **Warning indicator** — Tabs show a ⚠ icon when the file is unreachable (e.g., network outage). The indicator clears automatically when the file becomes accessible again.
- **Update badge** — Inactive tabs show an update dot when new lines arrive.
- **File rotation** — ctail automatically detects log file rotation (when the file is replaced with a new file at the same path) and seamlessly switches to the new file.
- **Tab persistence** — Open tabs are saved automatically. If the application is closed (or force-killed), tabs are restored on next launch. This can be toggled in Settings.

## Following & Scrolling

### Follow Mode

When a tab is in Follow mode, new lines are automatically appended and the view scrolls to the bottom — like `tail -f`. The Follow checkbox is in the status bar at the bottom of each tab.

- **Auto-enable**: Follow turns on automatically when you scroll to the end of the file.
- **Auto-disable**: Follow turns off when you scroll up, letting you inspect earlier log entries without interruption.
- While Follow is off, new lines are still counted (shown in the status bar) but not loaded into the view.

### Scroll Buffer

ctail uses a sliding window buffer to keep memory usage low. A configurable number of lines (default 10,000) are held in memory at any time.

- **Scrolling up** loads earlier lines from the file when you reach the upper portion of the buffer.
- **Scrolling down** loads later lines when you reach the lower portion.
- The status bar shows your current position in the file (e.g., "Lines 1,200 – 1,700 of 50,000").
- The scroll buffer size is configurable in Settings (100–5,000 lines).

### Horizontal Scrolling

Long lines extend beyond the viewport. Scroll horizontally to read them, or enable **Word Wrap** in Settings.

## Search

Press **Ctrl+F** to open the inline search bar at the top of the log view. The search bar provides VS Code-style functionality with several powerful options.

### Search Toggles

| Toggle | Label | Description |
|--------|-------|-------------|
| **Aa** | Case sensitive | Match exact letter casing |
| **ab** | Whole word | Only match complete words (adds `\b` word boundaries) |
| **.\*** | Regex | Treat the query as a regular expression |

### Match Navigation

The search bar shows a match counter (e.g., "3 of 42") indicating which match is currently highlighted and how many total matches exist.

- **Enter** or **↓ button** — Jump to the next match
- **Shift+Enter** or **↑ button** — Jump to the previous match
- Navigation wraps around at the beginning and end of the file

### Filter Mode

Click the **≡ filter** button to switch between two modes:

- **Search mode** (default) — All lines are shown; matching text is highlighted with a yellow background
- **Filter mode** — Only lines containing a match are displayed; non-matching lines are hidden

### Behavior

- Opening search with text selected pre-fills the search query
- The search input auto-focuses when opened
- Press **Escape** or the **× button** to close the search bar and clear the query
- Search highlighting is applied on top of any highlighting rules

## Highlighting Rules

Rules colorize log output based on regex patterns. Each rule has:

| Property | Description |
|----------|-------------|
| **Name** | Display name shown in the rule list |
| **Pattern** | Regular expression (Go/PCRE-style syntax, e.g., `\bERROR\b`) |
| **Match Type** | **"Match only"** highlights the matched text; **"Entire line"** colors the whole line |
| **Foreground** | Text color (hex, e.g., `#ff6b6b`) |
| **Background** | Background color (hex, optional) |
| **Bold / Italic** | Text style |

### Rule Priority

Rules are displayed in a list. **Rules lower in the list take precedence** over rules higher up. When multiple rules match the same text:

- A higher-priority "entire line" rule overrides a lower-priority one.
- A "match only" rule only applies if its priority is equal to or greater than the active line rule.

### Editing Rules

1. Open the Settings panel (gear icon or **View → Toggle Settings**).
2. Switch to the **Rules** tab.
3. Click **+ Add Rule** to create a new rule, or click the ✏ button to edit an existing one.
4. Fill in the pattern, colors, and match type.
5. Click **Save**.

### Reordering Rules

- Use the **▲/▼ arrow buttons** on each rule to move it up or down.
- **Drag and drop** — click and hold a rule, then drag it to a new position. A blue indicator line shows where it will land.

### Visual Preview

Rule list items display with their configured colors (foreground, background, bold, italic) so you can see at a glance what each rule looks like against the editor background.

## Rule Profiles

Profiles are named collections of highlighting rules. You can create multiple profiles for different log formats.

- **Select a profile** from the dropdown in the Rules tab. The selected profile applies to all open tabs.
- **Create a profile** by clicking the **+** button and entering a name.
- **Delete a profile** by clicking the 🗑 button (at least one profile must remain).
- The active profile is saved and restored across application restarts.

### Default Profile: "Common Logs"

Ships with rules for common log patterns:

| Rule | Pattern | Type | Style |
|------|---------|------|-------|
| Fatal | `\bFATAL\b` | Entire line | White on red, bold |
| Error | `\bERROR\b` | Entire line | Red on dark red, bold |
| Warning | `\bWARN(ING)?\b` | Entire line | Yellow on dark yellow |
| Info | `\bINFO?\b` | Match only | Blue |
| Debug | `\bDEBUG\b` | Match only | Gray |
| Timestamp | `\d{4}-\d{2}-\d{2}T?\d{2}:\d{2}:\d{2}` | Match only | Green |

## AI Assistant

ctail includes an optional AI assistant that can analyze log content and generate highlighting rule profiles. No AI processing happens unless you explicitly ask.

### Opening the AI Dialog

- **Menu bar**: Tools → AI Assistant... (**Ctrl+Shift+A**)
- **Context menu**: Right-click in the log view → "🤖 Ask AI about logs"

### Supported Providers

| Provider | Auth | Notes |
|----------|------|-------|
| **GitHub Copilot** | OAuth sign-in (browser) | Requires active Copilot subscription |
| **GitHub Models** | Personal Access Token | Free tier available |
| **OpenAI** | API key | Pay-per-use |
| **Custom** | API key or none | Any OpenAI-compatible server (Ollama, LM Studio, etc.) |

Configure your provider in **Settings → AI**.

### Asking About Logs

Select what log context to send (last N lines, selected text, or full file), type your question, and press **Ask**. The AI sees the raw log text and responds in the dialog.

### AI-Generated Rule Profiles

Click **🤖 AI Generate Rules** in the Rules tab to have the AI analyze your current log file and create a complete highlighting profile with patterns, colors, and priority ordering.

📖 For detailed setup instructions, see the **[AI Assistant Guide](ai-assistant.md)**.

## Settings

Open the Settings panel (gear icon or **View → Toggle Settings**) to configure:

| Setting | Description | Default |
|---------|-------------|---------|
| **Poll Interval** | How often to check files for changes (ms) | 500 |
| **Scroll Buffer** | Lines kept in memory while scrolling (100–10,000) | 10,000 |
| **Scroll Speed** | Scroll acceleration multiplier (1–10) | 1 |
| **Smooth Scroll** | Smooth deceleration at scroll edges | Off |
| **Font Size** | Log text font size (10–24) | 14 |
| **Show Line Numbers** | Display line numbers in the gutter | Off |
| **Word Wrap** | Wrap long lines instead of horizontal scrolling | Off |
| **Restore Tabs** | Reopen previously open files on startup | On |
| **Theme** | Color theme (21 built-in themes + custom) | Catppuccin |
| **Theme Mode** | Dark or Light variant of the selected theme | Dark |

### Linux-Only Settings

These settings are only shown on Linux and require an application restart to take effect.

| Setting | Description | Default |
|---------|-------------|---------|
| **Display Backend** | Display server: Auto-detect, X11, or Wayland | Auto-detect |
| **GPU Rendering** | Auto (GPU accelerated) or Software rendering | Auto |

### Update Settings

| Setting | Description | Default |
|---------|-------------|---------|
| **Check for updates automatically** | Periodically check GitHub for newer releases | On |
| **Update check interval** | How often to check (hourly, 6h, 12h, daily, 3 days, weekly) | 24 hours |

### Window State

The application window position, size, and maximised state are automatically saved and restored between sessions.

## Themes

ctail includes 21 built-in color themes, each with dark and light variants. Switch themes in **Settings → Theme** and choose dark or light mode with the **Theme Mode** dropdown.

### Built-In Themes

Catppuccin (default), Catppuccin Frappé, Catppuccin Macchiato, Nord, Tokyo Night, Gruvbox, Dracula, One Dark, Solarized, Everforest, Ayu, Kanagawa, Matrix, Rosé Pine, Monokai, Night Owl, Synthwave '84, Cobalt2, GitHub, Palenight, Zenburn.

Theme palettes are inspired by and adapted from [OpenCode](https://github.com/anomalyco/opencode) by [Anomaly](https://anomaly.co/).

### Custom Themes

You can create custom themes by adding a JSON file to the themes directory:

| Platform | Path |
|----------|------|
| Linux | `~/.config/ctail/themes/` |
| Windows | `%APPDATA%\ctail\themes\` |
| macOS | `~/Library/Application Support/ctail/themes/` |

Custom themes appear alongside built-in themes in the theme picker. See the [Custom Themes Guide](custom-themes.md) for the full JSON format, color property reference, and tips on creating or adapting themes.

### Toggle Theme

Use **View → Toggle Theme** from the menu bar to quickly switch between dark and light mode for the current theme.

## Menu Bar

ctail includes a native menu bar:

| Menu | Item | Shortcut | Description |
|------|------|----------|-------------|
| **File** | Open | Ctrl+O | Open a file via the native file dialog |
| | Open Recent ▸ | | Submenu of recently opened files |
| | Close Tab | Ctrl+W | Close the current tab |
| | Quit | Ctrl+Q | Exit the application |
| **Edit** | Copy | Ctrl+C | Copy selected text |
| | Select All | Ctrl+A | Select all text in the log view |
| | Find | Ctrl+F | Open the search bar |
| **View** | Settings | Ctrl+, | Show/hide the settings panel |
| | Toggle Theme | | Switch between dark and light themes |
| **Tools** | AI Assistant... | Ctrl+Shift+A | Open the AI assistant dialog |
| **Help** | Check for Updates | | Check GitHub for a newer release |
| | About ctail | | Show version, license, and links |

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Ctrl+O** | Open file |
| **Ctrl+W** | Close current tab |
| **Ctrl+Tab** | Next tab / toggle between last two tabs |
| **Ctrl+Shift+Tab** | Previous tab |
| **Ctrl+Shift+A** | AI Assistant |
| **Ctrl+,** | Settings |
| **Ctrl+C** | Copy |
| **Ctrl+A** | Select all |
| **Ctrl+F** | Search (with case, word, regex toggles) |
| **Enter** | Next search match (when search bar is open) |
| **Shift+Enter** | Previous search match (when search bar is open) |
| **Escape** | Close search |

## Context Menus

### Tab Context Menu

Right-click any tab for quick actions:

| Item | Description |
|------|-------------|
| **Rename** | Give the tab a custom label (also available via double-click) |
| **Set color** | Assign a color to the tab for visual organization |
| **Close** | Close the clicked tab (Ctrl+W) |
| **Close Others** | Close all tabs except the clicked one |
| **Close to the Right** | Close all tabs to the right of the clicked one |
| **Refresh** | Reload the file content from disk |
| **Change file path…** | Open a file picker to change which file this tab displays |
| **Copy file path** | Copy the full file path to the clipboard |
| **Reveal in file manager** | Open the file's location in the system file explorer |

Destructive actions (Close Others, Close to the Right) ask for confirmation before proceeding.

### Log View Context Menu

Right-click in the log area for:

| Item | Description |
|------|-------------|
| **Copy** | Copy selected text (Ctrl+C) |
| **Select All** | Select all text (Ctrl+A) |
| **🤖 Ask AI about logs** | Open the AI assistant with current tab context |

## Check for Updates

Use **Help → Check for Updates** to check GitHub for a newer release. If an update is available, a notification is shown with a link to the release page. ctail also checks for updates automatically at a configurable interval (default: every 24 hours). Automatic update checks can be disabled in Settings.

## Linux Installation

### Using deb/rpm packages (recommended)

Pre-built packages include all dependencies:

```bash
# Debian/Ubuntu (24.04+)
sudo dpkg -i ctail_*_amd64.deb

# Fedora/RHEL
sudo rpm -i ctail-*-1.x86_64.rpm
```

The packages depend on `libgtk-3-0` and `libwebkit2gtk-4.1-0`, which are installed automatically.

### From source

After building, install system-wide with desktop integration:

```bash
make build
sudo make install
```

This installs:
- The binary to `/usr/local/bin/ctail`
- A `.desktop` file for application launchers
- Icons at all standard sizes (16×16 through 1024×1024)

Uninstall with:
```bash
sudo make uninstall
```

### Building packages from source

Requires [nfpm](https://nfpm.goreleaser.com/):

```bash
go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest
make package-deb    # builds .deb
make package-rpm    # builds .rpm
```

By default on Linux, ctail auto-detects the display backend (Wayland or X11). You can override this in Settings or via command-line flags:
```bash
ctail --x11       # Force X11 backend
ctail --wayland   # Force native Wayland backend
```

## Configuration Files

Configuration is stored in platform-specific directories:

| Platform | Path |
|----------|------|
| Linux | `~/.config/ctail/` (or `$XDG_CONFIG_HOME/ctail/`) |
| Windows | `%APPDATA%\ctail\` |
| macOS | `~/Library/Application Support/ctail/` |

### File Structure

```
ctail/
├── settings.json          # Application settings, window state, open tabs, recent files
├── profiles/
│   └── common-logs.json   # Highlighting rule profiles
└── themes/
    └── my-theme.json      # Custom color themes (optional)
```

### settings.json

Contains all application settings including poll interval, scroll buffer, theme, font size, window geometry, active profile, the list of open tabs for restoration, and recently opened files.

### profiles/*.json

Each file is a named profile containing an array of highlighting rules. Profile filenames are derived from the display name (sanitized for filesystem safety).

## Troubleshooting

### Files on network mounts are slow to open

ctail uses polling (not filesystem watchers) which works reliably over NFS/CIFS/SSHFS. If the mount is slow:
- Increase the **Poll Interval** to reduce I/O frequency.
- Files will still open — the tab shows a warning indicator while waiting.

### Application hangs on close

This was addressed in v0.2.0. File operations now have timeouts (3s for shutdown, 5s for reads, 10s for initial open). If a remote mount is completely unreachable, the application will still close after the timeout.

### Highlighting rules don't seem to work

- Verify the pattern is a valid regular expression.
- Go-style inline flags like `(?i)` are supported and automatically converted for the frontend.
- Check that the rule is **enabled** (checkbox in the rule list).
- Check rule ordering — rules lower in the list take precedence.

### Tabs not restored after crash

Tabs are saved on every open and close operation, so they should survive crashes. If tabs were lost:
- Check that `settings.json` exists and contains a `"tabs"` array.
- Ensure **Restore Tabs on Startup** is enabled in Settings.

### Window maximizes to wrong size on multi-monitor (Linux)

This is an [upstream GTK/WebKit2GTK limitation](https://github.com/wailsapp/wails/issues/2431) on Wayland with multiple monitors of different resolutions. ctail includes a workaround that detects and corrects wrong maximize dimensions, but it may not work in all configurations.

### Rendering corruption or flickering (Linux)

If you experience rendering issues (transparent windows, flickering, content bleeding outside the window), try switching to software rendering:

1. **Via Settings:** Settings → GPU Rendering → Software rendering (requires restart)
2. **Via command-line:** `ctail --software-render`

If the issue persists, also try `ctail --disable-dmabuf` as a lighter alternative.

### AI assistant not working

- Make sure a provider is configured in **Settings → AI**.
- For GitHub Copilot, check that your subscription is active and you completed the browser sign-in.
- For GitHub Models, verify your PAT hasn't expired and has the `models:read` permission.
- For custom providers, ensure the server is running and the endpoint URL is correct.
- See the [AI Assistant Guide](ai-assistant.md) for detailed troubleshooting.
