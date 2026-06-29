import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, AppActions, NSMenuDelegate {
    private var window: NSWindow!
    private var tabs: TabController!

    private let config = ConfigStore()
    private lazy var bookmarks = BookmarkStore(dir: config.dir)
    private var settings = AppSettings()
    private var palette = ThemeColors.placeholder

    func applicationDidFinishLaunching(_ notification: Notification) {
        settings = config.loadSettings()
        config.ensureDefaultProfile()
        palette = resolvePalette()
        // Set the Dock icon programmatically so it's correct however the app is
        // launched (the unbundled dev binary otherwise shows the generic icon).
        if let icon = Self.appIcon() { NSApp.applicationIconImage = icon }
        buildMenu()
        buildWindow()
        tabs.onTabsChanged = { [weak self] in self?.persistSession() }

        // StoreKit: load Pro entitlement and keep the menu in sync.
        StoreManager.shared.onChange = { [weak self] _ in self?.proStatusChanged() }
        StoreManager.shared.start()

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
        maybeAutoCheckUpdates()
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { persistSession() }
    func application(_ application: NSApplication, open urls: [URL]) {
        urls.forEach { bookmarks.save($0); tabs?.open(path: $0.path) }
    }

    private func resolvePalette() -> ThemeColors {
        // Defensive gate: a Pro theme only applies when Pro is unlocked (covers a
        // hand-edited settings.json too).
        let name = Pro.themeAllowed(settings.theme) ? settings.theme : Pro.fallbackTheme
        return ThemeCatalog.palette(name: name, mode: settings.themeMode, custom: config.themesDir)
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
        tabs?.shutdown()        // stop the outgoing controller's tailers before replacing it
        tabs = TabController(config: config, settings: settings, palette: palette, bookmarks: bookmarks)
        tabs.onActiveFileChanged = { [weak self] path in
            self?.window.title = path.map { "ctail — \(($0 as NSString).lastPathComponent)" } ?? "ctail"
        }
        tabs.onProRequired = { [weak self] feature in self?.showPaywall(feature: feature) }
        window.contentView = tabs.container
    }

    // MARK: - AppActions

    func openFileDialog() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.message = "Choose log file(s) to tail"
        if panel.runModal() == .OK {
            panel.urls.forEach { bookmarks.save($0); tabs.open(path: $0.path) }
        }
    }

    func findInLog() { tabs.openSearch() }

    func toggleTheme() {
        updateSettings { $0.themeMode = ($0.themeMode == "light") ? "dark" : "light" }
        rebuildContent()
    }

    /// Rebuilds the content (new palette/font/intervals) while preserving the
    /// open files and active tab. Used by Toggle Theme and Settings.
    private func rebuildContent() {
        let paths = tabs.openPaths
        let activeIdx = tabs.active
        palette = resolvePalette()
        installController()
        tabs.onTabsChanged = { [weak self] in self?.persistSession() }
        paths.forEach { tabs.open(path: $0, enforceLimit: false) }
        if tabs.tabs.indices.contains(activeIdx) { tabs.activate(activeIdx) }
    }

    func showSettings() {
        let controller = SettingsWindowController(
            settings: config.loadSettings(),
            themes: ThemeCatalog.all(custom: config.themesDir)) { [weak self] new in
                guard let self else { return }
                var new = new
                // Theme gate: a Pro theme picked without Pro reverts to the free
                // default and prompts the paywall.
                let pickedLockedTheme = !Pro.themeAllowed(new.theme)
                if pickedLockedTheme { new.theme = Pro.fallbackTheme }
                config.saveSettings(new)
                self.settings = new
                self.rebuildContent()
                if pickedLockedTheme { self.showPaywall(feature: .themes) }
        }
        settingsWindow = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }
    private var settingsWindow: SettingsWindowController?

    func showProfiles() {
        let controller = ProfilesWindowController(config: config, palette: palette) { [weak self] name in
            guard let self else { return }
            self.updateSettings { $0.activeProfile = name }
            self.rebuildContent()      // apply the newly active profile's rules
        }
        profilesWindow = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }
    private var profilesWindow: ProfilesWindowController?

    @objc func copySelection() { tabs.copyActiveSelection() }
    @objc func selectAllLines() { tabs.selectAllActive() }
    @objc private func closeActiveTab() { tabs.close(tabs.active) }
    @objc private func reopenTab() { tabs.reopenClosed() }
    @objc private func clearRecent() { config.clearRecentFiles() }

    @objc private func openRecent(_ sender: NSMenuItem) {
        if let path = sender.representedObject as? String { tabs.open(path: path) }
    }

    func showAbout() {
        let credits = NSAttributedString(
            string: "Fast native log tailer for huge files — syntax highlighting, search, "
                  + "disk-backed scrollback, and an AI assistant.\n\nCopyright © 2024–2026 André Biseth",
            attributes: [.font: NSFont.systemFont(ofSize: 11),
                         .foregroundColor: NSColor.secondaryLabelColor])
        var options: [NSApplication.AboutPanelOptionKey: Any] = [
            .applicationName: "ctail",
            .applicationVersion: appVersion(),
            .credits: credits,
        ]
        if let icon = Self.appIcon() { options[.applicationIcon] = icon }
        NSApp.orderFrontStandardAboutPanel(options: options)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func appVersion() -> String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }

    /// The app icon, resolved across all build flavors:
    ///   - App Store / Xcode app target → the `AppIcon` asset catalog
    ///   - bundled `.app` from the Makefile → the `.icns`
    ///   - unbundled SwiftPM dev binary → the bundled `appicon.png` resource
    /// `Bundle.module` only exists under SwiftPM, so guard it with `SWIFT_PACKAGE`
    /// (defined only by SwiftPM) — otherwise an Xcode app target won't compile.
    static func appIcon() -> NSImage? {
        if let asset = NSImage(named: "AppIcon") { return asset }
        #if SWIFT_PACKAGE
        if let url = Bundle.module.url(forResource: "appicon", withExtension: "png"),
           let img = NSImage(contentsOf: url) { return img }
        #endif
        return nil
    }

    func showAIAssistant() {
        guard Pro.isUnlocked else { showPaywall(feature: .ai); return }
        let controller = AIAssistantWindowController(
            settings: config.loadSettings(), config: config,
            logProvider: { [weak self] in self?.tabs.activeLogContext() ?? "" },
            onProfileGenerated: { [weak self] name in
                guard let self else { return }
                self.updateSettings { $0.activeProfile = name }
                self.rebuildContent()
            })
        aiWindow = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }
    private var aiWindow: AIAssistantWindowController?

    // MARK: - ctail Pro (StoreKit)

    private var paywall: PaywallWindowController?

    func showPaywall(feature: Pro.Feature? = nil) {
        if Pro.isUnlocked { alert("ctail Pro", "You already have ctail Pro. Thank you!"); return }
        if let existing = paywall?.window { existing.makeKeyAndOrderFront(nil); return }
        let controller = PaywallWindowController(feature: feature) { [weak self] in
            self?.proStatusChanged()
            self?.rebuildContent()        // apply any now-unlocked theme immediately
        }
        paywall = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }

    @objc private func unlockPro() { showPaywall() }

    @objc private func restorePurchases() {
        Task {
            await StoreManager.shared.restore()
            await MainActor.run {
                self.proStatusChanged()
                self.alert(StoreManager.shared.isPro ? "ctail Pro restored" : "Nothing to restore",
                           StoreManager.shared.isPro
                            ? "Your Pro features are unlocked."
                            : "No previous ctail Pro purchase was found for this Apple ID.")
            }
        }
    }

    /// Reflect entitlement changes in the menu.
    private func proStatusChanged() {
        unlockProItem?.isHidden = Pro.isUnlocked
        restoreItem?.isHidden = Pro.isUnlocked
        proActiveItem?.isHidden = !Pro.isUnlocked
    }
    private weak var unlockProItem: NSMenuItem?
    private weak var restoreItem: NSMenuItem?
    private weak var proActiveItem: NSMenuItem?

    #if DEBUG
    private weak var devProItem: NSMenuItem?
    /// Dev-only: toggle the Pro override and apply it live (re-applies theme gating).
    @objc private func toggleDevPro() {
        Pro.devUnlocked.toggle()
        devProItem?.state = Pro.devUnlocked ? .on : .off
        proStatusChanged()
        rebuildContent()
    }
    #endif

    func checkForUpdates() { runUpdateCheck(manual: true) }

    /// Auto-check on launch when enabled and the interval has elapsed (tracked in
    /// UserDefaults so it needs no settings-schema change). Quiet unless an
    /// update is found.
    private func maybeAutoCheckUpdates() {
        guard !settings.disableUpdateCheck else { return }
        let key = "lastUpdateCheck"
        let last = UserDefaults.standard.double(forKey: key)
        let now = Date().timeIntervalSince1970
        let intervalSec = Double(max(1, settings.updateCheckIntervalHours)) * 3600
        guard now - last >= intervalSec else { return }
        UserDefaults.standard.set(now, forKey: key)
        runUpdateCheck(manual: false)
    }

    private func runUpdateCheck(manual: Bool) {
        UpdateChecker.check(current: appVersion()) { [weak self] r in
            guard let self else { return }
            if let error = r.error { if manual { self.alert("Update check failed", error) }; return }
            if r.updateAvailable {
                let a = NSAlert()
                a.messageText = "Update available: \(r.latest)"
                a.informativeText = "You have \(r.current).\n\n" + String(r.notes.prefix(500))
                a.addButton(withTitle: "Download")
                a.addButton(withTitle: "Later")
                if a.runModal() == .alertFirstButtonReturn, let url = URL(string: r.url) {
                    NSWorkspace.shared.open(url)
                }
            } else if manual {
                self.alert("You're up to date", "ctail \(r.current) is the latest version.")
            }
        }
    }

    private func alert(_ title: String, _ body: String) {
        let a = NSAlert(); a.messageText = title; a.informativeText = body; a.runModal()
    }

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
        let unlock = appMenu.addItem(withTitle: "Unlock ctail Pro…", action: #selector(unlockPro), keyEquivalent: "")
        unlock.target = self
        let restore = appMenu.addItem(withTitle: "Restore Purchases", action: #selector(restorePurchases), keyEquivalent: "")
        restore.target = self
        let proActive = appMenu.addItem(withTitle: "ctail Pro ✓ Unlocked", action: nil, keyEquivalent: "")
        proActive.isEnabled = false
        proActive.isHidden = true
        unlockProItem = unlock; restoreItem = restore; proActiveItem = proActive
        #if DEBUG
        let devPro = appMenu.addItem(withTitle: "Unlock Pro (dev)", action: #selector(toggleDevPro), keyEquivalent: "")
        devPro.target = self
        devPro.state = Pro.devUnlocked ? .on : .off
        devProItem = devPro
        #endif
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
        viewMenu.addItem(withTitle: "Profiles & Rules…", action: #selector(showProfiles), keyEquivalent: "")
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
