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
  fetched from the engine when the window reaches its top. It also carries the
  search: match highlighting, the current match picked out, and filter mode.
- `src/search.rs` — the find bar (Ctrl/Cmd+F): query field, the Aa / W / .*
  toggles, a filter toggle that hides non-matching lines, a match counter, and
  prev/next/close. Enter and ↓ step forward, Shift+Enter and ↑ back, Escape
  closes. Typing searches live. The counter reads as "where you are": until a
  match has been stepped to it counts from the first one at or after the top
  of the view, so the first ↓ goes to a match near what is on screen rather
  than to the oldest one in the file. The count itself comes from
  `ctail_core::FileSearch`, which scans the file on disk — see below.
- `src/app.rs` — tabs (one `LogView` node each), engine callbacks funnelled
  through channels into the UI thread, status line, follow toggle, Ctrl/Cmd+O
  open (native dialog via `rfd`), Ctrl/Cmd+W close, Ctrl/Cmd+Q quit.
- `src/settings.rs` — the Settings window. A window of its own rather than a
  panel, so it gets the platform's title bar, close button and window
  management; its contents are drawn with the same widgets and theme as the
  rest. Ctrl/Cmd+, opens it, Escape or Cancel dismisses it, Save hands the
  edited settings back to the main window, which persists them and applies
  the theme, font size, gutter, buffer size and poll interval live.
- `src/profiles.rs` — the Profiles & Rules window (Ctrl/Cmd+R): the profile
  chooser with New / Rename / Delete / Set Active, the rule list with add,
  remove and reorder, and a rule editor with a live preview of the rule
  against a sample line. Names are asked for in a modal child window, and
  deleting a profile asks through the platform's own message dialog. Saving
  restyles the open tabs at once.
- `src/tabbar.rs` — the tab strip: a tab carries a colour stripe and a close
  cross as well as a label, and a right-click has to report *which* tab it
  hit, so it is a widget of its own rather than the toolkit's `Tabs`.
- `src/widgets.rs` — the two widgets the toolkit does not have, both about
  *arbitrary* colours rather than theme roles: a clickable colour swatch and
  the rule preview.
- `src/prompt.rs` — the one-field modal window the name prompts use.
- `src/theme.rs` — ctail palette → Denise `Theme`.
- `src/fonts.rs` — finds a monospace face and a UI face on the machine
  (SF Mono, DejaVu Sans Mono, Consolas, …), falling back to the built-in
  bitmap font.

The menu bar carries File, Edit, View and Help — the macOS app's menus, less
the items this front end does not have yet — and right-clicking a tab offers
rename, refresh, change file path, copy path, reveal in the file manager,
close, and a colour. Both are the toolkit's own `MenuBar` and `open_menu`:
under winit there is no GTK window to hang a native menu bar on, so a native
bar would exist on Windows and macOS and simply be missing on Linux, which is
the platform this front end is for. One bar that behaves the same everywhere
beat two thirds of a native one.

Sessions come back the way the macOS app's do: with no file named on the
command line, the tabs from last time reopen in their saved order on the tab
that was active, and the window keeps its size. Ctrl+Tab and Ctrl+Shift+Tab
cycle tabs, Ctrl/Cmd+W closes one and Ctrl/Cmd+Shift+T reopens the last
closed.

Settings, profiles, recent files and themes come from the same config store
(`~/.config/ctail`, `%APPDATA%\ctail`, `~/Library/Application Support/ctail`) as
the other front ends, so a profile edited on the Mac renders identically here.

Search covers the whole file, not just the window of lines in memory. The
engine's `FileSearch` scans what is on disk on its own thread once the typing
stops, and the counter reads "3/41892" for the file — while the scan is still
running it shows the window's own count with a `+…` after it, and ↑/↓ step
that, so a huge file answers immediately and gets more truthful as it goes.
Stepping to a match the window does not reach fetches the part of the file
around it and shows that instead; the tab stops following, since its window is
no longer the tail, and End (or the Follow box) reads the tail again.

Word wrap (⌥⌘W / Ctrl+Alt+W, or the setting) breaks long lines to the width
instead of running them off the edge: after the last space that fits, or
mid-character when no space does, because a log line is as likely to be one
unbroken token as a sentence. A wrapped line stays one line for everything
else — one number in the gutter, one row to select, one entry in the match
list — and the view scrolls a wrapped row at a time.

Help → Check for Updates… asks GitHub through the engine's `update` module
and says so either way; on launch the check runs quietly when the setting
allows and the interval has passed (the time of the last one is kept beside
the settings, in `last-update-check`), and only speaks up when there is
something to download.

## Not yet

Bold/italic rule styles (needs the bold face registered as a second font)
and the AI assistant's window — the assistant itself (providers, prompts,
Copilot sign-in) is in the engine's `ai` module, waiting for a window. Engine events cannot wake the event loop yet, so
the app polls its channels every 100 ms while a file is open — a waker in
`denise-winit` would remove that.

## Run

```bash
cargo run -p ctail-desktop -- /path/to/some.log
CTAIL_CONFIG_DIR=/tmp/ctail-dev cargo run -p ctail-desktop   # isolated config

# The window cannot be scripted from outside without accessibility permission,
# so these open things for screenshots and manual checks:
CTAIL_DEBUG_SEARCH=ERROR CTAIL_DEBUG_SEARCH_FILTER=1 cargo run -p ctail-desktop -- some.log
CTAIL_DEBUG_SETTINGS=1 cargo run -p ctail-desktop -- some.log
CTAIL_DEBUG_PROFILES=1 cargo run -p ctail-desktop -- some.log

# And this paints the Settings window into a PPM without needing a display at
# all, which is the only way to see it on a machine whose screen has slept:
cargo run -p ctail-desktop -- --snapshot settings /tmp/settings.ppm 2
cargo run -p ctail-desktop -- --snapshot profiles /tmp/profiles.ppm 2

# The main window too, with a menu down: CTAIL_DEBUG_MENU is a title index, or
# any other word for a tab's context menu.
CTAIL_DEBUG_FILE=some.log CTAIL_DEBUG_MENU=0 \
  cargo run -p ctail-desktop -- --snapshot main /tmp/main.ppm 2
```

Linux needs the usual winit/softbuffer packages (X11 or Wayland development
libraries); Windows needs nothing beyond the MSVC toolchain.

## Dependency pin

`Cargo.toml` pins DeniseUI to a pushed revision. The published 0.19 crates
predate the anchors, default-font and window-title APIs used here, and the
menu widgets (`MenuBar`, `MenuItem`, `open_menu`, `open_menu_at` and
`Ui::push_popup_at`) were written for this app and live on the toolkit's
`feat/menus` branch. Bump the `rev`, or switch to a crates.io version, once a
release carries them. The in-progress `painter-trait` branch renames `Canvas`
to `Pen` in `Widget::paint`, which is a one-line change in each widget here.
