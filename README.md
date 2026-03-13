# ctail — Cross-Platform Log Tail Viewer with Highlighting

**ctail** — short for **color tail** — is a desktop log file viewer built with [Wails v2](https://wails.io/) (Go backend + Svelte frontend). Think `tail -f`, but with regex-powered color highlighting, multiple tabs, and a full GUI. Inspired by BareTail. Supports Windows, Linux, and macOS.

📖 **[User Manual](docs/user-manual.md)** — Full documentation on features, settings, and usage.

## Screenshots

| Dark Theme | Light Theme |
|:---:|:---:|
| ![Dark theme](docs/screenshots/dark-theme.png) | ![Light theme](docs/screenshots/light-theme.png) |

| Settings Panel | Rules Editor |
|:---:|:---:|
| ![Settings panel](docs/screenshots/settings-panel.png) | ![Rules editor](docs/screenshots/rules-panel.png) |

## Features

- **Multi-tab interface** — Open multiple log files simultaneously with keyboard navigation (Ctrl+Tab)
- **Real-time tailing** — Follow mode streams new lines as they're written, with automatic enable/disable on scroll
- **Regex-based highlighting** — Rules with foreground/background colors, bold/italic, line-level or match-level matching
- **Sliding window buffer** — Memory-bounded scrolling through large files; only a configurable window of lines (default 500) is kept in memory
- **Profile system** — Multiple highlighting profiles with visual rule preview and drag-and-drop reordering
- **Non-blocking I/O** — Files on slow or unreachable network mounts won't freeze the UI; all file operations run in the background with timeouts
- **Session persistence** — Window position/size, open tabs, active profile, and all settings survive restarts
- **Search** — Ctrl+F to filter lines within the buffer
- **Themes** — Dark (Catppuccin Mocha) and Light (Catppuccin Latte)
- **Cross-platform** — Linux, Windows, and macOS

## Quick Start

### Prerequisites

- Go 1.21+
- Node.js 18+
- [Wails CLI v2](https://wails.io/docs/gettingstarted/installation)
- **Linux**: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev` (Ubuntu 24.04+) or `libwebkit2gtk-4.0-dev`
- **Windows**: WebView2 (included in Windows 10/11)

### Build & Run

```bash
# Install Wails CLI
go install github.com/wailsapp/wails/v2/cmd/wails@latest

# Run in dev mode (hot reload)
make dev

# Build for production
make build

# Run tests
make test
```

> **Note:** On Ubuntu 24.04+ / Zorin OS 18, the `webkit2_41` build tag is required (handled automatically by the Makefile). Override with `make dev TAGS=` on systems with webkit2gtk-4.0.

## Architecture

```
Go Backend                          Svelte Frontend
┌──────────────┐                    ┌──────────────────┐
│ File Tailer  │ ──Wails Events──▶  │ Tab Bar          │
│ (polling,    │                    │ Log View (scroll)│
│  offset idx) │                    │ Highlighted Lines│
├──────────────┤                    ├──────────────────┤
│ Config Mgr   │ ◀──Wails Bind──▶  │ Settings Panel   │
│ (JSON files) │                    │ Rule Editor      │
├──────────────┤                    ├──────────────────┤
│ Rule Engine  │                    │ Highlight Utils  │
│ (regex)      │                    │ (client-side)    │
└──────────────┘                    └──────────────────┘
```

- **Go backend** handles file I/O (polling + direct seek via byte offset index), configuration persistence, and the rules engine
- **Svelte frontend** handles rendering, client-side highlighting (for instant rule feedback), and scroll buffer management
- **Communication** via Wails bindings (sync method calls) and events (async streaming)
- **No external Go dependencies** beyond Wails itself

## Configuration

Config files are stored in platform-specific directories:

| Platform | Path |
|----------|------|
| Linux | `~/.config/ctail/` (or `$XDG_CONFIG_HOME/ctail/`) |
| Windows | `%APPDATA%\ctail\` |
| macOS | `~/Library/Application Support/ctail/` |

See the [User Manual](docs/user-manual.md) for details on all settings and configuration options.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+O | Open file |
| Ctrl+W | Close tab |
| Ctrl+Tab | Next tab |
| Ctrl+Shift+Tab | Previous tab |
| Ctrl+F | Search / filter |
| Escape | Close search |

## License

MIT
