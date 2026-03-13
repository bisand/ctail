# ctail User Manual

**ctail** (color tail) is a cross-platform desktop log file viewer with real-time tailing and regex-based color highlighting.

## Table of Contents

- [Getting Started](#getting-started)
- [Opening Files](#opening-files)
- [Tabs](#tabs)
- [Following & Scrolling](#following--scrolling)
- [Highlighting Rules](#highlighting-rules)
- [Rule Profiles](#rule-profiles)
- [Settings](#settings)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Configuration Files](#configuration-files)
- [Troubleshooting](#troubleshooting)

---

## Getting Started

ctail is a cross-platform desktop log viewer. Launch the application and open any text or log file to start tailing it in real-time.

On first launch, ctail creates a default configuration with the "Common Logs" highlighting profile and sensible defaults. If you had files open previously, they are automatically restored.

## Opening Files

- Press **Ctrl+O** or use the file menu to open the native file dialog.
- Select any text file — it opens in a new tab and immediately starts tailing.
- Files on network mounts (NFS, CIFS, SSHFS) are supported. If the connection is slow or unavailable, the tab shows a loading indicator and the UI remains responsive.

## Tabs

Each open file gets its own tab in the tab bar at the top of the window.

- **Switch tabs** by clicking on them or pressing **Ctrl+Tab** (next) / **Ctrl+Shift+Tab** (previous).
- **Close a tab** by clicking the × button or pressing **Ctrl+W**.
- **Loading indicator** — Tabs show a spinner while the file is being read for the first time.
- **Tab persistence** — Open tabs are saved automatically. If the application is closed (or force-killed), tabs are restored on next launch. This can be toggled in Settings.

## Following & Scrolling

### Follow Mode

When a tab is in Follow mode, new lines are automatically appended and the view scrolls to the bottom — like `tail -f`. The Follow checkbox is in the status bar at the bottom of each tab.

- **Auto-enable**: Follow turns on automatically when you scroll to the end of the file.
- **Auto-disable**: Follow turns off when you scroll up, letting you inspect earlier log entries without interruption.
- While Follow is off, new lines are still counted (shown in the status bar) but not loaded into the view.

### Scroll Buffer

ctail uses a sliding window buffer to keep memory usage low. Only a configurable number of lines (default 500) are held in memory at any time.

- **Scrolling up** loads earlier lines from the file when you reach the upper portion of the buffer.
- **Scrolling down** loads later lines when you reach the lower portion.
- The status bar shows your current position in the file (e.g., "Lines 1,200 – 1,700 of 50,000").
- The scroll buffer size is configurable in Settings (100–5,000 lines).

### Horizontal Scrolling

Long lines extend beyond the viewport. Scroll horizontally to read them, or enable **Word Wrap** in Settings.

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

1. Open the Settings panel (gear icon).
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

## Settings

Open the Settings panel to configure:

| Setting | Description | Default |
|---------|-------------|---------|
| **Poll Interval** | How often to check files for changes (ms) | 500 |
| **Scroll Buffer** | Lines kept in memory while scrolling (100–5,000) | 500 |
| **Font Size** | Log text font size (10–24) | 14 |
| **Show Line Numbers** | Display line numbers in the gutter | Off |
| **Word Wrap** | Wrap long lines instead of horizontal scrolling | Off |
| **Restore Tabs** | Reopen previously open files on startup | On |
| **Theme** | Dark (Catppuccin Mocha) or Light (Catppuccin Latte) | Dark |

### Window State

The application window position, size, and maximised state are automatically saved and restored between sessions.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Ctrl+O** | Open file |
| **Ctrl+W** | Close current tab |
| **Ctrl+Tab** | Next tab |
| **Ctrl+Shift+Tab** | Previous tab |
| **Ctrl+F** | Search / filter |
| **Escape** | Close search |

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
├── settings.json          # Application settings, window state, open tabs
└── profiles/
    └── common-logs.json   # Highlighting rule profiles
```

### settings.json

Contains all application settings including poll interval, scroll buffer, theme, font size, window geometry, active profile, and the list of open tabs for restoration.

### profiles/*.json

Each file is a named profile containing an array of highlighting rules. Profile filenames are derived from the display name (sanitized for filesystem safety).

## Troubleshooting

### Files on network mounts are slow to open

ctail uses polling (not filesystem watchers) which works reliably over NFS/CIFS/SSHFS. If the mount is slow:
- Increase the **Poll Interval** to reduce I/O frequency.
- Files will still open — the tab shows a loading state while waiting.

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
