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

    init(palette: ThemeColors, rules: [Rule], fontSize: CGFloat = 12,
         showLineNumbers: Bool = true, wordWrap: Bool = false,
         bufferSize: Int = 10_000, scrollBuffer: Int = 500) {
        self.palette = palette
        self.rowFont = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        self.showLineNumbers = showLineNumbers
        self.wordWrap = wordWrap
        self.lineHeight = ceil(NSLayoutManager().defaultLineHeight(for: rowFont))
        self.charAdvance = ("0" as NSString).size(withAttributes: [.font: rowFont]).width
        self.windowCap = max(200, bufferSize)
        // Page in at most half the window per scroll so it always slides rather
        // than wholly replacing; keep it positive even if scrollBuffer is 0.
        self.pageChunk = max(50, min(scrollBuffer <= 0 ? 500 : scrollBuffer, max(200, bufferSize) / 2))
        self.highlighter = HighlightEngine(rules: rules, palette: palette, font: rowFont)
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    // MARK: - View options (line numbers / word wrap)

    private var showLineNumbers: Bool
    private var wordWrap: Bool
    /// Height of one text line in `rowFont`, as the text system lays it out.
    private let lineHeight: CGFloat
    /// Advance of one glyph in the (monospaced) row font — the basis of the O(1)
    /// wrapped-row estimate in `heightOfRow`.
    private let charAdvance: CGFloat
    private let rowPadding: CGFloat = 4
    private let gutterColumn = NSTableColumn(identifier: .init("gutter"))
    private let textColumn = NSTableColumn(identifier: .init("text"))
    /// Text-column width the current wrapped row heights were computed for.
    private var wrapWidth: CGFloat = 0
    private var reloading = false

    /// Shows/hides the line-number gutter live (View menu), keeping the same
    /// content under the viewport.
    func setShowLineNumbers(_ on: Bool) {
        guard on != showLineNumbers else { return }
        showLineNumbers = on
        gutterColumn.isHidden = !on
        if !on { gutterColumn.width = 0 }
        reloadRestoring(scrollAnchor())
    }

    /// Toggles wrapping live. Wrapped rows have variable heights (see
    /// `heightOfRow`), so the table is reloaded and the top line re-anchored.
    func setWordWrap(_ on: Bool) {
        guard on != wordWrap else { return }
        wordWrap = on
        scrollView.hasHorizontalScroller = !on
        wrapWidth = textColumn.width
        reloadRestoring(scrollAnchor())
    }

    /// Gutter wide enough for the largest line number we can show (a 10M-line
    /// file needs 8 digits), never narrower than 4 digits so it doesn't jitter.
    private func gutterWidth() -> CGFloat {
        let maxLine = max(windowEnd, totalLinesProvider?() ?? 0, 1)
        let digits = max(4, String(maxLine).count)
        return ceil(CGFloat(digits) * charAdvance) + 16
    }

    /// Every table reload goes through here so the gutter can grow with the line
    /// count before rows are laid out. `reloading` keeps the column-resize
    /// observer from re-measuring heights mid-reload (reloadData does that itself).
    private func reload() {
        reloading = true
        if showLineNumbers {
            let w = gutterWidth()
            if gutterColumn.width != w { gutterColumn.width = w }
        }
        table.reloadData()
        reloading = false
    }

    /// Rows a line occupies when wrapped, from its column count and the cell
    /// width. The font is monospaced and wrapping is per character, so this is
    /// exact for ASCII; non-ASCII scalars count double (CJK/emoji) and tabs as 4,
    /// which errs toward a spare blank line rather than clipping. O(n) over the
    /// bytes and no text layout, so it's cheap enough to run on every reload.
    private func wrappedRows(for text: String) -> Int {
        let usable = textColumn.width - 8            // NSTextFieldCell's horizontal insets
        guard usable > charAdvance else { return 1 }
        let perRow = max(1, Int(usable / charAdvance))
        var cols = 0
        for b in text.utf8 {
            if b < 0x80 { cols += (b == 0x09) ? 4 : 1 } else if b & 0xC0 != 0x80 { cols += 2 }
        }
        return max(1, (cols + perRow - 1) / perRow)
    }

    /// The text column autoresizes with the window; wrapped heights depend on
    /// its width, so re-measure every row when it changes.
    @objc private func columnResized() {
        guard wordWrap, textColumn.width != wrapWidth else { return }
        wrapWidth = textColumn.width
        guard !reloading, table.numberOfRows > 0 else { return }
        NSAnimationContext.beginGrouping()
        NSAnimationContext.current.duration = 0
        table.noteHeightOfRows(withIndexesChanged: IndexSet(integersIn: 0..<table.numberOfRows))
        NSAnimationContext.endGrouping()
        if following { scrollToBottom() }
    }

    /// The column autoresizes while our subviews are laid out, so check after
    /// every layout pass too (zoom and split resizes don't go through live resize).
    override func layout() {
        super.layout()
        columnResized()
    }

    override func viewDidEndLiveResize() {
        super.viewDidEndLiveResize()
        columnResized()
    }

    private func setup() {
        table.headerView = nil
        table.backgroundColor = palette.background
        table.usesAlternatingRowBackgroundColors = false
        table.gridStyleMask = []
        table.rowHeight = lineHeight + rowPadding
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.selectionHighlightStyle = .regular
        table.allowsMultipleSelection = true     // shift/⌘-click + click-drag across lines
        table.allowsEmptySelection = true

        gutterColumn.width = showLineNumbers ? gutterWidth() : 0
        gutterColumn.isHidden = !showLineNumbers
        textColumn.resizingMask = .autoresizingMask
        table.addTableColumn(gutterColumn)
        table.addTableColumn(textColumn)
        table.dataSource = self
        table.delegate = self
        table.keyHandler = self

        scrollView.documentView = table
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = !wordWrap
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
        NotificationCenter.default.addObserver(self, selector: #selector(columnResized),
                                               name: NSTableView.columnDidResizeNotification,
                                               object: table)
    }

    /// Take key focus when shown so Home/End/Page keys reach the table without a
    /// click first, and re-sync the table — a background tab's appends went through
    /// reloadData while off-screen, so reload to display the current buffer.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else { return }
        window?.makeFirstResponder(table)
        reload()
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
                reload()
            }
            return
        }

        // Default: keep the window bounded and reloadData. There's no selection to
        // preserve here, and reloadData is always safe (off-screen background tabs,
        // initial big tail loads, evictions of any size) — no row-delta math.
        if lines.count > windowCap { lines.removeFirst(lines.count - windowCap) }
        reload()
        if following { scrollToBottom() }
    }

    func reset() {
        lines.removeAll(keepingCapacity: true)
        filtered.removeAll(keepingCapacity: true)
        matchRows.removeAll(); currentMatch = -1
        following = true
        reload()
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
        reload()      // gutter now renders real numbers (indexingReady == true)
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
            reload()
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
            self.reload()
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
            self.reload()
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

    /// Rows that fit one screen. With wrapping, rows vary in height, so count
    /// what's actually visible; otherwise derive it from the fixed row height
    /// (which also holds for a not-yet-filled table).
    private func viewportRows() -> Int {
        let bounds = scrollView.contentView.bounds
        return max(1, wordWrap ? table.rows(in: bounds).length : Int(bounds.height / table.rowHeight))
    }

    /// Absolute file line currently at the top of the viewport.
    private func currentTopLine() -> Int64 {
        let topRow = table.row(at: NSPoint(x: 0, y: scrollView.contentView.bounds.minY))
        return windowStart + Int64(min(max(0, topRow), max(0, lines.count - 1)))
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
            self.reload()
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
        let n = table.numberOfRows
        setScrollOrigin(y: n > 0 ? table.rect(ofRow: min(max(0, row), n - 1)).minY : 0)
    }

    private func setScrollOrigin(x: CGFloat? = nil, y: CGFloat) {
        let clip = scrollView.contentView
        let maxY = max(0, table.bounds.height - clip.bounds.height)
        let origin = NSPoint(x: x ?? clip.bounds.origin.x, y: min(maxY, max(0, y)))
        suppressed {
            clip.setBoundsOrigin(origin)
            scrollView.reflectScrolledClipView(clip)
        }
    }

    /// The absolute line at the top of the viewport plus the pixel offset into its
    /// row — enough to put the same content back under the viewport after the
    /// buffer or the row heights change, whether or not lines wrap.
    private struct ScrollAnchor { let line: Int64; let offset: CGFloat; let x: CGFloat }

    private func scrollAnchor() -> ScrollAnchor? {
        let origin = scrollView.contentView.bounds.origin
        let row = table.row(at: NSPoint(x: 0, y: origin.y))
        guard row >= 0, row < displayed.count else { return nil }
        return ScrollAnchor(line: displayed[row].number,
                            offset: origin.y - table.rect(ofRow: row).minY, x: origin.x)
    }

    /// Reloads the table and puts the anchored line back at the top of the
    /// viewport (or stays pinned to the tail when following). The bounds observer
    /// is suppressed throughout so paging isn't re-triggered by the shuffle.
    private func reloadRestoring(_ anchor: ScrollAnchor?) {
        suppressScrollHandling = true
        reload()
        if following {
            scrollToBottom()
        } else if let anchor, let row = displayed.firstIndex(where: { $0.number == anchor.line }) {
            setScrollOrigin(x: anchor.x, y: table.rect(ofRow: row).minY + anchor.offset)
        }
        suppressScrollHandling = false
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
        let total = totalLinesProvider?() ?? Int64(lines.count)
        let atVisualBottom = visible.maxY >= documentHeight - table.rowHeight * 1.5
        let atVisualTop = visible.minY <= visible.height        // within one screen of the top

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
            let anchor = self.scrollAnchor()
            self.lines.insert(contentsOf: older, at: 0)
            if self.lines.count > self.windowCap {
                self.lines.removeLast(self.lines.count - self.windowCap)   // evict the far (bottom) end
            }
            self.reloadRestoring(anchor)
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
            let anchor = self.scrollAnchor()
            self.lines.append(contentsOf: newer)
            if self.lines.count > self.windowCap {
                self.lines.removeFirst(self.lines.count - self.windowCap)  // evict the far (top) end
            }
            self.reloadRestoring(anchor)
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
        reload()
    }

    var searchIsValid: Bool { query.isValid }

    @discardableResult
    private func recomputeSearch(preserveCurrent: Bool) -> SearchResult {
        let keepLine = preserveCurrent && currentMatch >= 0 && currentMatch < matchRows.count
            ? displayed[matchRows[currentMatch]].number : nil

        if filterMode {
            let keep = query.matchingIndices(lines.map { $0.text })
            filtered = keep.map { lines[$0] }
            matchRows = Array(0..<filtered.count)
        } else if query.isEmpty {
            matchRows = []
        } else {
            matchRows = query.matchingIndices(displayed.map { $0.text })
        }
        reload()

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
        reload()
    }
}

extension LogView: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int { displayed.count }
}

extension LogView: NSTableViewDelegate {
    /// Fixed height unless wrapping, then one text line per wrapped row. Cheap
    /// enough (no text layout) to be asked for every resident row on each reload.
    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        guard wordWrap, row < displayed.count else { return tableView.rowHeight }
        return CGFloat(wrappedRows(for: displayed[row].text)) * lineHeight + rowPadding
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let line = displayed[row]
        let id = tableColumn!.identifier
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? NSTextField) ?? makeCell(id)
        // Reused cells may predate a wrap toggle, so (re)apply the mode each time.
        // Single-line mode centres vertically; off, both columns top-align so the
        // gutter number sits beside the first wrapped line.
        let isText = id.rawValue == "text"
        cell.cell?.usesSingleLineMode = !wordWrap
        cell.lineBreakMode = (wordWrap && isText) ? .byCharWrapping : .byClipping
        cell.maximumNumberOfLines = (wordWrap && isText) ? 0 : 1

        if !isText {
            // While the background line count runs, real numbers aren't known yet
            // — show a placeholder rather than the provisional local numbers.
            let counting = !(indexingReadyProvider?() ?? true)
            cell.attributedStringValue = NSAttributedString(
                string: counting ? "·" : String(line.number),
                attributes: [.font: rowFont, .foregroundColor: palette.gutter])
            cell.alignment = .right
        } else {
            let rendered = highlighter.render(line.text)
            // Only pay for a mutable copy when there's something to layer on; the
            // common (no search, no wrap) path uses the highlighter's result directly.
            if query.isEmpty && !wordWrap {
                cell.attributedStringValue = rendered
            } else {
                let attr = NSMutableAttributedString(attributedString: rendered)
                applySearchHighlight(attr, line: line, row: row)
                if wordWrap {
                    // An attributed value brings its own paragraph style (word
                    // wrapping by default), overriding the cell's lineBreakMode —
                    // pin it to per-character so the row-height estimate is exact.
                    attr.addAttribute(.paragraphStyle, value: Self.charWrapStyle,
                                      range: NSRange(location: 0, length: attr.length))
                }
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

    private static let charWrapStyle: NSParagraphStyle = {
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = .byCharWrapping
        return p
    }()

    private func makeCell(_ id: NSUserInterfaceItemIdentifier) -> NSTextField {
        let f = NSTextField(labelWithString: "")
        f.identifier = id
        f.font = rowFont
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
