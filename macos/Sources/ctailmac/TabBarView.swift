import AppKit

/// The tab strip. Renders one TabButton per open file plus a trailing "+".
/// Reports user intent (select/close/new/reorder/rename/context) to its owner.
final class TabBarView: NSView {
    var onSelect: ((Int) -> Void)?
    var onClose: ((Int) -> Void)?
    var onNew: (() -> Void)?
    var onReorder: ((Int, Int) -> Void)?
    var onRename: ((Int) -> Void)?
    var onContext: ((Int, NSEvent) -> Void)?

    private var palette: ThemeColors
    private let stack = NSStackView()
    private let newButton = NSButton()
    private var buttons: [TabButton] = []

    init(palette: ThemeColors) {
        self.palette = palette
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = palette.backgroundAlt.cgColor

        stack.orientation = .horizontal
        stack.spacing = 1
        stack.alignment = .centerY
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        newButton.title = "+"
        newButton.bezelStyle = .regularSquare
        newButton.isBordered = false
        newButton.font = .systemFont(ofSize: 16)
        newButton.contentTintColor = palette.muted
        newButton.target = self
        newButton.action = #selector(newTapped)
        newButton.translatesAutoresizingMaskIntoConstraints = false
        addSubview(newButton)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            newButton.leadingAnchor.constraint(equalTo: stack.trailingAnchor, constant: 6),
            newButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            newButton.widthAnchor.constraint(equalToConstant: 24),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    func apply(palette: ThemeColors) {
        self.palette = palette
        layer?.backgroundColor = palette.backgroundAlt.cgColor
        newButton.contentTintColor = palette.muted
    }

    /// Rebuilds the buttons from the tab list and marks the active one.
    func reload(titles: [(name: String, color: String)], active: Int) {
        buttons.forEach { $0.removeFromSuperview() }
        buttons = titles.enumerated().map { idx, t in
            let b = TabButton(index: idx, title: t.name, color: t.color,
                              isActive: idx == active, palette: palette)
            b.onSelect = { [weak self] i in self?.onSelect?(i) }
            b.onClose = { [weak self] i in self?.onClose?(i) }
            b.onRename = { [weak self] i in self?.onRename?(i) }
            b.onContext = { [weak self] i, e in self?.onContext?(i, e) }
            b.onDrag = { [weak self] from, to in self?.onReorder?(from, to) }
            b.indexProvider = { [weak self] btn in self?.buttons.firstIndex(of: btn) ?? -1 }
            return b
        }
        buttons.forEach { stack.addArrangedSubview($0) }
    }

    @objc private func newTapped() { onNew?() }
}

/// A single tab: color dot + label + close button. Handles click, double-click
/// (rename), right-click (context), and horizontal drag (reorder).
final class TabButton: NSView {
    var onSelect: ((Int) -> Void)?
    var onClose: ((Int) -> Void)?
    var onRename: ((Int) -> Void)?
    var onContext: ((Int, NSEvent) -> Void)?
    var onDrag: ((Int, Int) -> Void)?
    var indexProvider: ((TabButton) -> Int)?

    let index: Int
    private let palette: ThemeColors
    private let labelField = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private let dot = NSView()
    private var dragStart: NSPoint?

    init(index: Int, title: String, color: String, isActive: Bool, palette: ThemeColors) {
        self.index = index
        self.palette = palette
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = (isActive ? palette.background : palette.backgroundAlt).cgColor
        layer?.cornerRadius = 5
        translatesAutoresizingMaskIntoConstraints = false

        dot.wantsLayer = true
        dot.translatesAutoresizingMaskIntoConstraints = false
        dot.layer?.cornerRadius = 4
        dot.layer?.backgroundColor = color.isEmpty ? NSColor.clear.cgColor : Theme.hex(color).cgColor
        dot.isHidden = color.isEmpty

        labelField.stringValue = title
        labelField.font = .systemFont(ofSize: 12)
        labelField.textColor = isActive ? palette.foreground : palette.muted
        labelField.translatesAutoresizingMaskIntoConstraints = false
        labelField.lineBreakMode = .byTruncatingTail

        closeButton.title = "✕"
        closeButton.isBordered = false
        closeButton.font = .systemFont(ofSize: 9)
        closeButton.contentTintColor = palette.muted
        closeButton.target = self
        closeButton.action = #selector(closeTapped)
        closeButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(dot); addSubview(labelField); addSubview(closeButton)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 24),
            dot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            dot.centerYAnchor.constraint(equalTo: centerYAnchor),
            dot.widthAnchor.constraint(equalToConstant: 8),
            dot.heightAnchor.constraint(equalToConstant: 8),
            labelField.leadingAnchor.constraint(equalTo: dot.trailingAnchor, constant: 5),
            labelField.centerYAnchor.constraint(equalTo: centerYAnchor),
            labelField.widthAnchor.constraint(lessThanOrEqualToConstant: 160),
            closeButton.leadingAnchor.constraint(equalTo: labelField.trailingAnchor, constant: 6),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
            closeButton.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    private var idx: Int { indexProvider?(self) ?? index }

    override func mouseDown(with event: NSEvent) {
        dragStart = event.locationInWindow
        if event.clickCount == 2 { onRename?(idx) }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start = dragStart else { return }
        let dx = event.locationInWindow.x - start.x
        // Cross a half-tab-width -> swap with the neighbor in that direction.
        if abs(dx) > bounds.width / 2 {
            let from = idx
            let to = dx > 0 ? from + 1 : from - 1
            onDrag?(from, to)
            dragStart = event.locationInWindow
        }
    }

    override func mouseUp(with event: NSEvent) {
        if let start = dragStart, abs(event.locationInWindow.x - start.x) < 4 {
            onSelect?(idx)
        }
        dragStart = nil
    }

    override func rightMouseDown(with event: NSEvent) { onContext?(idx, event) }

    @objc private func closeTapped() { onClose?(idx) }
}
