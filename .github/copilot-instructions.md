# Copilot Instructions for ctail

## Build & Run

```bash
make dev          # wails dev with webkit2_41 tag (hot reload)
make build        # wails build with webkit2_41 tag (production binary)
make test         # go test ./internal/... -v
```

Run a single test:
```bash
go test ./internal/tailer -run TestTailerTruncation -v
```

The `webkit2_41` tag is required on Ubuntu 24.04+ / Zorin OS 18 (only ships webkit2gtk-4.1). Override with `make dev TAGS=` on systems with webkit2gtk-4.0.

After changing Go method signatures on `App`, run `wails generate module` to regenerate JS bindings in `frontend/wailsjs/`.

## Architecture

**Wails v2 desktop app** — Go backend + Svelte 3 frontend communicating via two mechanisms:

- **Wails Bind** (synchronous): Frontend calls Go methods directly via auto-generated JS functions in `frontend/wailsjs/go/main/App.js`. All public methods on the `App` struct in `app.go` are exposed.
- **Wails Events** (async push): Go backend emits events (`tailer:lines`, `tailer:truncated`, `tailer:error`) that the frontend listens to via `EventsOn()`. This is how new log lines stream to the UI.

The Go binary embeds the compiled frontend via `//go:embed all:frontend/dist` in `main.go`.

### Backend packages

- **`app.go`** — Central API surface. Manages tab lifecycle (open/close/list), delegates to tailer and config packages. Each tab gets its own `Tailer` goroutine. Protected by `sync.RWMutex`.
- **`internal/tailer`** — Polls files at a configurable interval, reads new bytes from the last offset, and maintains a bounded slice of the last N lines (sliding window). Detects truncation by comparing file size to previous size. Callbacks (`OnLines`, `OnTruncated`, `OnError`) are wired to Wails events in `app.go`.
- **`internal/config`** — Manages JSON config files in platform-specific directories (`~/.config/ctail/` on Linux, `%APPDATA%/ctail/` on Windows). Handles `settings.json` and `profiles/*.json`. Profile filenames are sanitized from display names.
- **`internal/rules`** — Compiles regex patterns once via `SetRules()`, then applies them per-line via `Apply()`. Two match types: `"line"` (styles entire line) and `"match"` (styles matched substring only). Rules sorted by priority ascending — higher priority wins on conflict.

### Frontend structure

- **`App.svelte`** — Root component. Initializes by loading settings and profiles from Go, sets up Wails event listeners for streaming log data.
- **`lib/stores/`** — Svelte writable stores for tabs (lines, active tab, update badges), settings, and rule profiles.
- **`lib/components/`** — `LogView` handles scrolling and search, `LogLine` renders highlighted segments, `SettingsPanel` provides the rule editor and settings UI, `TabBar` shows tabs with update badges.
- **`lib/utils/highlight.js`** — Client-side highlighting. Receives rules as JSON, compiles to JS RegExp, splits lines into styled segments. This runs on the frontend for instant re-render when rules change.

## Conventions

- **Highlighting is split**: Rules are defined/persisted in Go (`internal/config`), compiled/validated in Go (`internal/rules`), but *applied* on the frontend (`highlight.js`) for responsiveness.
- **Concurrency**: All shared state in Go uses `sync.RWMutex`. Each tailer runs its own goroutine. Callbacks fire from the polling goroutine.
- **Tests use real temp files**: `t.TempDir()` for isolation, `os.Setenv("XDG_CONFIG_HOME", ...)` to redirect config paths. Tailer tests use `time.Sleep` to wait for polling goroutines.
- **CSS theming**: Dark/light themes via CSS custom properties in `style.css` (`--bg-primary`, `--text-primary`, etc.), toggled by `data-theme` attribute on `<html>`. Color palette follows Catppuccin (Mocha for dark, Latte for light).
- **No external Go dependencies** beyond Wails itself. The tailer, config, and rules packages use only the standard library.
