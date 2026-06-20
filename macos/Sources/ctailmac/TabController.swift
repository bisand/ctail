import AppKit

/// Owns the open tabs and the window's content area: a tab strip on top, the
/// active tab's LogView in the middle, and a status bar at the bottom. Handles
/// open/close/switch/reorder/rename/color, reopen-closed, and Ctrl+Tab nav.
final class TabController: NSObject {
    let container = NSView()
    private let tabBar: TabBarView
    private let content = NSView()
    private lazy var searchBar = makeSearchBar()
    private let statusLabel = NSTextField(labelWithString: "")
    private let followLabel = NSTextField(labelWithString: "")
    private let statusBar = NSView()

    private let config: ConfigStore
    private var settings: AppSettings
    private var palette: ThemeColors

    private(set) var tabs: [Tab] = []
    private(set) var active = -1
    private var lastActive = -1
    private var closedStack: [String] = []
    private var keyMonitor: Any?

    var onActiveFileChanged: ((String?) -> Void)?
    var onTabsChanged: (() -> Void)?

    init(config: ConfigStore, settings: AppSettings, palette: ThemeColors) {
        self.config = config
        self.settings = settings
        self.palette = palette
        self.tabBar = TabBarView(palette: palette)
        super.init()
        buildLayout()
        installKeyMonitor()
    }

    deinit { if let m = keyMonitor { NSEvent.removeMonitor(m) } }

    // MARK: - Layout

    private func buildLayout() {
        [tabBar, content, statusBar].forEach {
            $0.translatesAutoresizingMaskIntoConstraints = false
            container.addSubview($0)
        }
        content.wantsLayer = true
        content.layer?.backgroundColor = palette.background.cgColor

        statusBar.wantsLayer = true
        statusBar.layer?.backgroundColor = palette.backgroundAlt.cgColor
        statusLabel.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        statusLabel.textColor = palette.foreground
        followLabel.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        followLabel.textColor = palette.successColor
        followLabel.alignment = .right
        [statusLabel, followLabel].forEach {
            $0.translatesAutoresizingMaskIntoConstraints = false
            statusBar.addSubview($0)
        }

        NSLayoutConstraint.activate([
            tabBar.topAnchor.constraint(equalTo: container.topAnchor),
            tabBar.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            tabBar.heightAnchor.constraint(equalToConstant: 34),
            content.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            content.bottomAnchor.constraint(equalTo: statusBar.topAnchor),
            statusBar.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            statusBar.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            statusBar.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            statusBar.heightAnchor.constraint(equalToConstant: 24),
            statusLabel.leadingAnchor.constraint(equalTo: statusBar.leadingAnchor, constant: 10),
            statusLabel.centerYAnchor.constraint(equalTo: statusBar.centerYAnchor),
            followLabel.trailingAnchor.constraint(equalTo: statusBar.trailingAnchor, constant: -10),
            followLabel.centerYAnchor.constraint(equalTo: statusBar.centerYAnchor),
        ])

        tabBar.onSelect = { [weak self] i in self?.activate(i) }
        tabBar.onClose = { [weak self] i in self?.close(i) }
        tabBar.onNew = { [weak self] in NSApp.sendAction(#selector(AppActions.openFileDialog), to: nil, from: nil) }
        tabBar.onReorder = { [weak self] from, to in self?.reorder(from: from, to: to) }
        tabBar.onRename = { [weak self] i in self?.promptRename(i) }
        tabBar.onContext = { [weak self] i, e in self?.showContextMenu(i, e) }
    }

    // MARK: - Tab lifecycle

    private func rules(for profileName: String) -> [HighlightRule] {
        let profile = config.loadProfile(profileName)
            ?? config.loadProfile(settings.activeProfile)
            ?? Defaults.commonLogsProfile()
        return HighlightRule.compile(profile)
    }

    @discardableResult
    func open(path: String) -> Tab {
        // If already open, just focus it.
        if let i = tabs.firstIndex(where: { $0.filePath == path }) { activate(i); return tabs[i] }

        let tab = Tab(filePath: path, palette: palette, rules: rules(for: settings.activeProfile),
                      profileName: settings.activeProfile,
                      pollInterval: Double(settings.pollIntervalMs) / 1000.0,
                      readTimeout: Double(settings.readTimeoutSec))
        wire(tab)
        config.addRecentFile(path)

        let insertAt = (settings.newTabPosition == "afterActive" && active >= 0) ? active + 1 : tabs.count
        tabs.insert(tab, at: min(insertAt, tabs.count))
        tab.tailer.start()
        activate(tabs.firstIndex(where: { $0.id == tab.id })!)
        return tab
    }

    private func wire(_ tab: Tab) {
        tab.tailer.onLines = { [weak self, weak tab] lines in
            tab?.logView.append(lines); self?.refreshStatusIfActive(tab)
        }
        tab.tailer.onReset = { [weak tab] in tab?.logView.reset() }
        tab.tailer.onReady = { [weak self, weak tab] in self?.refreshStatusIfActive(tab) }
        tab.tailer.onIndexed = { [weak self, weak tab] _ in self?.refreshStatusIfActive(tab) }
        tab.tailer.onError = { [weak self, weak tab] msg in
            guard let self, let tab, self.activeTab?.id == tab.id else { return }
            self.statusLabel.stringValue = "⚠︎ \(msg)"
        }
        tab.logView.onFollowingChanged = { [weak self, weak tab] _ in self?.refreshStatusIfActive(tab) }
        tab.logView.menu = makeLogMenu()
    }

    func close(_ index: Int) {
        guard tabs.indices.contains(index) else { return }
        let tab = tabs[index]
        tab.tailer.stop()
        closedStack.append(tab.filePath)
        tab.logView.removeFromSuperview()
        tabs.remove(at: index)

        if tabs.isEmpty { active = -1; onActiveFileChanged?(nil); reloadBar(); statusLabel.stringValue = ""; return }
        active = min(index, tabs.count - 1)
        showActiveContent()
    }

    func reopenClosed() {
        guard let path = closedStack.popLast() else { return }
        if FileManager.default.fileExists(atPath: path) { open(path: path) }
    }

    func activate(_ index: Int) {
        guard tabs.indices.contains(index) else { return }
        if active != index { lastActive = active }
        active = index
        showActiveContent()
    }

    var activeTab: Tab? { tabs.indices.contains(active) ? tabs[active] : nil }
    var openPaths: [String] { tabs.map { $0.filePath } }
    var activePath: String? { activeTab?.filePath }

    // MARK: - Session snapshot / restore (issue #14)

    func tabStates() -> [TabState] {
        tabs.enumerated().map { $0.element.toState(position: $0.offset) }
    }

    /// Reopens persisted tabs (skipping files that no longer exist), restoring
    /// label/color/profile, then activates the previously active tab.
    func restore(_ states: [TabState], activePath: String?) {
        for s in states.sorted(by: { $0.position < $1.position }) {
            guard FileManager.default.fileExists(atPath: s.filePath) else { continue }
            let tab = open(path: s.filePath)
            tab.label = s.label
            tab.color = s.color
            if !s.profileId.isEmpty { tab.profileName = s.profileId }
        }
        reloadBar()
        if let activePath, let i = tabs.firstIndex(where: { $0.filePath == activePath }) { activate(i) }
    }

    func copyActiveSelection() {
        guard let text = activeTab?.logView.selectedText(), !text.isEmpty else { return }
        FileOps.copyText(text)
    }
    func selectAllActive() { activeTab?.logView.selectAllRows() }

    private weak var shownLogView: LogView?

    private func showActiveContent() {
        shownLogView?.removeFromSuperview()        // swap only the log view, keep the search bar
        if let tab = activeTab {
            tab.logView.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(tab.logView, positioned: .below, relativeTo: searchBar)
            NSLayoutConstraint.activate([
                tab.logView.topAnchor.constraint(equalTo: content.topAnchor),
                tab.logView.bottomAnchor.constraint(equalTo: content.bottomAnchor),
                tab.logView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
                tab.logView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            ])
            shownLogView = tab.logView
            onActiveFileChanged?(tab.filePath)
            if !searchBar.isHidden { runSearch(resetPosition: true) }
        }
        reloadBar()
        refreshStatus()
        applyIntervals()
        onTabsChanged?()
    }

    // MARK: - Reorder / rename / color

    func reorder(from: Int, to: Int) {
        guard tabs.indices.contains(from), to >= 0, to < tabs.count, from != to else { return }
        let moving = tabs[active]
        let t = tabs.remove(at: from)
        tabs.insert(t, at: to)
        active = tabs.firstIndex(where: { $0.id == moving.id }) ?? to
        reloadBar()
    }

    // MARK: - Background optimization (issue #16)

    private var backgrounded = false
    private var activeInterval: TimeInterval { max(0.05, Double(settings.pollIntervalMs) / 1000.0) }
    private var inactiveInterval: TimeInterval { max(2.0, activeInterval * 4) }
    private let backgroundInterval: TimeInterval = 5.0

    /// Called when the window's occlusion state changes. When fully hidden we
    /// slow every tailer right down; when visible we restore the active/inactive
    /// cadence.
    func setBackgrounded(_ b: Bool) {
        guard b != backgrounded else { return }
        backgrounded = b
        applyIntervals()
    }

    /// Active tab polls fast; inactive tabs poll slowly (they keep tailing so the
    /// buffer is warm on switch); everything crawls when backgrounded.
    private func applyIntervals() {
        for (i, tab) in tabs.enumerated() {
            let interval = backgrounded ? backgroundInterval : (i == active ? activeInterval : inactiveInterval)
            tab.tailer.setPollInterval(interval)
        }
    }

    func nextTab() { guard !tabs.isEmpty else { return }; activate((active + 1) % tabs.count) }
    func prevTab() { guard !tabs.isEmpty else { return }; activate((active - 1 + tabs.count) % tabs.count) }
    func quickToggle() { if tabs.indices.contains(lastActive) { activate(lastActive) } }

    private func promptRename(_ index: Int) {
        guard tabs.indices.contains(index) else { return }
        let tab = tabs[index]
        let alert = NSAlert()
        alert.messageText = "Rename Tab"
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        field.stringValue = tab.displayName
        alert.accessoryView = field
        alert.addButton(withTitle: "Rename")
        alert.addButton(withTitle: "Cancel")
        if alert.runModal() == .alertFirstButtonReturn {
            tab.label = field.stringValue
            reloadBar()
        }
    }

    func setColor(_ index: Int, _ hex: String) {
        guard tabs.indices.contains(index) else { return }
        tabs[index].color = hex
        reloadBar()
    }

    // MARK: - Context menus (issue #12)

    private func showContextMenu(_ index: Int, _ event: NSEvent) {
        guard tabs.indices.contains(index) else { return }
        ctxIndex = index
        let menu = NSMenu()
        addItem(menu, "Rename…", #selector(ctxRename))
        let colorItem = NSMenuItem(title: "Color", action: nil, keyEquivalent: "")
        colorItem.submenu = colorSubmenu()
        menu.addItem(colorItem)
        addItem(menu, "Refresh", #selector(ctxRefresh))
        menu.addItem(.separator())
        addItem(menu, "Change File Path…", #selector(ctxChangePath))
        addItem(menu, "Copy Path", #selector(ctxCopyPath))
        addItem(menu, "Reveal in Finder", #selector(ctxReveal))
        menu.addItem(.separator())
        addItem(menu, "Close Tab", #selector(ctxClose))
        NSMenu.popUpContextMenu(menu, with: event, for: tabBar)
    }

    private var ctxIndex = -1
    private let tabColors = ["#f38ba8", "#fab387", "#f9e2af", "#a6e3a1", "#89b4fa", "#cba6f7"]

    private func addItem(_ menu: NSMenu, _ title: String, _ action: Selector) {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        menu.addItem(item)
    }

    private func colorSubmenu() -> NSMenu {
        let sub = NSMenu()
        for (i, hex) in tabColors.enumerated() {
            let item = NSMenuItem(title: "Color \(i + 1)", action: #selector(ctxSetColor(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = hex
            let swatch = NSImage(size: NSSize(width: 12, height: 12))
            swatch.lockFocus(); Theme.hex(hex).setFill(); NSRect(x: 0, y: 0, width: 12, height: 12).fill(); swatch.unlockFocus()
            item.image = swatch
            sub.addItem(item)
        }
        sub.addItem(.separator())
        let none = NSMenuItem(title: "No Color", action: #selector(ctxSetColor(_:)), keyEquivalent: "")
        none.target = self; none.representedObject = ""
        sub.addItem(none)
        return sub
    }

    @objc private func ctxRename() { promptRename(ctxIndex) }
    @objc private func ctxRefresh() { guard tabs.indices.contains(ctxIndex) else { return }; tabs[ctxIndex].logView.reset(); tabs[ctxIndex].tailer.refresh() }
    @objc private func ctxCopyPath() { guard tabs.indices.contains(ctxIndex) else { return }; FileOps.copyPath(tabs[ctxIndex].filePath) }
    @objc private func ctxReveal() { guard tabs.indices.contains(ctxIndex) else { return }; FileOps.revealInFinder(tabs[ctxIndex].filePath) }
    @objc private func ctxClose() { close(ctxIndex) }
    @objc private func ctxSetColor(_ sender: NSMenuItem) { setColor(ctxIndex, (sender.representedObject as? String) ?? "") }

    @objc private func ctxChangePath() {
        guard tabs.indices.contains(ctxIndex) else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true; panel.allowsMultipleSelection = false
        panel.message = "Point this tab at a different file"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let pos = ctxIndex
        close(pos)
        open(path: url.path)
    }

    /// Builds the log-area right-click menu (assigned to each tab's LogView).
    func makeLogMenu() -> NSMenu {
        let menu = NSMenu()
        addItem(menu, "Copy", #selector(logCopy))
        addItem(menu, "Select All", #selector(logSelectAll))
        menu.addItem(.separator())
        addItem(menu, "Ask AI about selection…", #selector(logAskAI))
        return menu
    }

    @objc private func logCopy() { copyActiveSelection() }
    @objc private func logSelectAll() { selectAllActive() }
    @objc private func logAskAI() { NSApp.sendAction(#selector(AppActions.showAIAssistant), to: nil, from: nil) }

    // MARK: - Status

    private func refreshStatusIfActive(_ tab: Tab?) {
        guard let tab, tab.id == activeTab?.id else { return }
        refreshStatus()
    }

    private func refreshStatus() {
        guard let tab = activeTab else { statusLabel.stringValue = ""; followLabel.stringValue = ""; return }
        let total = max(tab.tailer.totalLines, Int64(tab.logView.lineCount))
        let indexing = tab.tailer.indexingComplete ? "" : " — indexing…"
        statusLabel.stringValue = "\(tab.displayName) · \(total) lines\(indexing)"
        let following = tab.logView.following
        followLabel.stringValue = following ? "● FOLLOWING" : "○ paused"
        followLabel.textColor = following ? palette.successColor : palette.muted
    }

    private func reloadBar() {
        tabBar.reload(titles: tabs.map { ($0.displayName, $0.color) }, active: active)
    }

    // MARK: - Search (issue #9)

    private func makeSearchBar() -> SearchBar {
        let bar = SearchBar(palette: palette)
        bar.translatesAutoresizingMaskIntoConstraints = false
        bar.isHidden = true
        bar.onChange = { [weak self] in self?.runSearch(resetPosition: true) }
        bar.onNext = { [weak self] in self?.stepSearch(forward: true) }
        bar.onPrev = { [weak self] in self?.stepSearch(forward: false) }
        bar.onClose = { [weak self] in self?.closeSearch() }
        content.addSubview(bar)
        NSLayoutConstraint.activate([
            bar.topAnchor.constraint(equalTo: content.topAnchor, constant: 8),
            bar.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
        ])
        return bar
    }

    func openSearch() {
        guard activeTab != nil else { return }
        searchBar.isHidden = false
        content.addSubview(searchBar)            // keep above the log view
        searchBar.focusField()
        runSearch(resetPosition: true)
    }

    private func closeSearch() {
        searchBar.isHidden = true
        activeTab?.logView.clearSearch()
        refreshStatus()
    }

    private func runSearch(resetPosition: Bool) {
        guard let log = activeTab?.logView else { return }
        let r = log.search(text: searchBar.queryText, caseSensitive: searchBar.caseSensitive,
                           wholeWord: searchBar.wholeWord, isRegex: searchBar.isRegex,
                           filter: searchBar.filterMode)
        searchBar.setCounter(total: r.total, current: r.current, valid: log.searchIsValid)
    }

    private func stepSearch(forward: Bool) {
        guard let log = activeTab?.logView else { return }
        let r = forward ? log.nextMatch() : log.prevMatch()
        searchBar.setCounter(total: r.total, current: r.current, valid: log.searchIsValid)
    }

    // MARK: - Keyboard (Ctrl+Tab / Ctrl+Shift+Tab + Cmd+F)

    private func installKeyMonitor() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] e in
            guard let self else { return e }
            // Ctrl+Tab / Ctrl+Shift+Tab — tab key is 48.
            if e.keyCode == 48, e.modifierFlags.contains(.control) {
                e.modifierFlags.contains(.shift) ? self.prevTab() : self.nextTab()
                return nil
            }
            return e
        }
    }
}

/// Selector target protocol so the "+" button can route to the app's open dialog.
@objc protocol AppActions {
    func openFileDialog()
    func findInLog()
    func toggleTheme()
    func showAbout()
    func showSettings()
    func showAIAssistant()
    func checkForUpdates()
}
