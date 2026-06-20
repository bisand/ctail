import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow!
    private var logView: LogView!
    private var tailer: Tailer?
    private var statusLabel: NSTextField!
    private var followLabel: NSTextField!

    private let theme = Theme.catppuccinMocha

    // A few sample highlight rules so the demo shows the real highlighting path.
    private lazy var rules: [HighlightRule] = [
        HighlightRule(pattern: #"\b(ERROR|FATAL|panic)\b"#, fg: Theme.hex("#f38ba8"), bold: true, lineLevel: true),
        HighlightRule(pattern: #"\bWARN(ING)?\b"#,          fg: Theme.hex("#f9e2af"), lineLevel: true),
        HighlightRule(pattern: #"\bINFO\b"#,                fg: Theme.hex("#a6e3a1")),
        HighlightRule(pattern: #"\bDEBUG\b"#,               fg: Theme.hex("#6c7086")),
        HighlightRule(pattern: #"\b\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}\b"#, fg: Theme.hex("#89b4fa")),
        HighlightRule(pattern: #"\bhttps?://[^\s]+"#,       fg: Theme.hex("#74c7ec")),
    ].compactMap { $0 }

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMenu()
        buildWindow()

        // File path from argv, else prompt.
        let args = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
        if let path = args.first {
            openFile(path)
        } else {
            promptForFile()
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { true }

    // MARK: - UI

    private func buildWindow() {
        let frame = NSRect(x: 0, y: 0, width: 1000, height: 680)
        window = NSWindow(contentRect: frame,
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.title = "ctail (native POC)"
        window.titlebarAppearsTransparent = true
        window.backgroundColor = theme.background
        window.center()

        logView = LogView(theme: theme, rules: rules)
        logView.translatesAutoresizingMaskIntoConstraints = false
        logView.onFollowingChanged = { [weak self] f in self?.updateFollow(f) }

        let bar = buildStatusBar()
        let container = NSView()
        container.addSubview(logView)
        container.addSubview(bar)
        NSLayoutConstraint.activate([
            logView.topAnchor.constraint(equalTo: container.topAnchor),
            logView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            logView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            logView.bottomAnchor.constraint(equalTo: bar.topAnchor),
            bar.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            bar.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            bar.heightAnchor.constraint(equalToConstant: 24),
        ])
        window.contentView = container
        window.makeKeyAndOrderFront(nil)
    }

    private func buildStatusBar() -> NSView {
        let bar = NSView()
        bar.translatesAutoresizingMaskIntoConstraints = false
        bar.wantsLayer = true
        bar.layer?.backgroundColor = theme.backgroundAlt.cgColor

        statusLabel = label(theme.foreground)
        followLabel = label(Theme.hex("#a6e3a1"))
        followLabel.alignment = .right
        bar.addSubview(statusLabel)
        bar.addSubview(followLabel)
        NSLayoutConstraint.activate([
            statusLabel.leadingAnchor.constraint(equalTo: bar.leadingAnchor, constant: 10),
            statusLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            followLabel.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -10),
            followLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
        ])
        updateFollow(true)
        return bar
    }

    private func label(_ color: NSColor) -> NSTextField {
        let l = NSTextField(labelWithString: "")
        l.translatesAutoresizingMaskIntoConstraints = false
        l.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        l.textColor = color
        return l
    }

    private func updateFollow(_ following: Bool) {
        followLabel.stringValue = following ? "● FOLLOWING (tail -f)" : "○ paused — scroll to bottom to resume"
        followLabel.textColor = following ? Theme.hex("#a6e3a1") : theme.gutter
    }

    private func refreshStatus() {
        statusLabel.stringValue = "\(logView.lineCount) lines"
    }

    // MARK: - File / tailer wiring

    private func promptForFile() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.message = "Choose a log file to tail"
        if panel.runModal() == .OK, let url = panel.url {
            openFile(url.path)
        } else {
            NSApp.terminate(nil)
        }
    }

    private func openFile(_ path: String) {
        tailer?.stop()
        logView.reset()
        window.title = "ctail — \((path as NSString).lastPathComponent)"

        let t = Tailer(path: path)
        t.onLines = { [weak self] lines in self?.logView.append(lines); self?.refreshStatus() }
        t.onReset = { [weak self] in self?.logView.reset() }
        t.onError = { [weak self] msg in self?.statusLabel.stringValue = "⚠︎ \(msg)" }
        t.onReady = { [weak self] in self?.refreshStatus() }
        t.start()
        tailer = t
    }

    // MARK: - Menu

    private func buildMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        main.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Open…", action: #selector(menuOpen), keyEquivalent: "o")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit ctail", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        NSApp.mainMenu = main
    }

    @objc private func menuOpen() { promptForFile() }
}
