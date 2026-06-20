import AppKit

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)   // show in Dock + own the menu bar (unbundled binary)
app.run()
