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
    private let table = LogTableView()
    private var lines: [LogLine] = []            // the in-memory window (≤ windowCap)
    private var filtered: [LogLine] = []         // populated only in filter mode
    /// The window slides over the file; only `windowCap` lines are ever resident,
    /// the rest is paged from disk on demand and evicted from the far end. The
    /// window bounds are DERIVED from the buffer (never tracked separately) so they
    /// can't desync — the source of an earlier splice bug.
    private var windowStart: Int64 { lines.first?.number ?? 1 }   // absolute line of lines.first
    private var windowEnd: Int64 { lines.last?.number ?? 0 }      // absolute line of lines.last
    private let windowCap: Int                   // configurable: settings.bufferSize
    private let pageChunk: Int                   // configurable: settings.scrollBuffer
    private var isPaging = false                 // serializes disk page-in requests
    private var suppressScrollHandling = false   // ignore programmatic scroll adjustments
    private var highlighter: HighlightEngine
    private let palette: ThemeColors
    private let rowFont: NSFont

    /// Pulls an absolute line range [start, start+count) from disk (the Tailer),
    /// delivering the lines on the main queue. Drives the sliding window.
    var requestRange: ((_ start: Int64, _ count: Int, _ completion: @escaping ([LogLine]) -> Void) -> Void)?
    /// Total lines currently known in the file (grows as the file is tailed).
    var totalLinesProvider: (() -> Int64)?
    /// Whether the background offset index is ready (scrollback needs it).
    var indexingReadyProvider: (() -> Bool)?

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

    init(palette: ThemeColors, rules: [HighlightRule], fontSize: CGFloat = 12,
         showLineNumbers: Bool = true, bufferSize: Int = 10_000, scrollBuffer: Int = 500) {
        self.palette = palette
        self.rowFont = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        self.showLineNumbers = showLineNumbers
        self.windowCap = max(200, bufferSize)
        // Page in at most half the window per scroll so it always slides rather
        // than wholly replacing; keep it positive even if scrollBuffer is 0.
        self.pageChunk = max(50, min(scrollBuffer <= 0 ? 500 : scrollBuffer, max(200, bufferSize) / 2))
        self.highlighter = HighlightEngine(rules: rules, palette: palette, font: rowFont)
        super.init(frame: .zero)
        setup()
    }

    private let showLineNumbers: Bool
    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        table.headerView = nil
        table.backgroundColor = palette.background
        table.usesAlternatingRowBackgroundColors = false
        table.gridStyleMask = []
        table.rowHeight = ceil(rowFont.ascender - rowFont.descender + rowFont.leading) + 4
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.selectionHighlightStyle = .regular
        table.allowsMultipleSelection = true     // shift/⌘-click + click-drag across lines
        table.allowsEmptySelection = true

        let gutter = NSTableColumn(identifier: .init("gutter"))
        gutter.width = showLineNumbers ? 64 : 0
        gutter.isHidden = !showLineNumbers
        let text = NSTableColumn(identifier: .init("text"))
        text.resizingMask = .autoresizingMask
        table.addTableColumn(gutter)
        table.addTableColumn(text)
        table.dataSource = self
        table.delegate = self
        table.keyHandler = self

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

    /// Take key focus when shown so Home/End/Page keys reach the table without a
    /// click first, and re-sync the table — a background tab's appends went through
    /// reloadData while off-screen, so reload to display the current buffer.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else { return }
        window?.makeFirstResponder(table)
        table.reloadData()
        if following { scrollToBottom() }
    }

    // MARK: - Data feed (called from the Tailer callbacks, on the main thread)

    func append(_ newLines: [LogLine]) {
        guard !newLines.isEmpty else { return }
        // Only mutate the window while following the tail. If the user has scrolled
        // up, new lines stay on disk (reachable by paging back down) and their view
        // is left undisturbed.
        guard following else { return }
        // Only extend a buffer that's actually at the live tail: the new lines must
        // be contiguous with what we hold. This guards against a stale `following`
        // splicing tail lines onto a scrolled-up (head) window.
        if let last = lines.last, newLines.first!.number != last.number + 1 { return }

        if filterMode || !query.isEmpty {
            lines.append(contentsOf: newLines)
            if lines.count > windowCap { lines.removeFirst(lines.count - windowCap) }
            recomputeSearch(preserveCurrent: true)   // search owns the row selection
            scrollToBottom()
            return
        }

        let firstNew = lines.count
        lines.append(contentsOf: newLines)

        // Frozen while the user has a selection (or is mid-drag): keep the selection
        // and the visible content perfectly put by ONLY appending rows at the bottom
        // — no eviction, no scroll, no reloadData (which would shift the selection).
        // Safe to use insertRows only when the table is displayed and its row count
        // matches the buffer; appending at `firstNew == numberOfRows` can't go out
        // of range. A growth cap (then a reload) bounds a long-held selection.
        if (table.isDragging || hasSelection), table.window != nil, table.numberOfRows == firstNew {
            let hardCap = windowCap * 3
            if lines.count <= hardCap {
                table.insertRows(at: IndexSet(integersIn: firstNew..<lines.count), withAnimation: [])
            } else {
                lines.removeFirst(lines.count - hardCap)
                table.reloadData()
            }
            return
        }

        // Default: keep the window bounded and reloadData. There's no selection to
        // preserve here, and reloadData is always safe (off-screen background tabs,
        // initial big tail loads, evictions of any size) — no row-delta math.
        if lines.count > windowCap { lines.removeFirst(lines.count - windowCap) }
        table.reloadData()
        if following { scrollToBottom() }
    }

    func reset() {
        lines.removeAll(keepingCapacity: true)
        filtered.removeAll(keepingCapacity: true)
        matchRows.removeAll(); currentMatch = -1
        following = true
        table.reloadData()
    }

    /// The background line count finished: the tail was shown numbered locally
    /// (1, 2, …); shift every resident line by `base` (lines before the tail) to
    /// make the numbers absolute, and drop the placeholder gutter. Cheap and
    /// in-memory — no disk reload.
    func applyLineNumberBase(_ base: Int64) {
        let wasFollowing = following
        if base > 0 {
            lines = lines.map { LogLine(number: $0.number + base, text: $0.text) }
            filtered = filtered.map { LogLine(number: $0.number + base, text: $0.text) }
        }
        table.reloadData()      // gutter now renders real numbers (indexingReady == true)
        // reloadData can reset the scroll position; if we were following the tail,
        // stay pinned to the bottom so `following` keeps matching the viewport
        // (otherwise the window desyncs and later paging splices the wrong range).
        if wasFollowing { scrollToBottom() }
    }

    var lineCount: Int { lines.count }

    /// The last `n` lines as text, for AI context.
    func tailText(_ n: Int = 500) -> String {
        lines.suffix(n).map { $0.text }.joined(separator: "\n")
    }

    func selectAllRows() {
        guard !displayed.isEmpty else { return }
        table.selectRowIndexes(IndexSet(integersIn: 0..<displayed.count), byExtendingSelection: false)
    }

    /// Whether any line is selected.
    var hasSelection: Bool { !table.selectedRowIndexes.isEmpty }

    /// Clears the selection (Esc) and resumes normal operation: trim any overflow
    /// the frozen window accumulated, and if following, snap back to the tail.
    func clearSelection() {
        guard hasSelection else { return }
        table.deselectAll(nil)
        if lines.count > windowCap {
            lines.removeFirst(lines.count - windowCap)
            table.reloadData()
        }
        if following { scrollToBottom() }
    }

    /// Text of the selected rows (or all resident rows if none selected),
    /// newline-joined. Used by Copy.
    func selectedText() -> String {
        let rows = table.selectedRowIndexes
        let source = rows.isEmpty ? Array(0..<displayed.count) : Array(rows)
        return source.compactMap { displayed.indices.contains($0) ? displayed[$0].text : nil }
            .joined(separator: "\n")
    }

    /// Text of the currently selected lines, or nil when nothing is selected.
    /// Used to feed a selection to the AI assistant.
    func selectionText() -> String? {
        let rows = table.selectedRowIndexes
        guard !rows.isEmpty else { return nil }
        return rows.compactMap { displayed.indices.contains($0) ? displayed[$0].text : nil }
            .joined(separator: "\n")
    }

    func scrollToBottom() {
        let n = displayed.count
        guard n > 0 else { return }
        table.scrollRowToVisible(n - 1)
    }

    /// Public toggle for the status-bar Follow checkbox: enabling jumps to the
    /// live tail and resumes auto-scroll; disabling just stops following.
    func setFollow(_ on: Bool) {
        if on { jumpToEnd() } else { setFollowing(false) }
    }

    // MARK: - Keyboard navigation (Home / End / Page Up / Page Down)

    /// Home: jump to the very start of the file, loading the first window from disk.
    func jumpToStart() {
        guard !filterMode else { scrollRowToTop(0); return }
        let total = totalLinesProvider?() ?? Int64(lines.count)
        guard let requestRange, (indexingReadyProvider?() ?? false), total > 0 else {
            scrollRowToTop(0); return
        }
        let count = min(windowCap, Int(total))
        isPaging = true
        requestRange(1, count) { [weak self] head in
            guard let self else { return }
            defer { self.isPaging = false }
            guard !head.isEmpty else { return }
            self.setFollowing(false)
            self.lines = head
            self.table.reloadData()
            self.scrollRowToTop(0)
        }
    }

    /// End: jump to the tail and resume following, loading the last window from disk.
    func jumpToEnd() {
        let total = totalLinesProvider?() ?? Int64(lines.count)
        guard !filterMode, let requestRange, (indexingReadyProvider?() ?? false), total > 0 else {
            setFollowing(true); scrollToBottom(); return
        }
        let count = min(windowCap, Int(total))
        let start = total - Int64(count) + 1
        isPaging = true
        requestRange(start, count) { [weak self] tail in
            guard let self else { return }
            defer { self.isPaging = false }
            guard !tail.isEmpty else { return }
            self.lines = tail
            self.setFollowing(true)
            self.table.reloadData()
            self.scrollToBottom()
        }
    }

    func pageUpByScreen()   { goTo(topLine: currentTopLine() - Int64(viewportRows())) }
    func pageDownByScreen() {
        let total = totalLinesProvider?() ?? Int64(lines.count)
        let target = currentTopLine() + Int64(viewportRows())
        // Landing at or past EOF means we're back at the tail — follow.
        if target + Int64(viewportRows()) - 1 >= total { jumpToEnd() } else { goTo(topLine: target) }
    }

    private func viewportRows() -> Int {
        max(1, Int(scrollView.contentView.bounds.height / table.rowHeight))
    }

    /// Absolute file line currently at the top of the viewport.
    private func currentTopLine() -> Int64 {
        let topRow = max(0, Int(scrollView.contentView.bounds.minY / table.rowHeight))
        return windowStart + Int64(min(topRow, max(0, lines.count - 1)))
    }

    /// Scrolls so `topLine` sits at the top of the viewport, loading a fresh window
    /// from disk when the target lies outside (or too near the edge of) the one
    /// currently resident. Disabled in filter mode (absolute lines don't map).
    private func goTo(topLine: Int64) {
        guard !filterMode else { return }
        let total = totalLinesProvider?() ?? Int64(lines.count)
        let clampedTop = min(max(1, topLine), max(1, total))
        let rows = Int64(viewportRows())
        let haveAbove = !lines.isEmpty && clampedTop >= windowStart
        let haveBelow = windowEnd >= min(total, clampedTop + rows - 1)

        if haveAbove && haveBelow {                      // already resident — instant scroll
            setFollowing(false)
            scrollRowToTop(Int(clampedTop - windowStart))
            return
        }
        guard let requestRange, (indexingReadyProvider?() ?? false) else {
            scrollRowToTop(Int(max(0, clampedTop - windowStart)))
            return
        }
        let start = max(1, min(clampedTop, max(1, total - Int64(windowCap) + 1)))
        let count = min(windowCap, Int(total - start + 1))
        isPaging = true
        requestRange(start, count) { [weak self] win in
            guard let self else { return }
            defer { self.isPaging = false }
            guard !win.isEmpty else { return }
            self.setFollowing(false)
            self.lines = win
            self.table.reloadData()
            self.scrollRowToTop(Int(clampedTop - self.windowStart))
        }
    }

    private func setFollowing(_ value: Bool) {
        guard following != value else { return }
        following = value
        onFollowingChanged?(value)
    }

    /// Runs a programmatic scroll without the bounds observer triggering paging.
    private func suppressed(_ body: () -> Void) {
        suppressScrollHandling = true
        body()
        suppressScrollHandling = false
    }

    /// Scrolls so `row` sits at the top of the viewport (clamped to content), with
    /// the bounds observer suppressed so paging isn't re-triggered.
    private func scrollRowToTop(_ row: Int) {
        let maxY = max(0, table.bounds.height - scrollView.contentView.bounds.height)
        let y = min(maxY, max(0, CGFloat(row) * table.rowHeight))
        suppressed {
            scrollView.contentView.setBoundsOrigin(NSPoint(x: scrollView.contentView.bounds.origin.x, y: y))
            scrollView.reflectScrolledClipView(scrollView.contentView)
        }
    }

    @objc private func boundsChanged() {
        guard !suppressScrollHandling else { return }
        handleScroll()
    }

    /// Decides, on every scroll, whether to (a) page older lines in at the top,
    /// (b) page newer lines in at the bottom, or (c) toggle tail-following — all
    /// while keeping memory bounded to `windowCap`.
    private func handleScroll() {
        let visible = scrollView.contentView.bounds
        let documentHeight = table.bounds.height
        let rowH = table.rowHeight
        let viewportRows = max(1, Int(visible.height / rowH))
        let total = totalLinesProvider?() ?? Int64(lines.count)
        let atVisualBottom = visible.maxY >= documentHeight - rowH * 1.5
        let atVisualTop = visible.minY <= rowH * Double(viewportRows)

        // Prefetch older lines when nearing the top (if any remain on disk).
        if atVisualTop, windowStart > 1, pagingAllowed { pageUp(); return }
        // Prefetch newer lines when nearing the bottom and the window isn't at EOF.
        if atVisualBottom, windowEnd < total, pagingAllowed { pageDown(); return }

        // Follow only when the window is at the tail and we're scrolled to bottom.
        let shouldFollow = atVisualBottom && windowEnd >= total
        if shouldFollow != following {
            following = shouldFollow
            onFollowingChanged?(following)
        }
    }

    private var pagingAllowed: Bool {
        !isPaging && !filterMode && (indexingReadyProvider?() ?? false) && requestRange != nil
    }

    private func pageUp() {
        guard windowStart > 1, let requestRange else { return }
        isPaging = true
        let newStart = max(1, windowStart - Int64(pageChunk))
        let count = Int(windowStart - newStart)
        guard count > 0 else { isPaging = false; return }
        requestRange(newStart, count) { [weak self] older in
            guard let self else { return }
            defer { self.isPaging = false }
            guard !older.isEmpty, older.last?.number == self.windowStart - 1 else { return }   // must be contiguous
            self.following = false
            self.lines.insert(contentsOf: older, at: 0)
            if self.lines.count > self.windowCap {
                self.lines.removeLast(self.lines.count - self.windowCap)   // evict the far (bottom) end
            }
            self.reloadKeepingPosition(rowsDeltaAboveViewport: older.count)
        }
    }

    private func pageDown() {
        let total = totalLinesProvider?() ?? Int64(lines.count)
        guard windowEnd < total, let requestRange else { return }
        isPaging = true
        let fetchStart = windowEnd + 1
        let count = Int(min(Int64(pageChunk), total - windowEnd))
        guard count > 0 else { isPaging = false; return }
        requestRange(fetchStart, count) { [weak self] newer in
            guard let self else { return }
            defer { self.isPaging = false }
            guard !newer.isEmpty, newer.first?.number == self.windowEnd + 1 else { return }   // must be contiguous
            self.lines.append(contentsOf: newer)
            var evicted = 0
            if self.lines.count > self.windowCap {
                evicted = self.lines.count - self.windowCap
                self.lines.removeFirst(evicted)                            // evict the far (top) end
            }
            self.reloadKeepingPosition(rowsDeltaAboveViewport: -evicted)
        }
    }

    /// Reloads the table while keeping the same lines under the viewport. Content
    /// above the viewport changed by `delta` rows (positive = grew via prepend,
    /// negative = shrank via top eviction), so shift the scroll origin to match.
    private func reloadKeepingPosition(rowsDeltaAboveViewport delta: Int) {
        suppressScrollHandling = true
        var origin = scrollView.contentView.bounds.origin
        table.reloadData()
        origin.y = max(0, origin.y + CGFloat(delta) * table.rowHeight)
        scrollView.contentView.setBoundsOrigin(origin)
        scrollView.reflectScrolledClipView(scrollView.contentView)
        suppressScrollHandling = false
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
            // While the background line count runs, real numbers aren't known yet
            // — show a placeholder rather than the provisional local numbers.
            let counting = !(indexingReadyProvider?() ?? true)
            cell.attributedStringValue = NSAttributedString(
                string: counting ? "·" : String(line.number),
                attributes: [.font: rowFont, .foregroundColor: palette.gutter])
            cell.alignment = .right
        } else {
            let rendered = highlighter.render(line.text)
            // Only pay for a mutable copy when there's a search overlay to layer on;
            // the common (no-search) path uses the highlighter's result directly.
            if query.isEmpty {
                cell.attributedStringValue = rendered
            } else {
                let attr = NSMutableAttributedString(attributedString: rendered)
                applySearchHighlight(attr, line: line, row: row)
                cell.attributedStringValue = attr
            }
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

extension LogView: LogScrollKeyHandler {
    func keyJumpToStart() { jumpToStart() }
    func keyJumpToEnd()   { jumpToEnd() }
    func keyPageUp()      { pageUpByScreen() }
    func keyPageDown()    { pageDownByScreen() }
    func keyClearSelection() { clearSelection() }
}

/// Receives the document-navigation keys the table intercepts.
protocol LogScrollKeyHandler: AnyObject {
    func keyJumpToStart()
    func keyJumpToEnd()
    func keyPageUp()
    func keyPageDown()
    func keyClearSelection()
}

/// NSTableView subclass that routes Home / End / Page Up / Page Down to the log
/// view's disk-backed window navigation instead of the default (which only moves
/// within the rows currently loaded). Other keys fall through to normal handling.
final class LogTableView: NSTableView {
    weak var keyHandler: LogScrollKeyHandler?

    /// True while the user is mouse-dragging (NSTableView runs a nested event loop
    /// inside mouseDown). Live appends use this to avoid disturbing the in-progress
    /// selection.
    private(set) var isDragging = false
    override func mouseDown(with event: NSEvent) {
        isDragging = true
        super.mouseDown(with: event)   // blocks until mouse-up while drag-selecting
        isDragging = false
    }

    override func keyDown(with event: NSEvent) {
        switch Int(event.keyCode) {
        case 115: keyHandler?.keyJumpToStart()   // Home
        case 119: keyHandler?.keyJumpToEnd()     // End
        case 116: keyHandler?.keyPageUp()        // Page Up
        case 121: keyHandler?.keyPageDown()      // Page Down
        case 53:  keyHandler?.keyClearSelection() // Esc
        default:  super.keyDown(with: event)
        }
    }
}
