import AppKit

/// Profiles & rules editor (issue #7), mirroring the rules UI in
/// SettingsPanel.svelte. Left: profile chooser + CRUD. Right: the selected
/// profile's rules with a per-rule editor (pattern with live regex validation,
/// fg/bg color wells, bold/italic, line/match type, enabled), reordering, and a
/// live preview line.
final class ProfilesWindowController: NSWindowController {
    private let config: ConfigStore
    private let palette: ThemeColors
    private let onActiveProfileChanged: (String) -> Void

    private var profileNames: [String] = []
    private var profile = Profile(name: "", rules: [])
    private var selectedRule = -1

    private let profilePopup = NSPopUpButton()
    private let rulesTable = NSTableView()
    private let nameField = NSTextField()
    private let patternField = NSTextField()
    private let patternError = NSTextField(labelWithString: "")
    private let matchPopup = NSPopUpButton()
    private let fgWell = NSColorWell()
    private let bgWell = NSColorWell()
    private let boldCheck = NSButton(checkboxWithTitle: "Bold", target: nil, action: nil)
    private let italicCheck = NSButton(checkboxWithTitle: "Italic", target: nil, action: nil)
    private let enabledCheck = NSButton(checkboxWithTitle: "Enabled", target: nil, action: nil)
    private let preview = NSTextField(labelWithString: "")

    init(config: ConfigStore, palette: ThemeColors, onActiveProfileChanged: @escaping (String) -> Void) {
        self.config = config
        self.palette = palette
        self.onActiveProfileChanged = onActiveProfileChanged
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 720, height: 520),
                              styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.title = "Profiles & Rules"
        super.init(window: window)
        window.contentView = buildContent()
        window.center()
        reloadProfiles()
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Layout

    private func buildContent() -> NSView {
        // Top bar: profile chooser + CRUD.
        profilePopup.target = self; profilePopup.action = #selector(profileChanged)
        let newBtn = button("New", #selector(newProfile))
        let renameBtn = button("Rename", #selector(renameProfile))
        let deleteBtn = button("Delete", #selector(deleteProfile))
        let activeBtn = button("Set Active", #selector(setActive))
        let top = NSStackView(views: [label("Profile:"), profilePopup, newBtn, renameBtn, deleteBtn, activeBtn])
        top.spacing = 8

        // Left: rules list + add/remove/reorder.
        rulesTable.headerView = nil
        rulesTable.addTableColumn(NSTableColumn(identifier: .init("rule")))
        rulesTable.dataSource = self
        rulesTable.delegate = self
        let rulesScroll = NSScrollView()
        rulesScroll.documentView = rulesTable
        rulesScroll.hasVerticalScroller = true
        rulesScroll.borderType = .bezelBorder
        rulesScroll.translatesAutoresizingMaskIntoConstraints = false
        rulesScroll.widthAnchor.constraint(equalToConstant: 220).isActive = true

        let ruleButtons = NSStackView(views: [
            button("+", #selector(addRule)), button("−", #selector(removeRule)),
            button("↑", #selector(ruleMoveUp)), button("↓", #selector(ruleMoveDown)),
        ])
        ruleButtons.spacing = 4
        let left = NSStackView(views: [rulesScroll, ruleButtons])
        left.orientation = .vertical
        left.spacing = 6
        left.alignment = .leading

        // Right: per-rule editor.
        matchPopup.addItems(withTitles: ["match", "line"])
        [nameField, patternField].forEach { $0.target = self; $0.action = #selector(ruleEdited) }
        nameField.delegate = self; patternField.delegate = self
        matchPopup.target = self; matchPopup.action = #selector(ruleEdited)
        fgWell.target = self; fgWell.action = #selector(ruleEdited)
        bgWell.target = self; bgWell.action = #selector(ruleEdited)
        [boldCheck, italicCheck, enabledCheck].forEach { $0.target = self; $0.action = #selector(ruleEdited) }
        patternError.textColor = palette.dangerColor
        patternError.font = .systemFont(ofSize: 10)
        preview.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        preview.drawsBackground = true
        preview.backgroundColor = palette.background
        preview.translatesAutoresizingMaskIntoConstraints = false
        preview.heightAnchor.constraint(equalToConstant: 24).isActive = true

        let editor = NSStackView(views: [
            editorRow("Name", nameField),
            editorRow("Pattern", patternField),
            patternError,
            editorRow("Type", matchPopup),
            editorRow("Foreground", fgWell),
            editorRow("Background", bgWell),
            NSStackView(views: [boldCheck, italicCheck, enabledCheck]),
            label("Preview:"),
            preview,
        ])
        editor.orientation = .vertical
        editor.alignment = .leading
        editor.spacing = 8

        let split = NSStackView(views: [left, editor])
        split.orientation = .horizontal
        split.alignment = .top
        split.spacing = 16

        let save = NSButton(title: "Save Profile", target: self, action: #selector(saveProfile))
        save.keyEquivalent = "\r"
        let close = NSButton(title: "Close", target: self, action: #selector(closeTapped))
        let bottom = NSStackView(views: [close, save])
        bottom.spacing = 10

        let root = NSStackView(views: [top, split, bottom])
        root.orientation = .vertical
        root.alignment = .leading
        root.spacing = 14
        root.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        root.translatesAutoresizingMaskIntoConstraints = false

        let container = NSView()
        container.addSubview(root)
        NSLayoutConstraint.activate([
            root.topAnchor.constraint(equalTo: container.topAnchor),
            root.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            root.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            editor.widthAnchor.constraint(equalToConstant: 420),
        ])
        return container
    }

    private func button(_ t: String, _ a: Selector) -> NSButton {
        NSButton(title: t, target: self, action: a)
    }
    private func label(_ t: String) -> NSTextField { NSTextField(labelWithString: t) }
    private func editorRow(_ t: String, _ c: NSView) -> NSView {
        let l = label(t); l.alignment = .right
        l.translatesAutoresizingMaskIntoConstraints = false
        l.widthAnchor.constraint(equalToConstant: 90).isActive = true
        if let f = c as? NSTextField {
            f.translatesAutoresizingMaskIntoConstraints = false
            f.widthAnchor.constraint(equalToConstant: 300).isActive = true
        }
        let h = NSStackView(views: [l, c]); h.spacing = 10
        return h
    }

    // MARK: - Profile CRUD

    private func reloadProfiles(select: String? = nil) {
        config.ensureDefaultProfile()
        profileNames = config.listProfiles()
        profilePopup.removeAllItems()
        profilePopup.addItems(withTitles: profileNames)
        let target = select ?? profileNames.first
        if let target { profilePopup.selectItem(withTitle: target); loadProfile(target) }
    }

    private func loadProfile(_ name: String) {
        profile = config.loadProfile(name) ?? Profile(name: name, rules: [])
        selectedRule = profile.rules.isEmpty ? -1 : 0
        rulesTable.reloadData()
        if selectedRule >= 0 { rulesTable.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false) }
        loadRuleIntoEditor()
    }

    @objc private func profileChanged() {
        if let name = profilePopup.titleOfSelectedItem { loadProfile(name) }
    }

    @objc private func newProfile() {
        guard let name = prompt("New Profile", "Profile name:", "") , !name.isEmpty else { return }
        config.saveProfile(Profile(name: name, rules: []))
        reloadProfiles(select: name)
    }

    @objc private func renameProfile() {
        guard let old = profilePopup.titleOfSelectedItem,
              let new = prompt("Rename Profile", "New name:", old), !new.isEmpty, new != old else { return }
        config.renameProfile(old, to: new)
        reloadProfiles(select: new)
    }

    @objc private func deleteProfile() {
        guard let name = profilePopup.titleOfSelectedItem, profileNames.count > 1 else { return }
        config.deleteProfile(name)
        reloadProfiles()
    }

    @objc private func setActive() {
        if let name = profilePopup.titleOfSelectedItem { onActiveProfileChanged(name) }
    }

    @objc private func saveProfile() {
        config.saveProfile(profile)
        reloadProfiles(select: profile.name)
    }

    @objc private func closeTapped() { close() }

    // MARK: - Rule editing

    @objc private func addRule() {
        let id = "rule-\(profile.rules.count + 1)"
        profile.rules.append(Rule(id: id, name: "New Rule", pattern: "", matchType: "match",
                                  foreground: "#ffd93d", enabled: true,
                                  priority: (profile.rules.map { $0.priority }.max() ?? 0) + 10))
        selectedRule = profile.rules.count - 1
        rulesTable.reloadData()
        rulesTable.selectRowIndexes(IndexSet(integer: selectedRule), byExtendingSelection: false)
        loadRuleIntoEditor()
    }

    @objc private func removeRule() {
        guard profile.rules.indices.contains(selectedRule) else { return }
        profile.rules.remove(at: selectedRule)
        selectedRule = min(selectedRule, profile.rules.count - 1)
        rulesTable.reloadData()
        loadRuleIntoEditor()
    }

    @objc private func ruleMoveUp() { swapRule(selectedRule, selectedRule - 1) }
    @objc private func ruleMoveDown() { swapRule(selectedRule, selectedRule + 1) }

    private func swapRule(_ a: Int, _ b: Int) {
        guard profile.rules.indices.contains(a), profile.rules.indices.contains(b) else { return }
        profile.rules.swapAt(a, b)
        selectedRule = b
        rulesTable.reloadData()
        rulesTable.selectRowIndexes(IndexSet(integer: b), byExtendingSelection: false)
    }

    private func loadRuleIntoEditor() {
        let enabled = profile.rules.indices.contains(selectedRule)
        [nameField, patternField, matchPopup, fgWell, bgWell, boldCheck, italicCheck, enabledCheck]
            .forEach { $0.isEnabled = enabled }
        guard enabled else { nameField.stringValue = ""; patternField.stringValue = ""; preview.stringValue = ""; return }
        let r = profile.rules[selectedRule]
        nameField.stringValue = r.name
        patternField.stringValue = r.pattern
        matchPopup.selectItem(withTitle: r.matchType == "line" ? "line" : "match")
        fgWell.color = r.foreground.isEmpty ? .clear : Theme.hex(r.foreground)
        bgWell.color = r.background.isEmpty ? .clear : Theme.hex(r.background)
        boldCheck.state = r.bold ? .on : .off
        italicCheck.state = r.italic ? .on : .off
        enabledCheck.state = r.enabled ? .on : .off
        updatePreview()
    }

    @objc private func ruleEdited() {
        guard profile.rules.indices.contains(selectedRule) else { return }
        var r = profile.rules[selectedRule]
        r.name = nameField.stringValue
        r.pattern = patternField.stringValue
        r.matchType = matchPopup.titleOfSelectedItem ?? "match"
        r.foreground = Theme.hexString(fgWell.color)
        r.background = Theme.hexString(bgWell.color)
        r.bold = boldCheck.state == .on
        r.italic = italicCheck.state == .on
        r.enabled = enabledCheck.state == .on
        profile.rules[selectedRule] = r
        rulesTable.reloadData(forRowIndexes: IndexSet(integer: selectedRule), columnIndexes: IndexSet(integer: 0))
        validateAndPreview()
    }

    private func validateAndPreview() {
        let pattern = patternField.stringValue
        if pattern.isEmpty || (try? NSRegularExpression(pattern: pattern)) != nil {
            patternError.stringValue = ""
        } else {
            patternError.stringValue = "Invalid regular expression"
        }
        updatePreview()
    }

    private func updatePreview() {
        guard profile.rules.indices.contains(selectedRule) else { preview.stringValue = ""; return }
        let sample = "2026-06-21 12:00:00 INFO ERROR WARN DEBUG sample log line 42"
        let rules = HighlightRule.compile(Profile(name: "", rules: [profile.rules[selectedRule]]))
        let engine = HighlightEngine(rules: rules, palette: palette,
                                     font: .monospacedSystemFont(ofSize: 12, weight: .regular))
        preview.attributedStringValue = engine.render(sample)
    }

    private func prompt(_ title: String, _ message: String, _ initial: String) -> String? {
        let alert = NSAlert()
        alert.messageText = title; alert.informativeText = message
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        field.stringValue = initial
        alert.accessoryView = field
        alert.addButton(withTitle: "OK"); alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn ? field.stringValue : nil
    }
}

extension ProfilesWindowController: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { profile.rules.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let r = profile.rules[row]
        let id = NSUserInterfaceItemIdentifier("rulecell")
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? NSTextField)
            ?? { let f = NSTextField(labelWithString: ""); f.identifier = id; return f }()
        let mark = r.enabled ? "" : "⊘ "
        cell.stringValue = "\(mark)\(r.name.isEmpty ? "(unnamed)" : r.name)"
        cell.textColor = r.foreground.isEmpty ? palette.foreground : Theme.hex(r.foreground)
        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = rulesTable.selectedRow
        if row >= 0 { selectedRule = row; loadRuleIntoEditor() }
    }
}

extension ProfilesWindowController: NSTextFieldDelegate {
    func controlTextDidChange(_ obj: Notification) { ruleEdited() }
}
