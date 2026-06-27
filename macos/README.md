# ctail — native macOS POC

A proof-of-concept Swift/AppKit rewrite of ctail, built to de-risk the two hard
parts of a full native port **before** committing to it:

1. **The tail engine** — a faithful port of [`internal/tailer/tailer.go`](../internal/tailer/tailer.go):
   polling (network-mount safe), inode-based rotation detection, truncation
   handling, partial-line buffering, and tail-first reads for huge files.
   See [Tailer.swift](Sources/ctailmac/Tailer.swift).
2. **The virtualized log view** — `NSTableView`-backed so only visible rows are
   rendered; flat memory/CPU regardless of buffer size, with a line-number
   gutter, regex highlighting, and follow (`tail -f`) mode that auto-pauses on
   scroll-up. See [LogView.swift](Sources/ctailmac/LogView.swift).

It also carries the real Catppuccin Mocha theme and sample highlight rules so the
look and feel matches the Wails app.

## Requirements

Swift 6 toolchain (Xcode or Command Line Tools). No full Xcode project needed.

## Run

```bash
cd macos
swift build

# Tail a specific file:
./.build/debug/ctailmac /path/to/some.log

# …or launch with no arg to get a file picker:
./.build/debug/ctailmac

# Watch it tail live — in another terminal:
./gen-log.sh /tmp/ctail-demo.log 0.3
# then open /tmp/ctail-demo.log in the app
```

## Status

Feature parity with the Wails app is implemented natively (tracked under the
"Native macOS App" milestone, issues #1–#16):

- **Engine** — polling tailer with inode rotation + truncation detection,
  partial-line buffering, tail-first + background line indexing, windowed range
  reads, read timeouts.
- **UI** — virtualized `NSTableView` log surface, multi-tab interface (drag
  reorder, rename, color, Ctrl+Tab, reopen-closed), VS Code-style search
  (case/word/regex + filter mode), all 21 themes + custom themes, profiles &
  rules editor, settings panel, native menu bar + context menus.
- **Integrations** — recent files, file associations, session persistence,
  background throttling, GitHub update check, AI assistant (OpenAI/GitHub
  Models/Copilot/custom) with Copilot device-flow OAuth.
- **App Store** — sandbox entitlements + **security-scoped bookmarks** so opened
  files reopen across launches; bookmark use is best-effort so unsandboxed
  dev/direct builds still work.

## Sandbox notes

`make bundle` ad-hoc signs with `Resources/ctail.entitlements` (sandbox on). For
actual App Store submission, sign with your Apple Developer identity +
provisioning profile. Watching files on arbitrary network mounts may be
constrained under the sandbox; a notarized direct-download build remains the
fallback for the unrestricted experience.

## Tests

`make test` runs the in-process self-test suite (`--selftest`) — 80 checks
across config, themes, search, updates, AI endpoint/parsing, bookmarks, and the
tail engine. XCTest isn't available under the Command Line Tools toolchain; the
harness is trivially portable to XCTest once full Xcode is installed.
