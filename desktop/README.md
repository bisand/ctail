# ctail desktop — Linux and Windows front end on DeniseUI

The cross-platform front end for ctail: one Rust binary, the engine from
[`core/`](../core/) as a plain Cargo dependency, and the UI drawn by
[DeniseUI](https://github.com/bisand/denise) — a direct-rendering toolkit with
damage-tracked repaints, no webview and no GPU requirement. It runs on macOS
too, which is how it is developed; the shipping macOS app stays the AppKit one
in [`macos/`](../macos/).

## Why this and not a webview or Qt

The Wails app's Svelte UI was never as smooth as the native Mac app, and Tauri
would have kept the same system webviews. Qt brings a C++ bridge and a heavy
toolchain. DeniseUI is pure Rust, its theme model is a nine-seed semantic
palette that ctail's 21 themes map onto directly (`src/theme.rs`), and a full
1080p repaint measures about 0.2 ms on a desktop CPU, so a log view that only
paints visible rows has budget to spare. The toolkit's own form designer ships
on macOS, Windows and Linux the same way this app does.

## What is here

- `src/logview.rs` — the custom `LogView` widget: a bounded window of lines,
  only visible rows laid out and painted, gutter numbers (absolute once the
  engine's head count lands), highlight spans from the engine's `Highlighter`,
  follow mode that pauses on scroll-up and resumes on End, row selection by
  click/drag with Ctrl/Cmd+C copy, PageUp/PageDown/Home/End, and scrollback
  fetched from the engine when the window reaches its top.
- `src/app.rs` — tabs (one `LogView` node each), engine callbacks funnelled
  through channels into the UI thread, status line, follow toggle, Ctrl/Cmd+O
  open (native dialog via `rfd`), Ctrl/Cmd+W close, Ctrl/Cmd+Q quit.
- `src/theme.rs` — ctail palette → Denise `Theme`.
- `src/fonts.rs` — finds a monospace face and a UI face on the machine
  (SF Mono, DejaVu Sans Mono, Consolas, …), falling back to the built-in
  bitmap font.

Settings, profiles, recent files and themes come from the same config store
(`~/.config/ctail`, `%APPDATA%\ctail`, `~/Library/Application Support/ctail`) as
the other front ends, so a profile edited on the Mac renders identically here.

## Not yet

Search bar and filter, settings and profile editors, a menu bar (Denise has no
native menus; an in-app strip or `muda` is the plan), bold/italic rule styles
(needs the bold face registered as a second font), word wrap, the AI
assistant, update checks, and Linux/Windows CI builds. Engine events cannot
wake the event loop yet, so the app polls its channels every 100 ms while a
file is open — a waker in `denise-winit` would remove that.

## Run

```bash
cargo run -p ctail-desktop -- /path/to/some.log
CTAIL_CONFIG_DIR=/tmp/ctail-dev cargo run -p ctail-desktop   # isolated config
```

Linux needs the usual winit/softbuffer packages (X11 or Wayland development
libraries); Windows needs nothing beyond the MSVC toolchain.

## Dependency pin

`Cargo.toml` pins DeniseUI to a pushed `main` revision because the published
0.19 crates predate the anchors, default-font and window-title APIs used here.
Bump the `rev` (or switch to a crates.io version) when the next release lands;
the in-progress `painter-trait` branch renames `Canvas` to `Pen` in
`Widget::paint`, which is a one-line change in `logview.rs`.
