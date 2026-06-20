import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, AppActions, NSMenuDelegate {
    private var window: NSWindow!
    private var tabs: TabController!

    private let config = ConfigStore()
    private var settings = AppSettings()
    private var palette = ThemeColors.placeholder

    func applicationDidFinishLaunching(_ notification: Notification) {
        settings = config.loadSettings()
        config.ensureDefaultProfile()
        palette = resolvePalette()
        buildMenu()
        buildWindow()
        tabs.onTabsChanged = { [weak self] in self?.persistSession() }

        let args = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
        if args.isEmpty {
            if settings.restoreTabs && !settings.tabs.isEmpty {
                tabs.restore(settings.tabs,
                             activePath: settings.lastActiveTabPath.isEmpty ? nil : settings.lastActiveTabPath)
            }
            if tabs.tabs.isEmpty {
                openFileDialog()
                if tabs.tabs.isEmpty { NSApp.terminate(nil) }
            }
        } else {
            args.forEach { tabs.open(path: $0) }
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { persistSession() }
    func application(_ application: NSApplication, open urls: [URL]) {
        urls.forEach { tabs?.open(path: $0.path) }
    }

    private func resolvePalette() -> ThemeColors {
        ThemeCatalog.palette(name: settings.theme, mode: settings.themeMode, custom: config.themesDir)
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
        // Restore saved geometry, else center.
        let w = settings.window
        if w.width > 0 && w.height > 0 && (w.x != 0 || w.y != 0) {
            window.setFrame(NSRect(x: w.x, y: w.y, width: w.width, height: w.height), display: false)
        } else {
            window.center()
        }
        installController()
        window.makeKeyAndOrderFront(nil)

        // Background optimization (issue #16): throttle tailing when the window
        // isn't visible on screen.
        NotificationCenter.default.addObserver(self, selector: #selector(occlusionChanged),
                                               name: NSWindow.didChangeOcclusionStateNotification,
                                               object: window)
    }

    @objc private func occlusionChanged() {
        tabs?.setBackgrounded(!window.occlusionState.contains(.visible))
    }

    // MARK: - Session persistence (issue #14)

    /// Load-modify-save so we never clobber fields (e.g. recentFiles) written to
    /// disk elsewhere during the session.
    private func updateSettings(_ mutate: (inout AppSettings) -> Void) {
        var s = config.loadSettings()
        mutate(&s)
        config.saveSettings(s)
        settings = s
    }

    private func persistSession() {
        guard tabs != nil else { return }
        let states = tabs.tabStates()
        let activePath = tabs.activePath ?? ""
        let frame = window.frame
        updateSettings {
            $0.tabs = states
            $0.lastActiveTabPath = activePath
            $0.window = WindowState(x: Int(frame.origin.x), y: Int(frame.origin.y),
                                    width: Int(frame.width), height: Int(frame.height),
                                    maximised: self.window.isZoomed)
        }
    }

    private func installController() {
        window.backgroundColor = palette.background
        tabs = TabController(config: config, settings: settings, palette: palette)
        tabs.onActiveFileChanged = { [weak self] path in
            self?.window.title = path.map { "ctail — \(($0 as NSString).lastPathComponent)" } ?? "ctail"
        }
        window.contentView = tabs.container
    }

    // MARK: - AppActions

    func openFileDialog() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.message = "Choose log file(s) to tail"
        if panel.runModal() == .OK { panel.urls.forEach { tabs.open(path: $0.path) } }
    }

    func findInLog() { tabs.openSearch() }

    func toggleTheme() {
        updateSettings { $0.themeMode = ($0.themeMode == "light") ? "dark" : "light" }
        // Live re-theme: rebuild the content with the new palette, preserving the
        // open files and active tab.
        let paths = tabs.openPaths
        let activeIdx = tabs.active
        palette = resolvePalette()
        installController()
        paths.forEach { tabs.open(path: $0) }
        if tabs.tabs.indices.contains(activeIdx) { tabs.activate(activeIdx) }
    }

    @objc func copySelection() { tabs.copyActiveSelection() }
    @objc func selectAllLines() { tabs.selectAllActive() }
    @objc private func closeActiveTab() { tabs.close(tabs.active) }
    @objc private func reopenTab() { tabs.reopenClosed() }
    @objc private func clearRecent() { config.clearRecentFiles() }

    @objc private func openRecent(_ sender: NSMenuItem) {
        if let path = sender.representedObject as? String { tabs.open(path: path) }
    }

    func showAbout() {
        let alert = NSAlert()
        alert.messageText = "ctail"
        alert.informativeText = "Native macOS log viewer\nVersion \(appVersion())\n© 2024–2026 André Biseth"
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    private func appVersion() -> String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }

    // Wired in their own issues — placeholders so the menu is complete now.
    func showSettings() { notYet("Settings", "#8") }
    func showAIAssistant() { notYet("AI Assistant", "#10") }
    func checkForUpdates() { notYet("Check for Updates", "#15") }

    private func notYet(_ name: String, _ issue: String) {
        let a = NSAlert()
        a.messageText = "\(name) — coming soon"
        a.informativeText = "This lands with issue \(issue) on the native port."
        a.runModal()
    }

    // MARK: - Menu

    private func buildMenu() {
        let main = NSMenu()

        // App menu.
        let appItem = NSMenuItem(); main.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About ctail", action: #selector(showAbout), keyEquivalent: "")
        appMenu.addItem(.separator())
        let prefs = appMenu.addItem(withTitle: "Settings…", action: #selector(showSettings), keyEquivalent: ",")
        prefs.target = self
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit ctail", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu

        // File menu (with dynamic Open Recent).
        let fileItem = NSMenuItem(); main.addItem(fileItem)
        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(withTitle: "Open…", action: #selector(openFileDialog), keyEquivalent: "o")
        let recentItem = fileMenu.addItem(withTitle: "Open Recent", action: nil, keyEquivalent: "")
        let recentMenu = NSMenu(title: "Open Recent")
        recentMenu.delegate = self                 // repopulated on open
        recentItem.submenu = recentMenu
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close Tab", action: #selector(closeActiveTab), keyEquivalent: "w")
        let reopen = fileMenu.addItem(withTitle: "Reopen Closed Tab", action: #selector(reopenTab), keyEquivalent: "t")
        reopen.keyEquivalentModifierMask = [.command, .shift]
        fileItem.submenu = fileMenu

        // Edit menu.
        let editItem = NSMenuItem(); main.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Copy", action: #selector(copySelection), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Select All", action: #selector(selectAllLines), keyEquivalent: "a")
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: "Find…", action: #selector(findInLog), keyEquivalent: "f")
        editItem.submenu = editMenu

        // View menu.
        let viewItem = NSMenuItem(); main.addItem(viewItem)
        let viewMenu = NSMenu(title: "View")
        viewMenu.addItem(withTitle: "Toggle Theme", action: #selector(toggleTheme), keyEquivalent: "")
        viewItem.submenu = viewMenu

        // Tools menu.
        let toolsItem = NSMenuItem(); main.addItem(toolsItem)
        let toolsMenu = NSMenu(title: "Tools")
        let ai = toolsMenu.addItem(withTitle: "AI Assistant…", action: #selector(showAIAssistant), keyEquivalent: "a")
        ai.keyEquivalentModifierMask = [.command, .shift]
        toolsItem.submenu = toolsMenu

        // Help menu.
        let helpItem = NSMenuItem(); main.addItem(helpItem)
        let helpMenu = NSMenu(title: "Help")
        helpMenu.addItem(withTitle: "Check for Updates", action: #selector(checkForUpdates), keyEquivalent: "")
        helpItem.submenu = helpMenu

        NSApp.mainMenu = main
    }

    /// Repopulate Open Recent each time it opens.
    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let recents = config.recentFiles()
        if recents.isEmpty {
            menu.addItem(withTitle: "(empty)", action: nil, keyEquivalent: "").isEnabled = false
            return
        }
        for path in recents {
            let item = menu.addItem(withTitle: (path as NSString).lastPathComponent,
                                    action: #selector(openRecent(_:)), keyEquivalent: "")
            item.representedObject = path
            item.target = self
            item.toolTip = path
        }
        menu.addItem(.separator())
        menu.addItem(withTitle: "Clear Recent", action: #selector(clearRecent), keyEquivalent: "").target = self
    }
}
