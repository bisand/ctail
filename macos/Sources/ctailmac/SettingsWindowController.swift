import AppKit

/// Settings window mirroring SettingsPanel.svelte (minus the Linux-only display/
/// GPU options). Edits a working copy of AppSettings and hands it back on Save;
/// the caller persists and applies it.
final class SettingsWindowController: NSWindowController {
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

    init(settings: AppSettings, themes: [Theme], onSave: @escaping (AppSettings) -> Void) {
        self.settings = settings
        self.themes = themes
        self.onSave = onSave
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 460, height: 620),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Settings"
        super.init(window: window)
        window.contentView = buildContent()
        window.center()
        load()
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Layout

    private func buildContent() -> NSView {
        let form = NSStackView()
        form.orientation = .vertical
        form.alignment = .leading
        form.spacing = 10
        form.edgeInsets = NSEdgeInsets(top: 18, left: 20, bottom: 18, right: 20)
        form.translatesAutoresizingMaskIntoConstraints = false

        for v in [themePopup, modePopup, newTabPopup, aiProviderPopup] { v.translatesAutoresizingMaskIntoConstraints = false }
        modePopup.addItems(withTitles: ["dark", "light"])
        newTabPopup.addItems(withTitles: ["end", "afterActive"])
        aiProviderPopup.addItems(withTitles: ["", "openai", "github", "copilot", "custom"])
        themes.forEach { themePopup.addItem(withTitle: $0.displayName); themePopup.lastItem?.representedObject = $0.name }

        form.addArrangedSubview(header("Appearance"))
        form.addArrangedSubview(row("Theme", themePopup))
        form.addArrangedSubview(row("Mode", modePopup))
        form.addArrangedSubview(row("Font size", fontField))
        form.addArrangedSubview(lineNumbers)
        form.addArrangedSubview(wordWrap)

        form.addArrangedSubview(header("Behavior"))
        form.addArrangedSubview(row("Poll interval (ms)", pollField))
        form.addArrangedSubview(row("Buffer size (lines)", bufferField))
        form.addArrangedSubview(row("Scrollback (lines)", scrollbackField))
        form.addArrangedSubview(row("Read timeout (s)", timeoutField))
        form.addArrangedSubview(restoreTabs)
        form.addArrangedSubview(row("New tab position", newTabPopup))

        form.addArrangedSubview(header("Updates"))
        form.addArrangedSubview(disableUpdates)
        form.addArrangedSubview(row("Check interval (h)", updateIntervalField))

        form.addArrangedSubview(header("AI Assistant"))
        form.addArrangedSubview(row("Provider", aiProviderPopup))
        form.addArrangedSubview(row("Endpoint", aiEndpointField))
        form.addArrangedSubview(row("API key", aiKeyField))
        form.addArrangedSubview(row("Model", aiModelField))

        let save = NSButton(title: "Save", target: self, action: #selector(saveTapped))
        save.keyEquivalent = "\r"
        let cancel = NSButton(title: "Cancel", target: self, action: #selector(cancelTapped))
        let buttons = NSStackView(views: [cancel, save])
        buttons.spacing = 10
        form.addArrangedSubview(separator())
        form.addArrangedSubview(buttons)

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.documentView = form
        scroll.drawsBackground = false
        NSLayoutConstraint.activate([
            form.widthAnchor.constraint(equalToConstant: 420),
        ])
        return scroll
    }

    private func header(_ t: String) -> NSView {
        let l = NSTextField(labelWithString: t)
        l.font = .boldSystemFont(ofSize: 13)
        return l
    }

    private func separator() -> NSView {
        let v = NSBox(); v.boxType = .separator
        v.translatesAutoresizingMaskIntoConstraints = false
        v.widthAnchor.constraint(equalToConstant: 420).isActive = true
        return v
    }

    private func row(_ label: String, _ control: NSView) -> NSView {
        let l = NSTextField(labelWithString: label)
        l.alignment = .right
        l.translatesAutoresizingMaskIntoConstraints = false
        l.widthAnchor.constraint(equalToConstant: 140).isActive = true
        if let field = control as? NSTextField {
            field.translatesAutoresizingMaskIntoConstraints = false
            field.widthAnchor.constraint(equalToConstant: 240).isActive = true
        }
        let h = NSStackView(views: [l, control])
        h.orientation = .horizontal
        h.spacing = 10
        return h
    }

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
