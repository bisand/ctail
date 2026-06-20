import Foundation

/// A single log line with its 1-based number.
struct LogLine: Equatable {
    let number: Int64
    let text: String
}

/// Polling-based file tailer ported from internal/tailer/tailer.go.
///
/// Design choices carried over from the Go original:
///   - Polling (not kqueue/FSEvents) so slow/unreachable network mounts can't
///     wedge the UI — every I/O op runs off the main thread and under a timeout.
///   - Inode-change detection for log rotation (file renamed + recreated).
///   - Truncation detection when the file shrinks.
///   - Only *complete* lines (ending in \n) are committed; a trailing partial
///     line is left in place until the next poll reads it whole.
///   - Tail-first: for large files we seek near the end on first read instead of
///     paging the whole thing, then build the full line-offset index in the
///     background so scrollback (ReadRange) and the total count become available.
///
/// The pure line-splitting and indexing logic is factored into static functions
/// (`splitLines`, `indexOffsets`) and the poll body into synchronous `perform*`
/// methods so the engine is testable without the async timer.
final class Tailer {
    private let path: String
    private let pollInterval: TimeInterval
    private let readTimeout: TimeInterval
    private let tailFirstThreshold: Int64 = 1 * 1024 * 1024   // 1 MB
    private let tailSeekBack: Int64 = 512 * 1024              // 512 KB

    private let queue = DispatchQueue(label: "net.biseth.ctail.tailer")
    // Concurrent so a long background index doesn't block timed reads behind it.
    private let ioQueue = DispatchQueue(label: "net.biseth.ctail.tailer.io", attributes: .concurrent)
    private var timer: DispatchSourceTimer?

    // --- state (only touched on `queue`, or synchronously in tests) ---
    private var lineNum: Int64 = 0
    private var offset: Int64 = 0
    private var fileSize: Int64 = 0
    private var inode: UInt64 = 0
    private var inError = false
    private var running = false
    /// Byte offset of the start of each line; lineOffsets[i] is line (i+1).
    private(set) var lineOffsets: [Int64] = []
    private(set) var indexingComplete = true

    // --- callbacks (invoked on the main queue) ---
    var onLines: (([LogLine]) -> Void)?
    var onReset: (() -> Void)?          // truncation or rotation: clear the view
    var onError: ((String) -> Void)?
    var onReady: (() -> Void)?
    var onIndexed: ((Int64) -> Void)?   // background indexing finished; arg = total lines

    init(path: String, pollInterval: TimeInterval = 0.25, readTimeout: TimeInterval = 30) {
        self.path = path
        self.pollInterval = max(0.05, pollInterval)
        self.readTimeout = readTimeout
    }

    var totalLines: Int64 { Int64(lineOffsets.count) }

    // MARK: - Lifecycle

    func start() {
        queue.async { [weak self] in
            guard let self, !self.running else { return }
            self.running = true
            self.performInitialRead()
            self.fire { self.onReady?() }
            let t = DispatchSource.makeTimerSource(queue: self.queue)
            t.schedule(deadline: .now() + self.pollInterval, repeating: self.pollInterval)
            t.setEventHandler { [weak self] in self?.performPoll() }
            self.timer = t
            t.resume()
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.timer?.cancel(); self?.timer = nil; self?.running = false
        }
    }

    /// Manual refresh: discard state and re-read the file from scratch (used by
    /// the tab "Refresh" command). Fires onReset so the view clears first.
    func refresh() {
        queue.async { [weak self] in
            guard let self else { return }
            self.lineNum = 0; self.offset = 0; self.fileSize = 0; self.inode = 0; self.lineOffsets = []
            self.fire { self.onReset?() }
            self.performInitialRead()
        }
    }

    // MARK: - Synchronous core (also the test seam)

    func performInitialRead() {
        guard let st = stat() else { return }
        inode = st.inode
        fileSize = st.size

        if st.size > tailFirstThreshold {
            // Tail-first: read only the tail now; index the rest in the background.
            let start = alignToLineBoundary(max(0, st.size - tailSeekBack))
            let (lines, consumed) = readNewLines(from: start, to: st.size, appendOffsets: false)
            // Seed the offset index with just the tail lines (renumbered later by the indexer).
            lineOffsets = lines.isEmpty ? [] : Array(repeating: 0, count: lines.count)
            offset = consumed
            indexingComplete = false
            if !lines.isEmpty { fire { self.onLines?(lines) } }
            scheduleBackgroundIndex(targetSize: st.size)
        } else {
            let (lines, consumed) = readNewLines(from: 0, to: st.size, appendOffsets: true)
            offset = consumed
            indexingComplete = true
            if !lines.isEmpty { fire { self.onLines?(lines) } }
        }
    }

    func performPoll() {
        guard let st = stat() else {
            if !inError { inError = true; fire { self.onError?("file unavailable: \(self.path)") } }
            return
        }
        let wasInError = inError
        inError = false

        // Rotation: same path, different inode -> the old file rolled away.
        let rotated = inode != 0 && st.inode != inode
        if rotated { inode = st.inode }

        if rotated || st.size < fileSize {            // rotation or truncation -> re-read from 0
            resetState(newSize: st.size)
            let (lines, consumed) = readNewLines(from: 0, to: st.size, appendOffsets: true)
            offset = consumed
            if !lines.isEmpty { fire { self.onLines?(lines) } }
            return
        }

        if wasInError { fire { self.onReady?() } }
        if st.size == offset { return }               // nothing new

        let (lines, consumed) = readNewLines(from: offset, to: st.size, appendOffsets: true)
        offset = consumed
        fileSize = st.size
        if !lines.isEmpty { fire { self.onLines?(lines) } }
    }

    private func resetState(newSize: Int64) {
        lineNum = 0; offset = 0; fileSize = newSize; lineOffsets = []
        fire { self.onReset?() }
    }

    // MARK: - Windowed range reads (scrollback beyond the live buffer)

    /// Reads `count` lines starting at 1-based `start` directly from disk using
    /// the offset index. Returns [] if indexing hasn't reached that range yet.
    func readRange(start: Int64, count: Int) -> [LogLine] {
        guard start >= 1, count > 0, !lineOffsets.isEmpty else { return [] }
        let lo = Int(start - 1)
        guard lo < lineOffsets.count else { return [] }
        let hi = min(lo + count, lineOffsets.count)
        let from = lineOffsets[lo]
        let to = (hi < lineOffsets.count) ? lineOffsets[hi] : fileSize
        guard let data = readBytes(from: from, to: to) else { return [] }
        var out: [LogLine] = []
        var num = start
        for slice in splitComplete(data) {
            out.append(LogLine(number: num, text: slice)); num += 1
            if out.count == count { break }
        }
        return out
    }

    // MARK: - Background indexing (large files)

    private func scheduleBackgroundIndex(targetSize: Int64) {
        ioQueue.async { [weak self] in
            guard let self else { return }
            let offsets = Self.indexOffsets(path: self.path, upTo: targetSize)
            self.queue.async {
                guard !offsets.isEmpty else { self.indexingComplete = true; return }
                self.lineOffsets = offsets
                self.indexingComplete = true
                let total = Int64(offsets.count)
                self.fire { self.onIndexed?(total) }
            }
        }
    }

    /// Scans a file building the byte offset of every line start. Pure + testable.
    static func indexOffsets(path: String, upTo size: Int64) -> [Int64] {
        guard let fh = FileHandle(forReadingAtPath: path) else { return [] }
        defer { try? fh.close() }
        var offsets: [Int64] = [0]
        var pos: Int64 = 0
        let chunkSize = 1 << 20
        while pos < size {
            guard let chunk = try? fh.read(upToCount: chunkSize), !chunk.isEmpty else { break }
            var i = chunk.startIndex
            while i < chunk.endIndex {
                if chunk[i] == UInt8(ascii: "\n") {
                    let next = pos + Int64(chunk.distance(from: chunk.startIndex, to: i)) + 1
                    if next < size { offsets.append(next) }
                }
                i = chunk.index(after: i)
            }
            pos += Int64(chunk.count)
        }
        return offsets
    }

    // MARK: - I/O (all under a timeout so dead mounts can't wedge the queue)

    private func stat() -> (size: Int64, inode: UInt64)? {
        withTimeout {
            guard let a = try? FileManager.default.attributesOfItem(atPath: self.path) else { return nil }
            let size = (a[.size] as? NSNumber)?.int64Value ?? 0
            let ino = (a[.systemFileNumber] as? NSNumber)?.uint64Value ?? 0
            return (size, ino)
        }
    }

    private func readBytes(from: Int64, to: Int64) -> Data? {
        guard to > from else { return Data() }
        return withTimeout {
            guard let fh = FileHandle(forReadingAtPath: self.path) else { return nil }
            defer { try? fh.close() }
            try? fh.seek(toOffset: UInt64(from))
            return (try? fh.read(upToCount: Int(to - from))) ?? Data()
        } ?? nil
    }

    /// Reads [from, to), splits complete lines, advances `lineNum`, and (when
    /// asked) appends each new line's start offset to the index.
    private func readNewLines(from: Int64, to: Int64, appendOffsets: Bool) -> (lines: [LogLine], consumed: Int64) {
        guard let data = readBytes(from: from, to: to), !data.isEmpty else { return ([], from) }
        let (lines, offsets, consumed) = Self.splitLines(data, startingAt: lineNum, baseOffset: from)
        if !lines.isEmpty {
            lineNum = lines.last!.number
            if appendOffsets { lineOffsets.append(contentsOf: offsets) }
        }
        return (lines, consumed)
    }

    /// Pure line splitter: returns complete lines, their start offsets, and the
    /// byte offset just past the last complete line (trailing partial left behind).
    static func splitLines(_ data: Data, startingAt startNum: Int64, baseOffset: Int64)
        -> (lines: [LogLine], offsets: [Int64], consumed: Int64) {
        var lines: [LogLine] = []
        var offsets: [Int64] = []
        var consumed = baseOffset
        var num = startNum
        var lineStart = data.startIndex
        let nl = UInt8(ascii: "\n")
        var i = data.startIndex
        while i < data.endIndex {
            if data[i] == nl {
                var slice = data[lineStart..<i]
                if slice.last == UInt8(ascii: "\r") { slice = slice.dropLast() }
                num += 1
                offsets.append(baseOffset + Int64(data.distance(from: data.startIndex, to: lineStart)))
                lines.append(LogLine(number: num, text: String(decoding: slice, as: UTF8.self)))
                consumed = baseOffset + Int64(data.distance(from: data.startIndex, to: i)) + 1
                lineStart = data.index(after: i)
            }
            i = data.index(after: i)
        }
        return (lines, offsets, consumed)
    }

    /// Splits a buffer into complete-line strings (drops a trailing partial).
    private func splitComplete(_ data: Data) -> [String] {
        Self.splitLines(data, startingAt: 0, baseOffset: 0).lines.map { $0.text }
    }

    private func alignToLineBoundary(_ start: Int64) -> Int64 {
        guard start > 0, let data = readBytes(from: start, to: start + 64 * 1024) else { return start }
        if let nl = data.firstIndex(of: UInt8(ascii: "\n")) {
            return start + Int64(data.distance(from: data.startIndex, to: nl)) + 1
        }
        return start
    }

    /// Runs `work` on a separate queue and waits up to `readTimeout`; returns nil
    /// on timeout so a hung mount degrades to an error instead of freezing.
    private func withTimeout<T>(_ work: @escaping () -> T?) -> T? {
        let sem = DispatchSemaphore(value: 0)
        var result: T?
        ioQueue.async { result = work(); sem.signal() }
        return sem.wait(timeout: .now() + readTimeout) == .timedOut ? nil : result
    }

    private func fire(_ block: @escaping () -> Void) { DispatchQueue.main.async(execute: block) }
}
