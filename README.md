# ctail — Cross-Platform Log Tail UI with Highlighting

A BareTail-inspired log tail application built with Go (Wails v2) and Svelte, supporting Windows and Linux.

## Features

- **Multi-tab interface** — Open multiple log files with update badges on inactive tabs
- **Regex-based highlighting** — Rules with foreground/background colors, bold/italic, line-level or match-level
- **Sliding window buffer** — Configurable buffer size (default 10K lines), streams file tail without loading entire file
- **Configurable polling** — Adjustable poll interval (default 500ms), handles file truncation and rotation
- **Profile system** — Multiple highlighting profiles, in-app rule editor, JSON config persistence
- **Search** — Ctrl+F to filter lines within the buffer
- **Themes** — Dark (Catppuccin Mocha) and Light (Catppuccin Latte) themes
- **Cross-platform** — Windows and Linux support

## Prerequisites

- Go 1.21+
- Node.js 18+
- [Wails CLI v2](https://wails.io/docs/gettingstarted/installation)
- Linux: `libgtk-3-dev`, `libwebkit2gtk-4.0-dev`
- Windows: WebView2 (included in Windows 10/11)

## Development

```bash
# Install Wails CLI
go install github.com/wailsapp/wails/v2/cmd/wails@latest

# Run in dev mode (hot reload)
wails dev

# Build for production
wails build
```

## Configuration

Config files are stored in:
- Linux: `~/.config/ctail/`
- Windows: `%APPDATA%/ctail/`

### Files
- `settings.json` — App settings (poll interval, buffer size, theme, font size, etc.)
- `profiles/*.json` — Highlighting rule profiles

### Default Profile: "Common Logs"

| Rule | Pattern | Type | Color |
|------|---------|------|-------|
| Fatal | `\bFATAL\b` | Line | White on red |
| Error | `\bERROR\b` | Line | Red on dark red |
| Warning | `\bWARN(ING)?\b` | Line | Yellow on dark yellow |
| Info | `\bINFO\b` | Match | Blue |
| Debug | `\bDEBUG\b` | Match | Gray |
| Timestamp | `\d{4}-\d{2}-\d{2}T?\d{2}:\d{2}:\d{2}` | Match | Green |

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+O | Open file |
| Ctrl+W | Close tab |
| Ctrl+F | Search/filter |
| Escape | Close search |

## Architecture

```
Go Backend                          Svelte Frontend
┌──────────────┐                    ┌──────────────────┐
│ File Tailer  │──Wails Events──▶  │ Tab Bar + Badges │
│ (polling,    │                    │ Log View (scroll)│
│  ring buffer)│                    │ Highlighted Lines│
├──────────────┤                    ├──────────────────┤
│ Config Mgr   │◀──Wails Bind──▶  │ Settings Panel   │
│ (JSON files) │                    │ Rule Editor      │
├──────────────┤                    ├──────────────────┤
│ Rule Engine  │                    │ Highlight Utils  │
│ (regex)      │                    │ (client-side)    │
└──────────────┘                    └──────────────────┘
```

## Running Tests

```bash
go test ./internal/... -v
```

## License

MIT
# ctail
