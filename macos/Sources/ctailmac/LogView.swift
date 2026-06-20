import AppKit

/// The high-performance log surface — the part that worried us most in the
/// feasibility review. Backed by NSTableView, which only instantiates row views
/// for visible rows, so memory and CPU stay flat whether the buffer holds 1k or
/// 1M lines. SwiftUI's List/LazyVStack degrade at that scale and give poor
/// scroll control; this is why a native port wants AppKit here.
final class LogView: NSView {
    private let scrollView = NSScrollView()
    private let table = NSTableView()
    private var lines: [LogLine] = []
    private let bufferSize = 200_000        // sliding window cap, like the Go buffer
    private var highlighter: HighlightEngine
    private let theme: Theme
    private let rowFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

    /// Whether new lines auto-scroll into view (tail -f). Auto-disables when the
    /// user scrolls up, re-enables when they return to the bottom.
    private(set) var following = true
    var onFollowingChanged: ((Bool) -> Void)?

    init(theme: Theme, rules: [HighlightRule]) {
        self.theme = theme
        self.highlighter = HighlightEngine(rules: rules, theme: theme,
                                           font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular))
        super.init(frame: .zero)
        setup()
    }
    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        table.headerView = nil
        table.backgroundColor = theme.background
        table.usesAlternatingRowBackgroundColors = false
        table.gridStyleMask = []
        table.rowHeight = ceil(rowFont.ascender - rowFont.descender + rowFont.leading) + 4
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.selectionHighlightStyle = .regular

        let gutter = NSTableColumn(identifier: .init("gutter"))
        gutter.width = 64
        let text = NSTableColumn(identifier: .init("text"))
        text.resizingMask = .autoresizingMask
        table.addTableColumn(gutter)
        table.addTableColumn(text)
        table.dataSource = self
        table.delegate = self

        scrollView.documentView = table
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.drawsBackground = true
        scrollView.backgroundColor = theme.background
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scrollView)
        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])

        // Track scroll position so follow mode mirrors the Svelte behavior.
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(boundsChanged),
                                               name: NSView.boundsDidChangeNotification,
                                               object: scrollView.contentView)
    }

    // MARK: - Data feed (called from the Tailer callbacks, on the main thread)

    func append(_ newLines: [LogLine]) {
        guard !newLines.isEmpty else { return }
        lines.append(contentsOf: newLines)
        if lines.count > bufferSize {
            lines.removeFirst(lines.count - bufferSize)
        }
        table.reloadData()
        if following { scrollToBottom() }
    }

    func reset() {
        lines.removeAll(keepingCapacity: true)
        table.reloadData()
    }

    var lineCount: Int { lines.count }

    func scrollToBottom() {
        guard !lines.isEmpty else { return }
        table.scrollRowToVisible(lines.count - 1)
    }

    @objc private func boundsChanged() {
        let documentHeight = table.bounds.height
        let visible = scrollView.contentView.bounds
        let atBottom = visible.maxY >= documentHeight - table.rowHeight * 1.5
        if atBottom != following {
            following = atBottom
            onFollowingChanged?(following)
        }
    }
}

extension LogView: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int { lines.count }
}

extension LogView: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let line = lines[row]
        let id = tableColumn!.identifier
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? NSTextField) ?? makeCell(id)

        if id.rawValue == "gutter" {
            cell.attributedStringValue = NSAttributedString(
                string: String(line.number),
                attributes: [.font: rowFont, .foregroundColor: theme.gutter]
            )
            cell.alignment = .right
        } else {
            cell.attributedStringValue = highlighter.render(line.text)
            cell.alignment = .left
        }
        return cell
    }

    private func makeCell(_ id: NSUserInterfaceItemIdentifier) -> NSTextField {
        let f = NSTextField(labelWithString: "")
        f.identifier = id
        f.font = rowFont
        f.lineBreakMode = .byClipping
        f.cell?.usesSingleLineMode = true
        f.drawsBackground = false
        f.isBordered = false
        return f
    }
}
