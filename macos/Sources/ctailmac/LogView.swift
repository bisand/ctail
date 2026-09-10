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
    /// Absolute file line at the top of the viewport — where the whole-file
    /// search measures "nearest match" from.
    var topLine: Int64 { currentTopLine() }


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
    /// Columns of the widest resident line. The text column is sized to it, and
    /// that is the whole of horizontal scrolling: a table wider than its clip
    /// view is something NSScrollView scrolls sideways by itself. Kept
    /// incrementally as lines arrive; recounted when eviction may have taken
    /// the widest one.
    private var widestColumns = 0

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
        updateTextColumnWidth()
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
        recountWidest()
        updateTextColumnWidth()
        table.reloadData()
        reloading = false
    }

    /// Columns a line takes in the monospaced row font. Exact for ASCII;
    /// non-ASCII scalars count double (CJK/emoji) and tabs as 4, which errs
    /// toward spare room rather than clipping. O(n) over the bytes and no text
    /// layout, so it's cheap enough to run on every line that arrives.
    private func columns(of text: String) -> Int {
        var cols = 0
        for b in text.utf8 {
            if b < 0x80 { cols += (b == 0x09) ? 4 : 1 } else if b & 0xC0 != 0x80 { cols += 2 }
        }
        return cols
    }

    /// Rows a line occupies when wrapped, from its column count and the cell
    /// width; wrapping is per character, so this is exact for what `columns`
    /// is exact for.
    private func wrappedRows(for text: String) -> Int {
        let usable = textColumn.width - 2 * LogRowCell.insetX
        guard usable > charAdvance else { return 1 }
        let perRow = max(1, Int(usable / charAdvance))
        return max(1, (columns(of: text) + perRow - 1) / perRow)
    }

    private func noteWidth(of newLines: [LogLine]) {
        for line in newLines { widestColumns = max(widestColumns, columns(of: line.text)) }
    }

    private func recountWidest() {
        widestColumns = lines.reduce(0) { max($0, columns(of: $1.text)) }
    }

    /// Sizes the text column: the viewport's width when wrapping, since rows
    /// wrap to it; otherwise the wider of the viewport and the widest line,
    /// which is what gives the scroll view something to scroll sideways to.
    private func updateTextColumnWidth() {
        let gutter = showLineNumbers ? gutterColumn.width : 0
        let available = max(0, scrollView.contentView.bounds.width - gutter)
        let content = CGFloat(widestColumns) * charAdvance + 2 * LogRowCell.insetX + charAdvance
        let width = wordWrap ? available : max(available, content)
        if textColumn.width != width { textColumn.width = width }
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
        updateTextColumnWidth()
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
        // Neither column follows the table's width: the text column is sized to
        // the content (see `updateTextColumnWidth`), and a table wider than its
        // clip view is what scrolls sideways.
        table.columnAutoresizingStyle = .noColumnAutoresizing
        textColumn.resizingMask = []
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

        // Frozen while the user has a selection (or is mid-drag): keep the selection
        // and the visible content perfectly put by ONLY appending rows at the bottom
        // — no eviction, no scroll, no reloadData (which would shift the selection).
        // Safe to use insertRows only when the table is displayed and its row count
        // matches the buffer; appending at `firstNew == numberOfRows` can't go out
        // of range. A growth cap (then a reload) bounds a long-held selection.
        if (table.isDragging || hasSelection), table.window != nil, table.numberOfRows == firstNew {
            lines.append(contentsOf: newLines)
            noteWidth(of: newLines)
            updateTextColumnWidth()
            let hardCap = windowCap * 3
            if lines.count <= hardCap {
                table.insertRows(at: IndexSet(integersIn: firstNew..<lines.count), withAnimation: [])
            } else {
                lines.removeFirst(lines.count - hardCap)
                reload()
            }
            return
        }

        // Default: the window slides along the tail, evicting from the top.
        slide(appending: newLines)
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
    private func goTo(topLine: Int64, then landed: (() -> Void)? = nil) {
        guard !filterMode else { return }
        let total = totalLinesProvider?() ?? Int64(lines.count)
        let clampedTop = min(max(1, topLine), max(1, total))
        let rows = Int64(viewportRows())
        let haveAbove = !lines.isEmpty && clampedTop >= windowStart
        let haveBelow = windowEnd >= min(total, clampedTop + rows - 1)

        if haveAbove && haveBelow {                      // already resident — instant scroll
            setFollowing(false)
            scrollRowToTop(Int(clampedTop - windowStart))
            landed?()
            return
        }
        guard let requestRange, (indexingReadyProvider?() ?? false) else {
            scrollRowToTop(Int(max(0, clampedTop - windowStart)))
            landed?()
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
            landed?()
        }
    }

    /// Puts the line numbered `number` in the middle of the view — paging the
    /// part of the file around it in from disk when it is not resident — and
    /// makes it the current match if it is one. For the whole-file search,
    /// whose matches are mostly lines the window has never held. Following
    /// stops, as for any scroll away from the tail; End brings it back.
    func reveal(line number: Int64) {
        guard !filterMode else { return }
        let half = Int64(viewportRows() / 2)
        goTo(topLine: max(1, number - half)) { [weak self] in
            guard let self, let row = self.lines.firstIndex(where: { $0.number == number }) else { return }
            if !self.query.isEmpty {
                // The window may be a new one; the match rows are indices into it.
                self.recomputeSearch(preserveCurrent: false, focus: false)
                self.currentMatch = self.matchRows.firstIndex(of: row) ?? -1
            }
            self.table.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
            self.reload()
            self.scrollRowToTop(max(0, row - Int(half)))
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
        suppressed { place(clip, at: NSPoint(x: x ?? clip.bounds.origin.x, y: y)) }
    }

    /// Scrolls `clip` to `origin`, clamped to the content. For a caller already
    /// inside `suppressed`.
    private func place(_ clip: NSClipView, at origin: NSPoint) {
        let maxY = max(0, table.bounds.height - clip.bounds.height)
        clip.setBoundsOrigin(NSPoint(x: origin.x, y: min(maxY, max(0, origin.y))))
        scrollView.reflectScrolledClipView(clip)
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
            self.slide(insertingAtTop: older)
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
            self.slide(appending: newer)
        }
    }

    // MARK: - Sliding the window without a reload

    /// Whether the table can take rows in and out around the viewport rather
    /// than be rebuilt: it has to be showing exactly the buffer, row for row.
    /// Filter mode and a search project the buffer, a background tab's table is
    /// stale until it is shown again, and an empty table is cheaper to fill.
    private var canSlide: Bool {
        !filterMode && query.isEmpty && table.window != nil
            && table.numberOfRows == lines.count && table.numberOfRows > 0
    }

    /// Pages `older` in above the window, evicting the same weight from the
    /// bottom, and leaves every row on screen exactly where it was.
    ///
    /// `reloadData` was the safe choice here — no row-delta arithmetic — but it
    /// discards every visible row view and remakes them at ~180 µs each: 11 ms
    /// on the main thread per page-in, which is a hitch a flick can feel and a
    /// momentum scroll does not survive. Inserting and removing rows costs the
    /// table nothing for rows outside the viewport, and the contiguity checks
    /// the callers make are what keep the arithmetic honest: what goes in at
    /// one end is exactly the count that comes off the other.
    private func slide(insertingAtTop older: [LogLine]) {
        guard !older.isEmpty else { return }
        guard canSlide else {
            let anchor = scrollAnchor()
            lines.insert(contentsOf: older, at: 0)
            if lines.count > windowCap { lines.removeLast(lines.count - windowCap) }
            reloadRestoring(anchor)
            return
        }
        let before = table.numberOfRows
        let evict = max(0, before + older.count - windowCap)
        lines.insert(contentsOf: older, at: 0)
        if evict > 0 { lines.removeLast(evict); recountWidest() } else { noteWidth(of: older) }
        updateTextColumnWidth()
        // What is about to appear above the viewport is exactly how far the
        // content has to move to stay put: the rows' heights, as the table
        // will ask for them, so wrapped rows count fully.
        let added = (0..<older.count).reduce(CGFloat(0)) { $0 + tableView(table, heightOfRow: $1) }
        suppressed {
            let clip = scrollView.contentView
            let target = NSPoint(x: clip.bounds.origin.x, y: clip.bounds.origin.y + added)
            // The viewport goes to where the content will be *before* the rows
            // go in. `endUpdates` is when the table decides which rows are on
            // screen and makes views for them; at the old origin those would
            // be the new rows — a whole screen of views made for the scroll to
            // hide a moment later, and kept in the reuse pool for ever after.
            // Past the end of the content for a moment, which the clip view
            // allows and nothing draws in between.
            clip.setBoundsOrigin(target)
            table.beginUpdates()
            if evict > 0 {
                table.removeRows(at: IndexSet(integersIn: (before - evict)..<before), withAnimation: [])
            }
            table.insertRows(at: IndexSet(integersIn: 0..<older.count), withAnimation: [])
            table.endUpdates()
            place(clip, at: target)
        }
    }

    /// Adds `newer` below the window, evicting the same weight from the top,
    /// with the rows on screen left where they were. See `slide(insertingAtTop:)`.
    private func slide(appending newer: [LogLine]) {
        guard !newer.isEmpty else { return }
        guard canSlide else {
            let anchor = scrollAnchor()
            lines.append(contentsOf: newer)
            if lines.count > windowCap { lines.removeFirst(lines.count - windowCap) }
            reloadRestoring(anchor)
            return
        }
        let before = table.numberOfRows
        let evict = max(0, before + newer.count - windowCap)
        // The height about to leave above the viewport, measured before it goes.
        let removed = evict > 0 ? table.rect(ofRow: evict).minY - table.rect(ofRow: 0).minY : 0
        lines.append(contentsOf: newer)
        if evict > 0 { lines.removeFirst(evict); recountWidest() } else { noteWidth(of: newer) }
        updateTextColumnWidth()
        suppressed {
            let clip = scrollView.contentView
            let target = NSPoint(x: clip.bounds.origin.x, y: clip.bounds.origin.y - removed)
            // Viewport first, rows second; see `slide(insertingAtTop:)`.
            clip.setBoundsOrigin(target)
            table.beginUpdates()
            if evict > 0 {
                table.removeRows(at: IndexSet(integersIn: 0..<evict), withAnimation: [])
            }
            let kept = before - evict
            table.insertRows(at: IndexSet(integersIn: kept..<(kept + newer.count)), withAnimation: [])
            table.endUpdates()
            place(clip, at: target)
        }
    }

    // MARK: - Performance harness hooks

    /// Rebuilds the table keeping the same content under the viewport — what a
    /// settings toggle and every page-in do — so the self-test can time it.
    func perfReloadInPlace() { reloadRestoring(scrollAnchor()) }

    /// Pages older lines in at the top, as scrolling near it does. The harness
    /// answers `requestRange` inline, so this is the whole page-in but the disk.
    func perfPageIn() { pageUp() }

    /// Puts the first resident row at the top of the viewport.
    func perfScrollToTop() { scrollRowToTop(0) }

    /// How many row views were made and row heights asked since the last
    /// reset: what a page-in costs the table, counted rather than timed.
    private(set) var perfViewsMade = 0
    private(set) var perfHeightsAsked = 0
    func perfResetCounters() { perfViewsMade = 0; perfHeightsAsked = 0 }

    /// Content and viewport widths: the difference is what can scroll sideways.
    var perfTableWidth: CGFloat { table.frame.width }
    var perfViewportWidth: CGFloat { scrollView.contentView.bounds.width }

    /// The last resident line, so an append can be made contiguous with it.
    var perfLastLine: Int64? { lines.last?.number }

    /// Appends as the tailer does while the view follows the tail.
    func perfAppend(_ newLines: [LogLine]) {
        following = true
        append(newLines)
    }

    /// The pixel offset of the viewport into its top row, to check that a
    /// page-in left the content exactly where it was.
    var perfTopOffset: CGFloat {
        let y = scrollView.contentView.bounds.minY
        let row = table.row(at: NSPoint(x: 0, y: y))
        return row < 0 ? -1 : y - table.rect(ofRow: row).minY
    }

    /// Makes (or remakes) the row views for every visible row, as a scroll does
    /// for the rows that come into view; returns how many that was.
    func perfMakeVisibleRows() -> Int {
        // A display pass, as the window would run: it is what makes the row
        // views for the visible rect and commits them to the table.
        table.layoutSubtreeIfNeeded()
        table.display()
        return table.rows(in: table.visibleRect).length
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
    private func recomputeSearch(preserveCurrent: Bool, focus: Bool = true) -> SearchResult {
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
        if focus { focusCurrentMatch() }
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
        perfHeightsAsked += 1
        guard wordWrap, row < displayed.count else { return tableView.rowHeight }
        return CGFloat(wrappedRows(for: displayed[row].text)) * lineHeight + rowPadding
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        perfViewsMade += 1
        let line = displayed[row]
        let id = tableColumn!.identifier
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? LogRowCell) ?? makeCell(id)
        let isText = id.rawValue == "text"

        if !isText {
            // While the background line count runs, real numbers aren't known yet
            // — show a placeholder rather than the provisional local numbers.
            let counting = !(indexingReadyProvider?() ?? true)
            let number = NSAttributedString(
                string: counting ? "·" : String(line.number),
                attributes: [.font: rowFont, .foregroundColor: palette.gutter])
            // Top-aligned like the text when wrapping, so the number sits
            // beside the first wrapped line.
            cell.set(number, wraps: false, alignment: .right, centred: !wordWrap)
        } else {
            let rendered = highlighter.render(line.text)
            // Only pay for a mutable copy when there's something to layer on; the
            // common (no search, no wrap) path uses the highlighter's result directly.
            if query.isEmpty && !wordWrap {
                cell.set(rendered, wraps: false, alignment: .left, centred: true)
            } else {
                let attr = NSMutableAttributedString(attributedString: rendered)
                applySearchHighlight(attr, line: line, row: row)
                if wordWrap {
                    // Wrapping is per character, so the row-height estimate
                    // in `wrappedRows` is exact.
                    attr.addAttribute(.paragraphStyle, value: Self.charWrapStyle,
                                      range: NSRange(location: 0, length: attr.length))
                }
                cell.set(attr, wraps: wordWrap, alignment: .left, centred: !wordWrap)
            }
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

    private func makeCell(_ id: NSUserInterfaceItemIdentifier) -> LogRowCell {
        let cell = LogRowCell(lineHeight: lineHeight)
        cell.identifier = id
        return cell
    }
}

/// A row cell that draws its attributed string and nothing else.
///
/// `NSTextField` was the cell here, and a text field costs what a text field
/// costs: a cell object, an Auto Layout engine, a dozen constraints and their
/// observations, per field — two hundred microseconds to make, and, kept in
/// the table's reuse pool, a plateau of sixty megabytes after a long scroll
/// through a big file. A log row needs one thing done: its string drawn at a
/// point, clipped at the edge, or wrapped in its rect when wrapping is on.
final class LogRowCell: NSView {
    /// Horizontal inset each side; `wrappedRows` estimates against the same
    /// eight points in total.
    static let insetX: CGFloat = 4

    private var text = NSAttributedString()
    private var wraps = false
    private var alignment: NSTextAlignment = .left
    private var centred = true
    private let lineHeight: CGFloat

    init(lineHeight: CGFloat) {
        self.lineHeight = lineHeight
        super.init(frame: .zero)
    }
    required init?(coder: NSCoder) { fatalError() }

    /// Top-left is the origin, as the table's row frames are.
    override var isFlipped: Bool { true }

    func set(_ text: NSAttributedString, wraps: Bool, alignment: NSTextAlignment, centred: Bool) {
        self.text = text
        self.wraps = wraps
        self.alignment = alignment
        self.centred = centred
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        let inset = bounds.insetBy(dx: Self.insetX, dy: 0)
        if wraps {
            // Wrapped in the rect's width by the string's own paragraph style;
            // the rect is the row's, so nothing draws past it.
            text.draw(with: inset, options: [.usesLineFragmentOrigin], context: nil)
            return
        }
        // One line: the view's bounds clip it at the edge, which is what
        // `byClipping` did. Centred in the row, as a single-line field was.
        let y = centred ? (bounds.height - lineHeight) / 2 : 0
        let x = alignment == .right ? inset.maxX - text.size().width : inset.minX
        text.draw(at: NSPoint(x: x, y: y))
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
