import AppKit

/// Settings window in the modern macOS "preferences" idiom: a centered toolbar
/// whose items switch between panes (Appearance / Behavior / Updates / AI), each
/// laid out with an NSGridView so labels and controls align cleanly, plus a
/// persistent Cancel/Save bar. Edits a working copy of AppSettings and hands it
/// back on Save; the caller persists and applies it.
final class SettingsWindowController: NSWindowController, NSToolbarDelegate {
    private var settings: AppSettings
    private let themes: [Theme]
    private let onSave: (AppSettings) -> Void

    // Controls we read back on save.
    private let themePopup = NSPopUpButton()
    private let modePopup = NSPopUpButton()
    private let fontField = NSTextField()
    private let pollField = NSTextField()
    private let bufferField = NSTextField()
    private let scrollbackField = NSTextField()
    private let timeoutField = NSTextField()
    private let lineNumbers = NSButton(checkboxWithTitle: "Show line numbers", target: nil, action: nil)
    private let wordWrap = NSButton(checkboxWithTitle: "Word wrap", target: nil, action: nil)
    private let restoreTabs = NSButton(checkboxWithTitle: "Restore tabs on launch", target: nil, action: nil)
    private let newTabPopup = NSPopUpButton()
    private let disableUpdates = NSButton(checkboxWithTitle: "Disable update check", target: nil, action: nil)
    private let updateIntervalField = NSTextField()
    private let aiProviderPopup = NSPopUpButton()
    private let aiEndpointField = NSTextField()
    private let aiKeyField = NSSecureTextField()
    private let aiModelField = NSTextField()

    // Panes, keyed by toolbar item.
    private let paneHost = NSView()
    private var panes: [NSToolbarItem.Identifier: NSView] = [:]
    private let buttonBarHeight: CGFloat = 52
    private let contentWidth: CGFloat = 540

    private static let appearance = NSToolbarItem.Identifier("appearance")
    private static let behavior = NSToolbarItem.Identifier("behavior")
    private static let updates = NSToolbarItem.Identifier("updates")
    private static let ai = NSToolbarItem.Identifier("ai")
    private let paneOrder: [NSToolbarItem.Identifier] = [appearance, behavior, updates, ai]
    private let paneTitles: [NSToolbarItem.Identifier: String] =
        [appearance: "Appearance", behavior: "Behavior", updates: "Updates", ai: "AI Assistant"]
    private let paneSymbols: [NSToolbarItem.Identifier: String] =
        [appearance: "paintpalette", behavior: "slider.horizontal.3",
         updates: "arrow.triangle.2.circlepath", ai: "sparkles"]

    init(settings: AppSettings, themes: [Theme], onSave: @escaping (AppSettings) -> Void) {
        self.settings = settings
        self.themes = themes
        self.onSave = onSave
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 540, height: 420),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Settings"
        super.init(window: window)

        configurePopups()
        buildPanes()
        window.contentView = buildRoot()

        let toolbar = NSToolbar(identifier: "SettingsToolbar")
        toolbar.delegate = self
        toolbar.allowsUserCustomization = false
        toolbar.displayMode = .iconAndLabel
        if #available(macOS 11.0, *) { window.toolbarStyle = .preference }
        window.toolbar = toolbar
        toolbar.selectedItemIdentifier = Self.appearance

        load()
        show(pane: Self.appearance)
        window.center()
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Layout

    private func configurePopups() {
        for v in [themePopup, modePopup, newTabPopup, aiProviderPopup] {
            v.translatesAutoresizingMaskIntoConstraints = false
        }
        modePopup.addItems(withTitles: ["dark", "light"])
        newTabPopup.addItems(withTitles: ["end", "afterActive"])
        aiProviderPopup.addItems(withTitles: ["", "openai", "github", "copilot", "custom"])
        themes.forEach {
            // Mark Pro-only themes with a lock; selecting one prompts the paywall
            // on save (the real theme name stays in representedObject).
            let locked = !Pro.themeAllowed($0.name)
            themePopup.addItem(withTitle: $0.displayName + (locked ? "  🔒" : ""))
            themePopup.lastItem?.representedObject = $0.name
        }
    }

    private func buildPanes() {
        panes[Self.appearance] = pane([
            row("Theme", themePopup),
            row("Mode", modePopup),
            row("Font size", fontField),
            check(lineNumbers),
            check(wordWrap),
        ])
        panes[Self.behavior] = pane([
            row("Poll interval (ms)", pollField),
            row("Buffer size (lines)", bufferField),
            row("Scrollback (lines)", scrollbackField),
            row("Read timeout (s)", timeoutField),
            check(restoreTabs),
            row("New tab position", newTabPopup),
        ])
        panes[Self.updates] = pane([
            check(disableUpdates),
            row("Check interval (h)", updateIntervalField),
        ])
        panes[Self.ai] = pane([
            row("Provider", aiProviderPopup),
            row("Endpoint", aiEndpointField),
            row("API key", aiKeyField),
            row("Model", aiModelField),
        ])
    }

    private func buildRoot() -> NSView {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: contentWidth, height: 420))

        let save = NSButton(title: "Save", target: self, action: #selector(saveTapped))
        save.keyEquivalent = "\r"
        save.bezelStyle = .rounded
        let cancel = NSButton(title: "Cancel", target: self, action: #selector(cancelTapped))
        cancel.bezelStyle = .rounded
        cancel.keyEquivalent = "\u{1b}"
        let buttons = NSStackView(views: [cancel, save])
        buttons.spacing = 12
        buttons.translatesAutoresizingMaskIntoConstraints = false

        let divider = NSBox(); divider.boxType = .separator
        divider.translatesAutoresizingMaskIntoConstraints = false
        paneHost.translatesAutoresizingMaskIntoConstraints = false

        [paneHost, divider, buttons].forEach { root.addSubview($0) }
        NSLayoutConstraint.activate([
            paneHost.topAnchor.constraint(equalTo: root.topAnchor),
            paneHost.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            paneHost.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            paneHost.bottomAnchor.constraint(equalTo: divider.topAnchor),
            divider.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            divider.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            divider.bottomAnchor.constraint(equalTo: buttons.topAnchor, constant: -10),
            buttons.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -20),
            buttons.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -14),
        ])
        return root
    }

    /// Builds one settings pane: an NSGridView (right-aligned labels, controls in
    /// column 1) inset within a host view sized to its content.
    private func pane(_ rows: [[NSView]]) -> NSView {
        let grid = NSGridView(views: rows)
        grid.translatesAutoresizingMaskIntoConstraints = false
        grid.rowSpacing = 12
        grid.columnSpacing = 12
        grid.column(at: 0).xPlacement = .trailing
        grid.rowAlignment = .firstBaseline

        let host = NSView()
        host.addSubview(grid)
        NSLayoutConstraint.activate([
            grid.topAnchor.constraint(equalTo: host.topAnchor, constant: 26),
            grid.leadingAnchor.constraint(equalTo: host.leadingAnchor, constant: 26),
            grid.trailingAnchor.constraint(lessThanOrEqualTo: host.trailingAnchor, constant: -26),
            grid.bottomAnchor.constraint(equalTo: host.bottomAnchor, constant: -26),
        ])
        return host
    }

    private func row(_ label: String, _ control: NSView) -> [NSView] {
        let l = NSTextField(labelWithString: label)
        l.alignment = .right
        if let field = control as? NSTextField {
            field.translatesAutoresizingMaskIntoConstraints = false
            field.widthAnchor.constraint(equalToConstant: 260).isActive = true
        } else if let popup = control as? NSPopUpButton {
            popup.widthAnchor.constraint(greaterThanOrEqualToConstant: 200).isActive = true
        }
        return [l, control]
    }

    /// A checkbox row: empty label cell so the box aligns under the controls column.
    private func check(_ box: NSButton) -> [NSView] {
        [NSGridCell.emptyContentView, box]
    }

    // MARK: - Toolbar / pane switching

    private func show(pane id: NSToolbarItem.Identifier) {
        guard let view = panes[id] else { return }
        paneHost.subviews.forEach { $0.removeFromSuperview() }
        view.translatesAutoresizingMaskIntoConstraints = false
        paneHost.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: paneHost.topAnchor),
            view.leadingAnchor.constraint(equalTo: paneHost.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: paneHost.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: paneHost.bottomAnchor),
        ])
        window?.title = paneTitles[id] ?? "Settings"
        resizeWindow(toFit: view)
    }

    /// Sizes the window so the pane fits exactly above the button bar.
    private func resizeWindow(toFit pane: NSView) {
        guard let window else { return }
        pane.layoutSubtreeIfNeeded()
        let paneHeight = max(pane.fittingSize.height, 120)
        let contentHeight = paneHeight + buttonBarHeight
        var frame = window.frame
        let delta = contentHeight - (window.contentView?.frame.height ?? contentHeight)
        frame.origin.y -= delta                     // grow/shrink from the top
        frame.size.height += delta
        frame.size.width = contentWidth + (frame.width - (window.contentView?.frame.width ?? contentWidth))
        window.setFrame(frame, display: true, animate: window.isVisible)
    }

    @objc private func selectPane(_ sender: NSToolbarItem) { show(pane: sender.itemIdentifier) }

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier id: NSToolbarItem.Identifier,
                 willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        let item = NSToolbarItem(itemIdentifier: id)
        item.label = paneTitles[id] ?? ""
        item.image = NSImage(systemSymbolName: paneSymbols[id] ?? "gearshape",
                             accessibilityDescription: item.label)
        item.target = self
        item.action = #selector(selectPane(_:))
        item.isBordered = true
        return item
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { paneOrder }
    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { paneOrder }
    func toolbarSelectableItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { paneOrder }

    // MARK: - Load / save

    private func load() {
        selectByRepresented(themePopup, settings.theme)
        modePopup.selectItem(withTitle: settings.themeMode)
        fontField.stringValue = String(settings.fontSize)
        pollField.stringValue = String(settings.pollIntervalMs)
        bufferField.stringValue = String(settings.bufferSize)
        scrollbackField.stringValue = String(settings.scrollBuffer)
        timeoutField.stringValue = String(settings.readTimeoutSec)
        lineNumbers.state = settings.showLineNumbers ? .on : .off
        wordWrap.state = settings.wordWrap ? .on : .off
        restoreTabs.state = settings.restoreTabs ? .on : .off
        newTabPopup.selectItem(withTitle: settings.newTabPosition)
        disableUpdates.state = settings.disableUpdateCheck ? .on : .off
        updateIntervalField.stringValue = String(settings.updateCheckIntervalHours)
        aiProviderPopup.selectItem(withTitle: settings.aiProvider)
        aiEndpointField.stringValue = settings.aiEndpoint
        aiKeyField.stringValue = settings.aiKey
        aiModelField.stringValue = settings.aiModel
    }

    @objc private func saveTapped() {
        var s = settings
        s.theme = (themePopup.selectedItem?.representedObject as? String) ?? s.theme
        s.themeMode = modePopup.titleOfSelectedItem ?? s.themeMode
        s.fontSize = intOr(fontField, s.fontSize, min: 8, max: 32)
        s.pollIntervalMs = intOr(pollField, s.pollIntervalMs, min: 50, max: 60000)
        s.bufferSize = intOr(bufferField, s.bufferSize, min: 100, max: 10_000_000)
        s.scrollBuffer = intOr(scrollbackField, s.scrollBuffer, min: 0, max: 1_000_000)
        s.readTimeoutSec = intOr(timeoutField, s.readTimeoutSec, min: 1, max: 600)
        s.showLineNumbers = lineNumbers.state == .on
        s.wordWrap = wordWrap.state == .on
        s.restoreTabs = restoreTabs.state == .on
        s.newTabPosition = newTabPopup.titleOfSelectedItem ?? s.newTabPosition
        s.disableUpdateCheck = disableUpdates.state == .on
        s.updateCheckIntervalHours = intOr(updateIntervalField, s.updateCheckIntervalHours, min: 1, max: 720)
        s.aiProvider = aiProviderPopup.titleOfSelectedItem ?? ""
        s.aiEndpoint = aiEndpointField.stringValue
        s.aiKey = aiKeyField.stringValue
        s.aiModel = aiModelField.stringValue
        onSave(s)
        close()
    }

    @objc private func cancelTapped() { close() }

    private func intOr(_ field: NSTextField, _ fallback: Int, min lo: Int, max hi: Int) -> Int {
        guard let v = Int(field.stringValue.trimmingCharacters(in: .whitespaces)) else { return fallback }
        return Swift.max(lo, Swift.min(hi, v))
    }

    private func selectByRepresented(_ popup: NSPopUpButton, _ value: String) {
        for item in popup.itemArray where (item.representedObject as? String) == value {
            popup.select(item); return
        }
    }
}
