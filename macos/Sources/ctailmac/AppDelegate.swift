import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, AppActions {
    private var window: NSWindow!
    private var tabs: TabController!

    private let config = ConfigStore()
    private var settings = AppSettings()
    private lazy var palette: ThemeColors =
        ThemeCatalog.palette(name: settings.theme, mode: settings.themeMode, custom: config.themesDir)

    func applicationDidFinishLaunching(_ notification: Notification) {
        settings = config.loadSettings()
        config.ensureDefaultProfile()
        buildMenu()
        buildWindow()

        // Open files from argv, else show the picker.
        let args = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
        if args.isEmpty {
            openFileDialog()
            if tabs.tabs.isEmpty { NSApp.terminate(nil) }
        } else {
            args.forEach { tabs.open(path: $0) }
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { true }

    /// Finder double-click / drag-onto-Dock entry point.
    func application(_ application: NSApplication, open urls: [URL]) {
        urls.forEach { tabs?.open(path: $0.path) }
    }

    // MARK: - Window

    private func buildWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0,
                                              width: CGFloat(settings.window.width),
                                              height: CGFloat(settings.window.height)),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.title = "ctail"
        window.titlebarAppearsTransparent = true
        window.backgroundColor = palette.background
        window.center()

        tabs = TabController(config: config, settings: settings, palette: palette)
        tabs.onActiveFileChanged = { [weak self] path in
            self?.window.title = path.map { "ctail — \(($0 as NSString).lastPathComponent)" } ?? "ctail"
        }
        window.contentView = tabs.container
        window.makeKeyAndOrderFront(nil)
    }

    // MARK: - AppActions

    func openFileDialog() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.message = "Choose log file(s) to tail"
        if panel.runModal() == .OK {
            panel.urls.forEach { tabs.open(path: $0.path) }
        }
    }

    func findInLog() { tabs.openSearch() }

    // MARK: - Menu

    private func buildMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem(); main.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Open…", action: #selector(openFileDialog), keyEquivalent: "o")
        appMenu.addItem(withTitle: "Close Tab", action: #selector(closeActiveTab), keyEquivalent: "w")
        let reopen = appMenu.addItem(withTitle: "Reopen Closed Tab", action: #selector(reopenTab), keyEquivalent: "t")
        reopen.keyEquivalentModifierMask = [.command, .shift]
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit ctail", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu

        let editItem = NSMenuItem(); main.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Find…", action: #selector(findInLog), keyEquivalent: "f")
        editItem.submenu = editMenu

        NSApp.mainMenu = main
    }

    @objc private func closeActiveTab() { tabs.close(tabs.active) }
    @objc private func reopenTab() { tabs.reopenClosed() }
}
