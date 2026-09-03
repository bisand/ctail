# ctail — native macOS POC

A proof-of-concept Swift/AppKit rewrite of ctail, built to de-risk the two hard
parts of a full native port **before** committing to it:

1. **The engine** — now the Rust crate in [`core/`](../core/), reached
   through [UniFFI](https://mozilla.github.io/uniffi-rs/) bindings: the tailer,
   the data model (settings, profiles, rules, themes) and its JSON persistence,
   the theme catalogue, regex highlighting and search. The Swift files of the
   same names ([Tailer.swift](Sources/ctailmac/Tailer.swift),
   [ConfigStore.swift](Sources/ctailmac/ConfigStore.swift),
   [Highlight.swift](Sources/ctailmac/Highlight.swift),
   [SearchQuery.swift](Sources/ctailmac/SearchQuery.swift),
   [Theme.swift](Sources/ctailmac/Theme.swift)) are thin wrappers that keep the
   surface the UI uses and add the AppKit bits (main-queue callbacks, NSColor,
   NSAttributedString). The Swift tailer they replaced is kept at
   [scripts/tailbench/LegacyTailer.swift](scripts/tailbench/LegacyTailer.swift)
   as the benchmark reference.
2. **The virtualized log view** — `NSTableView`-backed so only visible rows are
   rendered; flat memory/CPU regardless of buffer size, with a line-number
   gutter, regex highlighting, and follow (`tail -f`) mode that auto-pauses on
   scroll-up. See [LogView.swift](Sources/ctailmac/LogView.swift).

It also carries the real Catppuccin Mocha theme and sample highlight rules so the
look and feel matches the Wails app.

## Requirements

Swift 6 toolchain (Xcode or Command Line Tools) plus a Rust toolchain
(`rustup`, with the `x86_64-apple-darwin` target for universal builds — the
build script adds it if missing). The self-tests need only the CLT; the
XCFramework step needs `xcodebuild`.

## Build the engine first

```bash
cd macos
make core          # = scripts/build-core.sh
```

That cargo-builds `libctail_core.a` for arm64 + x86_64, lipo's them, generates
the Swift bindings into `Sources/CtailCore/` and wraps the library as
`Frameworks/CtailCoreFFI.xcframework`. All of it is git-ignored, and both
`Package.swift` and `project.yml` consume it, so run it after every engine
change (the `make build/run/test/xcodeproj` targets do). `PROFILE=debug` and
`CORE_TARGETS=aarch64-apple-darwin` speed up local iteration.

## Run

```bash
cd macos
make core && swift build

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

- **Engine** — (Rust, `core/`) polling tailer with inode rotation + truncation
  detection, partial-line buffering, tail-first + background line indexing,
  windowed range reads, read timeouts; settings/profile/theme persistence with
  the Go app's JSON keys; regex highlighting (`fancy-regex`: linear-time for
  plain patterns, lookaround/backreferences still accepted) and search.
  The engine also scans whole files for a find bar (`CoreFileSearch`: a
  debounced, cancellable scan on its own thread, and the match stepping that
  goes with it). **The Swift UI does not use that yet** — its find bar still
  matches only the window of lines in memory, as it always has; the Linux and
  Windows front end drives the same object today.
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

## Engine benchmark

`scripts/tailbench/main.swift` prints the same measurements as the Rust harness
(`core/examples/tailbench.rs`). It compiles against either engine:

```bash
# The legacy Swift engine (reference):
swiftc -O scripts/tailbench/LegacyTailer.swift scripts/tailbench/main.swift -o /tmp/tailbench-legacy

# The Rust engine through the Swift wrapper + UniFFI (what the app ships), after `make core`:
M=/tmp/ffimod; mkdir -p $M
swiftc -O -I .build/core/include -emit-module -emit-library -module-name CtailCore \
  Sources/CtailCore/CtailCore.swift -L .build/core/lib -lctail_core \
  -o $M/libCtailCore.dylib -emit-module-path $M/CtailCore.swiftmodule
swiftc -O -I $M -I .build/core/include Sources/ctailmac/Tailer.swift scripts/tailbench/main.swift \
  -L $M -lCtailCore -Xlinker -rpath -Xlinker $M -o /tmp/tailbench-ffi

/tmp/tailbench-ffi --file /path/to/ctail-bench.log
```

On a warm 2 GB file the FFI path indexes in ~0.11–0.18 s (legacy Swift: ~0.26–0.5 s)
but pays ~0.4 µs per line to marshal records into Swift: a 10 k-line page-in is
~5 ms instead of ~1 ms. That work happens on the engine's worker thread, not the
main thread. Packing batches as one byte blob and splitting on the Swift side
would remove most of it if it ever shows.

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
