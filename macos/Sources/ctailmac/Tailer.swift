import Foundation

/// A single log line with its 1-based number.
struct LogLine: Equatable {
    let number: Int64
    let text: String
}

/// Polling-based file tailer ported from internal/tailer/tailer.go.
///
/// Design choices:
///   - Polling (not kqueue/FSEvents) so slow/unreachable network mounts can't
///     wedge the UI — every I/O op runs off the main thread and under a timeout.
///   - Inode-change detection for log rotation; truncation detection on shrink.
///   - Only *complete* lines (ending in \n) are committed; a trailing partial is
///     left until the next poll reads it whole.
///
/// Instant tail (the important bit): for large files we seek near the end and
/// show + live-follow the tail IMMEDIATELY, numbering those lines *locally* from
/// the tail start (1, 2, …). The expensive part — counting how many lines precede
/// the tail (`base`) so we can show true line numbers, plus indexing the head for
/// scrollback — runs in the background and never blocks display or following.
/// Absolute line number = `base + localNumber`; until `base` is known the UI
/// shows a placeholder in the gutter.
///
/// The byte-offset index is split into two disjoint, independently-owned regions
/// so the background scan and the live poller never touch the same state:
///   - `headCheckpoints`: sparse offsets for lines before the tail (absolute),
///     written once by the background scan.
///   - `tailCheckpoints`: sparse offsets for lines from the tail onward (local),
///     appended by the live poller.
final class Tailer {
    private let path: String
    private var pollInterval: TimeInterval
    private let readTimeout: TimeInterval
    private let tailFirstThreshold: Int64
    private let tailSeekBack: Int64

    /// Sparse index granularity: one checkpoint per this many lines.
    private let indexStride = 1000

    private let queue = DispatchQueue(label: "net.biseth.ctail.tailer")
    private let ioQueue = DispatchQueue(label: "net.biseth.ctail.tailer.io", attributes: .concurrent)
    private var timer: DispatchSourceTimer?

    // --- state (only touched on `queue`, or synchronously in tests) ---
    private var lineNum: Int64 = 0          // LOCAL line count (lines read since tailStart)
    private var offset: Int64 = 0
    private var fileSize: Int64 = 0
    private var inode: UInt64 = 0
    private var inError = false
    private var running = false
    private(set) var tailStart: Int64 = 0   // byte offset where tail reading began
    private(set) var base: Int64 = 0        // complete lines before tailStart (absolute offset)
    private(set) var baseKnown = true       // false while the background count runs
    private var headCheckpoints: [Int64] = []   // absolute offsets for lines 1...base
    private var tailCheckpoints: [Int64] = []    // offsets for tail lines, indexed locally

    // --- callbacks (invoked on the main queue) ---
    var onLines: (([LogLine]) -> Void)?
    var onReset: (() -> Void)?              // truncation or rotation: clear the view
    var onError: ((String) -> Void)?
    var onReady: (() -> Void)?
    var onBaseResolved: ((Int64) -> Void)?  // background count done; arg = base (lines before tail)

    init(path: String, pollInterval: TimeInterval = 0.25, readTimeout: TimeInterval = 30,
         tailFirstThreshold: Int64 = 1 * 1024 * 1024, tailSeekBack: Int64 = 512 * 1024) {
        self.path = path
        self.pollInterval = max(0.05, pollInterval)
        self.readTimeout = readTimeout
        self.tailFirstThreshold = tailFirstThreshold
        self.tailSeekBack = tailSeekBack
    }

    /// Total lines known so far (grows as the file is tailed; absolute once based).
    var totalLines: Int64 { base + lineNum }
    /// Whether absolute line numbers / scrollback are available yet.
    var indexingComplete: Bool { baseKnown }

    // MARK: - Lifecycle

    func start() {
        queue.async { [weak self] in
            guard let self, !self.running else { return }
            self.running = true
            self.performInitialRead()      // shows + starts following the tail at once
            self.fire { self.onReady?() }
            self.startTimer()              // live polling begins immediately
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.pauseTimer(); self?.running = false
        }
    }

    private func startTimer() {
        timer?.cancel()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + pollInterval, repeating: pollInterval)
        t.setEventHandler { [weak self] in self?.performPoll() }
        timer = t
        t.resume()
    }

    private func pauseTimer() { timer?.cancel(); timer = nil }

    /// Adjusts the poll cadence at runtime (slow inactive/backgrounded tabs).
    func setPollInterval(_ interval: TimeInterval) {
        queue.async { [weak self] in
            guard let self else { return }
            let clamped = max(0.05, interval)
            guard self.running, abs(clamped - self.pollInterval) > 0.001 else { return }
            self.pollInterval = clamped
            self.timer?.schedule(deadline: .now() + clamped, repeating: clamped)
        }
    }

    /// Manual refresh: discard state and re-read from scratch.
    func refresh() {
        queue.async { [weak self] in
            guard let self else { return }
            self.resetState(newSize: 0)
            self.performInitialRead()
            if self.timer == nil, self.running { self.startTimer() }
        }
    }

    // MARK: - Synchronous core (also the test seam)

    func performInitialRead() {
        guard let st = stat() else { return }
        inode = st.inode
        fileSize = st.size
        lineNum = 0; offset = 0; base = 0
        headCheckpoints = []; tailCheckpoints = []

        if st.size > tailFirstThreshold {
            // Instant tail: show the last chunk now (numbered locally), follow live,
            // and count the head in the background to fill in real numbers.
            tailStart = alignToLineBoundary(max(0, st.size - tailSeekBack))
            baseKnown = false
            let (lines, consumed) = readNewLines(from: tailStart, to: st.size, buildTailIndex: true)
            offset = consumed
            if !lines.isEmpty { fire { self.onLines?(lines) } }
            scheduleHeadCount(tailStart: tailStart)
        } else {
            // Small file: read it all from the top; numbers are absolute immediately.
            tailStart = 0
            baseKnown = true
            let (lines, consumed) = readNewLines(from: 0, to: st.size, buildTailIndex: true)
            offset = consumed
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

        let rotated = inode != 0 && st.inode != inode
        if rotated { inode = st.inode }

        if rotated || st.size < fileSize {            // rotation or truncation -> re-read from 0
            resetState(newSize: st.size)
            let (lines, consumed) = readNewLines(from: 0, to: st.size, buildTailIndex: true)
            offset = consumed
            if !lines.isEmpty { fire { self.onLines?(lines) } }
            return
        }

        if wasInError { fire { self.onReady?() } }
        if st.size == offset { return }               // nothing new

        let (lines, consumed) = readNewLines(from: offset, to: st.size, buildTailIndex: true)
        offset = consumed
        fileSize = st.size
        if !lines.isEmpty { fire { self.onLines?(lines) } }
    }

    private func resetState(newSize: Int64) {
        lineNum = 0; offset = 0; fileSize = newSize; inode = 0
        tailStart = 0; base = 0; baseKnown = true
        headCheckpoints = []; tailCheckpoints = []
        fire { self.onReset?() }
    }

    // MARK: - Background head count (large files)

    private func scheduleHeadCount(tailStart: Int64) {
        ioQueue.async { [weak self] in
            guard let self else { return }
            let result = Self.indexFile(path: self.path, upTo: tailStart, stride: self.indexStride)
            self.queue.async { self.applyHeadCount(result.checkpoints, base: result.total) }
        }
    }

    /// Adopts the background head index: records the absolute offsets for the head
    /// region and the line count before the tail, then notifies so the UI can swap
    /// placeholder gutters for real numbers. Live state (offset/lineNum/tail index)
    /// is owned by the poller and left untouched.
    func applyHeadCount(_ checkpoints: [Int64], base newBase: Int64) {
        headCheckpoints = checkpoints
        base = newBase
        baseKnown = true
        fire { self.onBaseResolved?(newBase) }
    }

    // MARK: - Windowed range reads (scrollback)

    /// Reads `count` lines starting at 1-based absolute `start` directly from disk,
    /// seeking to the nearest checkpoint (head or tail region) and scanning forward.
    /// Returns [] until the head count is known.
    func readRange(start: Int64, count: Int) -> [LogLine] {
        let total = base + lineNum
        guard baseKnown, start >= 1, count > 0, start <= total else { return [] }
        let lastLine = min(total, start + Int64(count) - 1)
        guard let (fromByte, lineAtByte) = checkpointAtOrBefore(start) else { return [] }
        let toByte = checkpointAfter(lastLine) ?? fileSize
        guard let data = readBytes(from: fromByte, to: toByte) else { return [] }
        var out: [LogLine] = []
        out.reserveCapacity(Int(lastLine - start + 1))
        var num = lineAtByte
        for slice in splitComplete(data) {
            if num >= start && num <= lastLine { out.append(LogLine(number: num, text: slice)) }
            num += 1
            if num > lastLine { break }
        }
        return out
    }

    /// Byte offset + the absolute line number there, for the checkpoint at/just
    /// before `absLine`. Resolves head vs. tail region.
    private func checkpointAtOrBefore(_ absLine: Int64) -> (byte: Int64, line: Int64)? {
        let s = Int64(indexStride)
        if absLine <= base {
            guard !headCheckpoints.isEmpty else { return nil }
            let k = Int((absLine - 1) / s)
            guard k < headCheckpoints.count else { return nil }
            return (headCheckpoints[k], Int64(k) * s + 1)
        } else {
            let local = absLine - base
            let k = Int((local - 1) / s)
            guard k < tailCheckpoints.count else { return nil }
            return (tailCheckpoints[k], base + Int64(k) * s + 1)
        }
    }

    /// Byte offset of the first checkpoint strictly after `absLine`, or nil (= EOF).
    private func checkpointAfter(_ absLine: Int64) -> Int64? {
        let s = Int64(indexStride)
        if absLine < base {
            let k = Int(absLine / s) + 1
            if k < headCheckpoints.count { return headCheckpoints[k] }
            return tailCheckpoints.first            // crossing into the tail at tailStart
        } else {
            let local = absLine - base
            let k = Int(local / s) + 1
            if k < tailCheckpoints.count { return tailCheckpoints[k] }
            return nil
        }
    }

    /// Async wrapper used by the UI's sliding window.
    func fetchRange(start: Int64, count: Int, completion: @escaping ([LogLine]) -> Void) {
        queue.async { [weak self] in
            let lines = self?.readRange(start: start, count: count) ?? []
            DispatchQueue.main.async { completion(lines) }
        }
    }

    // MARK: - Pure indexer (testable)

    /// Scans [0, size) building a sparse offset index (one checkpoint per `stride`
    /// lines), the complete-line count, and the byte offset past the last complete
    /// line. Each 1 MB chunk read is wrapped in an autoreleasepool so the scan's
    /// transient memory stays at one chunk.
    static func indexFile(path: String, upTo size: Int64, stride: Int = 1000)
        -> (checkpoints: [Int64], total: Int64, consumed: Int64) {
        guard size > 0, let fh = FileHandle(forReadingAtPath: path) else { return ([0], 0, 0) }
        defer { try? fh.close() }
        let s = Int64(max(1, stride))
        var checkpoints: [Int64] = [0]
        var total: Int64 = 0
        var consumed: Int64 = 0
        var pos: Int64 = 0
        let chunkSize = 1 << 20
        let nl = UInt8(ascii: "\n")
        var stop = false
        while pos < size && !stop {
            autoreleasepool {
                guard let chunk = try? fh.read(upToCount: chunkSize), !chunk.isEmpty else { stop = true; return }
                let limit = Int(min(Int64(chunk.count), size - pos))
                chunk.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
                    guard let base = raw.baseAddress else { return }
                    var p = 0
                    // memchr is SIMD-optimized — far faster than scanning byte-by-byte.
                    while p < limit, let hit = memchr(base + p, Int32(nl), limit - p) {
                        let idx = base.distance(to: hit)
                        let next = pos + Int64(idx) + 1
                        total += 1
                        consumed = next
                        if next < size && total % s == 0 { checkpoints.append(next) }
                        p = idx + 1
                    }
                }
                pos += Int64(chunk.count)
            }
        }
        return (checkpoints, total, consumed)
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

    /// Reads [from, to), splits complete lines numbered absolutely (`base + local`),
    /// advances the LOCAL `lineNum`, and appends sparse tail checkpoints.
    private func readNewLines(from: Int64, to: Int64, buildTailIndex: Bool) -> (lines: [LogLine], consumed: Int64) {
        guard let data = readBytes(from: from, to: to), !data.isEmpty else { return ([], from) }
        let (lines, offsets, consumed) = Self.splitLines(data, startingAt: base + lineNum, baseOffset: from)
        if !lines.isEmpty {
            lineNum = lines.last!.number - base       // absolute -> local
            if buildTailIndex {
                let s = Int64(indexStride)
                for (i, line) in lines.enumerated() where (line.number - base - 1) % s == 0 {
                    tailCheckpoints.append(offsets[i])
                }
            }
        }
        return (lines, consumed)
    }

    /// Pure line splitter: complete lines numbered from `startNum`, their start
    /// offsets, and the byte offset just past the last complete line.
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

    private func withTimeout<T>(_ work: @escaping () -> T?) -> T? {
        let sem = DispatchSemaphore(value: 0)
        var result: T?
        ioQueue.async { result = work(); sem.signal() }
        return sem.wait(timeout: .now() + readTimeout) == .timedOut ? nil : result
    }

    private func fire(_ block: @escaping () -> Void) { DispatchQueue.main.async(execute: block) }
}
