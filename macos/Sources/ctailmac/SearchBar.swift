import AppKit

/// VS Code-style inline search bar (issue #9): query field, Aa / W / .* toggles,
/// match counter, prev/next, a filter toggle, and close. Reports changes to its
/// owner, which drives the active LogView.
final class SearchBar: NSView {
    var onChange: (() -> Void)?
    var onNext: (() -> Void)?
    var onPrev: (() -> Void)?
    var onClose: (() -> Void)?

    let field = NSTextField()
    private let counter = NSTextField(labelWithString: "")
    private let caseBtn = NSButton()
    private let wordBtn = NSButton()
    private let regexBtn = NSButton()
    private let filterBtn = NSButton()
    private let palette: ThemeColors

    var queryText: String { field.stringValue }
    var caseSensitive: Bool { caseBtn.state == .on }
    var wholeWord: Bool { wordBtn.state == .on }
    var isRegex: Bool { regexBtn.state == .on }
    var filterMode: Bool { filterBtn.state == .on }

    init(palette: ThemeColors) {
        self.palette = palette
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = palette.surface.cgColor
        layer?.cornerRadius = 6
        layer?.borderWidth = 1
        layer?.borderColor = palette.borderColor.cgColor

        field.placeholderString = "Find"
        field.font = .systemFont(ofSize: 12)
        field.focusRingType = .none
        field.bezelStyle = .roundedBezel
        field.target = self
        field.action = #selector(fieldChanged)
        field.delegate = self

        counter.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        counter.textColor = palette.muted
        counter.alignment = .right

        configureToggle(caseBtn, "Aa", "Match Case")
        configureToggle(wordBtn, "W", "Whole Word")
        configureToggle(regexBtn, ".*", "Use Regular Expression")
        configureToggle(filterBtn, "≡", "Filter: show only matching lines")

        let prev = iconButton("chevron.up", #selector(prevTapped), "Previous match")
        let next = iconButton("chevron.down", #selector(nextTapped), "Next match")
        let close = iconButton("xmark", #selector(closeTapped), "Close")

        let stack = NSStackView(views: [field, counter, caseBtn, wordBtn, regexBtn, filterBtn, prev, next, close])
        stack.orientation = .horizontal
        stack.spacing = 4
        stack.alignment = .centerY
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            field.widthAnchor.constraint(equalToConstant: 200),
            counter.widthAnchor.constraint(equalToConstant: 64),
            heightAnchor.constraint(equalToConstant: 36),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    private func configureToggle(_ b: NSButton, _ title: String, _ tip: String) {
        b.title = title
        b.setButtonType(.pushOnPushOff)
        b.bezelStyle = .recessed
        b.font = .monospacedSystemFont(ofSize: 11, weight: .medium)
        b.toolTip = tip
        b.target = self
        b.action = #selector(fieldChanged)
    }

    private func iconButton(_ symbol: String, _ action: Selector, _ tip: String) -> NSButton {
        let b = NSButton()
        b.image = NSImage(systemSymbolName: symbol, accessibilityDescription: tip)
        b.isBordered = false
        b.bezelStyle = .regularSquare
        b.contentTintColor = palette.foreground
        b.toolTip = tip
        b.target = self
        b.action = action
        return b
    }

    func setCounter(total: Int, current: Int, valid: Bool) {
        if !valid { counter.stringValue = "bad regex"; counter.textColor = palette.dangerColor; return }
        counter.textColor = palette.muted
        counter.stringValue = total == 0 ? (queryText.isEmpty ? "" : "No results") : "\(current)/\(total)"
    }

    func focusField() { window?.makeFirstResponder(field) }

    @objc private func fieldChanged() { onChange?() }
    @objc private func nextTapped() { onNext?() }
    @objc private func prevTapped() { onPrev?() }
    @objc private func closeTapped() { onClose?() }
}

extension SearchBar: NSTextFieldDelegate {
    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        switch sel {
        case #selector(NSResponder.insertNewline(_:)):
            if NSApp.currentEvent?.modifierFlags.contains(.shift) == true { onPrev?() } else { onNext?() }
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            onClose?(); return true
        default:
            return false
        }
    }

    func controlTextDidChange(_ obj: Notification) { onChange?() }
}
