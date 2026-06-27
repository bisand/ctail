import AppKit

// Headless self-tests (see SelfTest.swift) — used because XCTest isn't in CLT.
if CommandLine.arguments.contains("--selftest") {
    exit(SelfTest.run())
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)   // show in Dock + own the menu bar (unbundled binary)
app.run()
