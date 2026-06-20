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

## What this proves / what's still open

**Proven:** native tailing + rotation/truncation, virtualized rendering, regex
highlighting, follow mode, theming — the architecture (SwiftUI-or-AppKit shell +
AppKit `NSTableView` log surface) holds up.

**Not yet built (the other ~60%, mostly mechanical):** tabs, profiles/rules
editor, settings panel, AI assistant + Copilot device flow, search bar, update
checker, custom themes, context menus.

**The remaining real risk — App Store sandboxing:** this binary runs
unsandboxed. Shipping via the App Store requires the sandbox entitlement plus
**security-scoped bookmarks** to retain access to user-picked files across
launches, and may constrain watching arbitrary network mounts. That's the next
thing to validate.
