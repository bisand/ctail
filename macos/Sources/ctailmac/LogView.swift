import AppKit

/// The high-performance log surface — the part that worried us most in the
/// feasibility review. Backed by NSTableView, which only instantiates row views
/// for visible rows, so memory and CPU stay flat whether the buffer holds 1k or
/// 1M lines. SwiftUI's List/LazyVStack degrade at that scale and give poor
/// scroll control; this is why a native port wants AppKit here.
///
/// Supports VS Code-style search (issue #9): match highlighting, prev/next
/// navigation, and a filter mode that shows only matching lines.
final class LogView: NSView {
    private let scrollView = NSScrollView()
    private let table = NSTableView()
    private var lines: [LogLine] = []            // full buffer
    private var filtered: [LogLine] = []         // populated only in filter mode
    private let bufferSize = 200_000             // sliding window cap, like the Go buffer
    private var highlighter: HighlightEngine
    private let palette: ThemeColors
    private let rowFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

    // Search state.
    private var query = SearchQuery("", caseSensitive: false, wholeWord: false, isRegex: false)
    private var filterMode = false
    private var matchRows: [Int] = []            // row indices (into `displayed`) that match
    private var currentMatch = -1

    /// Whether new lines auto-scroll into view (tail -f). Auto-disables when the
    /// user scrolls up, re-enables when they return to the bottom.
    private(set) var following = true
    var onFollowingChanged: ((Bool) -> Void)?

    private var displayed: [LogLine] { filterMode ? filtered : lines }

    init(palette: ThemeColors, rules: [HighlightRule]) {
        self.palette = palette
        self.highlighter = HighlightEngine(rules: rules, palette: palette,
                                           font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular))
        super.init(frame: .zero)
        setup()
    }
    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        table.headerView = nil
        table.backgroundColor = palette.background
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
        scrollView.backgroundColor = palette.background
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scrollView)
        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])

        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(boundsChanged),
                                               name: NSView.boundsDidChangeNotification,
                                               object: scrollView.contentView)
    }

    // MARK: - Data feed (called from the Tailer callbacks, on the main thread)

    func append(_ newLines: [LogLine]) {
        guard !newLines.isEmpty else { return }
        lines.append(contentsOf: newLines)
        if lines.count > bufferSize { lines.removeFirst(lines.count - bufferSize) }
        if filterMode || !query.isEmpty {
            // Keep the filter view / match set current as new lines stream in.
            recomputeSearch(preserveCurrent: true)
        } else {
            table.reloadData()
        }
        if following { scrollToBottom() }
    }

    func reset() {
        lines.removeAll(keepingCapacity: true)
        filtered.removeAll(keepingCapacity: true)
        matchRows.removeAll(); currentMatch = -1
        table.reloadData()
    }

    var lineCount: Int { lines.count }

    func scrollToBottom() {
        let n = displayed.count
        guard n > 0 else { return }
        table.scrollRowToVisible(n - 1)
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

    // MARK: - Search (issue #9)

    struct SearchResult { let total: Int; let current: Int }   // current is 1-based, 0 if none

    @discardableResult
    func search(text: String, caseSensitive: Bool, wholeWord: Bool, isRegex: Bool, filter: Bool) -> SearchResult {
        query = SearchQuery(text, caseSensitive: caseSensitive, wholeWord: wholeWord, isRegex: isRegex)
        filterMode = filter && !query.isEmpty
        following = false
        return recomputeSearch(preserveCurrent: false)
    }

    func clearSearch() {
        query = SearchQuery("", caseSensitive: false, wholeWord: false, isRegex: false)
        filterMode = false
        matchRows.removeAll(); currentMatch = -1
        table.reloadData()
    }

    var searchIsValid: Bool { query.isValid }

    @discardableResult
    private func recomputeSearch(preserveCurrent: Bool) -> SearchResult {
        let keepLine = preserveCurrent && currentMatch >= 0 && currentMatch < matchRows.count
            ? displayed[matchRows[currentMatch]].number : nil

        if filterMode {
            filtered = lines.filter { query.matches($0.text) }
            matchRows = Array(0..<filtered.count)
        } else if query.isEmpty {
            matchRows = []
        } else {
            matchRows = displayed.enumerated().compactMap { query.matches($0.element.text) ? $0.offset : nil }
        }
        table.reloadData()

        if let keepLine, let idx = matchRows.firstIndex(where: { displayed[$0].number == keepLine }) {
            currentMatch = idx
        } else {
            currentMatch = matchRows.isEmpty ? -1 : 0
        }
        focusCurrentMatch()
        return SearchResult(total: matchRows.count, current: currentMatch < 0 ? 0 : currentMatch + 1)
    }

    @discardableResult
    func nextMatch() -> SearchResult { step(+1) }
    @discardableResult
    func prevMatch() -> SearchResult { step(-1) }

    private func step(_ dir: Int) -> SearchResult {
        guard !matchRows.isEmpty else { return SearchResult(total: 0, current: 0) }
        currentMatch = (currentMatch + dir + matchRows.count) % matchRows.count
        focusCurrentMatch()
        return SearchResult(total: matchRows.count, current: currentMatch + 1)
    }

    private func focusCurrentMatch() {
        guard currentMatch >= 0, currentMatch < matchRows.count else { return }
        let row = matchRows[currentMatch]
        table.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        table.scrollRowToVisible(row)
        table.reloadData()
    }
}

extension LogView: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int { displayed.count }
}

extension LogView: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let line = displayed[row]
        let id = tableColumn!.identifier
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? NSTextField) ?? makeCell(id)

        if id.rawValue == "gutter" {
            cell.attributedStringValue = NSAttributedString(
                string: String(line.number),
                attributes: [.font: rowFont, .foregroundColor: palette.gutter])
            cell.alignment = .right
        } else {
            let attr = NSMutableAttributedString(attributedString: highlighter.render(line.text))
            applySearchHighlight(attr, line: line, row: row)
            cell.attributedStringValue = attr
            cell.alignment = .left
        }
        return cell
    }

    /// Layers a yellow background on search matches, brighter on the current one.
    private func applySearchHighlight(_ attr: NSMutableAttributedString, line: LogLine, row: Int) {
        guard !query.isEmpty else { return }
        let isCurrent = currentMatch >= 0 && currentMatch < matchRows.count && matchRows[currentMatch] == row
        let bg = isCurrent ? palette.accentColor : palette.warningColor
        for r in query.ranges(in: line.text) where r.location != NSNotFound {
            attr.addAttribute(.backgroundColor, value: bg.withAlphaComponent(isCurrent ? 0.85 : 0.45), range: r)
        }
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
