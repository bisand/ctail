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
///     paging the whole thing, then build a sparse line-offset index in the
///     background so scrollback (ReadRange) and the total count become available.
///     The index keeps one checkpoint per `indexStride` lines, so its memory cost
///     is ~1000× smaller than one offset per line — readRange seeks to the nearest
///     checkpoint and scans forward.
///
/// The pure line-splitting and indexing logic is factored into static functions
/// (`splitLines`, `indexFile`) and the poll body into synchronous `perform*`
/// methods so the engine is testable without the async timer.
final class Tailer {
    private let path: String
    private var pollInterval: TimeInterval
    private let readTimeout: TimeInterval
    private let tailFirstThreshold: Int64 = 1 * 1024 * 1024   // 1 MB
    private let tailSeekBack: Int64 = 512 * 1024              // 512 KB

    private let queue = DispatchQueue(label: "net.biseth.ctail.tailer")
    // Concurrent so a long background index doesn't block timed reads behind it.
    private let ioQueue = DispatchQueue(label: "net.biseth.ctail.tailer.io", attributes: .concurrent)
    private var timer: DispatchSourceTimer?

    /// Sparse index granularity: one byte-offset checkpoint is kept per this many
    /// lines (so the index costs 8 bytes / `indexStride` lines instead of 8 bytes
    /// per line — ~1000× less memory on a huge file). `readRange` seeks to the
    /// nearest checkpoint and scans forward at most `indexStride` lines.
    private let indexStride = 1000

    // --- state (only touched on `queue`, or synchronously in tests) ---
    private var lineNum: Int64 = 0
    private var offset: Int64 = 0
    private var fileSize: Int64 = 0
    private var inode: UInt64 = 0
    private var inError = false
    private var running = false
    /// Byte offset of the start of every `indexStride`-th line: `checkpoints[k]`
    /// is the start of line `k * indexStride + 1`.
    private(set) var checkpoints: [Int64] = []
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

    var totalLines: Int64 { lineNum }

    // MARK: - Lifecycle

    func start() {
        queue.async { [weak self] in
            guard let self, !self.running else { return }
            self.running = true
            self.performInitialRead()
            self.fire { self.onReady?() }
            // For large files performInitialRead pauses polling until the
            // background index is ready (so line numbers stay consistent); only
            // start the timer here when the index is already complete.
            if self.indexingComplete { self.startTimer() }
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

    /// Adjusts the poll cadence at runtime (issue #16: slow inactive/backgrounded
    /// tabs to keep CPU near zero, speed the active tab back up).
    func setPollInterval(_ interval: TimeInterval) {
        queue.async { [weak self] in
            guard let self else { return }
            let clamped = max(0.05, interval)
            guard self.running, abs(clamped - self.pollInterval) > 0.001 else { return }
            self.pollInterval = clamped
            self.timer?.schedule(deadline: .now() + clamped, repeating: clamped)
        }
    }

    /// Manual refresh: discard state and re-read the file from scratch (used by
    /// the tab "Refresh" command). Fires onReset so the view clears first.
    func refresh() {
        queue.async { [weak self] in
            guard let self else { return }
            self.lineNum = 0; self.offset = 0; self.fileSize = 0; self.inode = 0; self.checkpoints = []
            self.fire { self.onReset?() }
            self.performInitialRead()
            // Small files index synchronously above; resume polling here. (Large
            // files re-pause and let the background index restart the timer.)
            if self.indexingComplete, self.running { self.startTimer() }
        }
    }

    // MARK: - Synchronous core (also the test seam)

    func performInitialRead() {
        guard let st = stat() else { return }
        inode = st.inode
        fileSize = st.size

        if st.size > tailFirstThreshold {
            // Tail-first: show the tail immediately, then index the whole file in
            // the background. Live polling is paused until the index is ready so
            // line numbers and the offset index stay consistent — a huge file is
            // historical, and a brief delay before live updates is acceptable.
            // The tail lines carry provisional numbers; they are renumbered from
            // disk once indexing completes (see scheduleBackgroundIndex / onIndexed).
            pauseTimer()
            let start = alignToLineBoundary(max(0, st.size - tailSeekBack))
            let (lines, _) = readNewLines(from: start, to: st.size, appendOffsets: false)
            checkpoints = []
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
        lineNum = 0; offset = 0; fileSize = newSize; checkpoints = []
        fire { self.onReset?() }
    }

    // MARK: - Windowed range reads (scrollback beyond the live buffer)

    /// Reads `count` lines starting at 1-based `start` directly from disk. Seeks to
    /// the nearest preceding checkpoint and scans forward, so only a sparse index
    /// is needed. Returns [] if indexing hasn't reached that range yet.
    func readRange(start: Int64, count: Int) -> [LogLine] {
        let total = lineNum
        guard start >= 1, count > 0, !checkpoints.isEmpty, start <= total else { return [] }
        let s = Int64(indexStride)
        let lastLine = min(total, start + Int64(count) - 1)
        let startCP = Int((start - 1) / s)
        guard startCP < checkpoints.count else { return [] }
        let fromByte = checkpoints[startCP]
        let firstLineAtCP = Int64(startCP) * s + 1          // line number at fromByte
        // End at the first checkpoint strictly past lastLine, else EOF.
        let endCP = Int(lastLine / s) + 1
        let toByte = endCP < checkpoints.count ? checkpoints[endCP] : fileSize
        guard let data = readBytes(from: fromByte, to: toByte) else { return [] }
        var out: [LogLine] = []
        out.reserveCapacity(Int(lastLine - start + 1))
        var num = firstLineAtCP
        for slice in splitComplete(data) {                  // scan forward from the checkpoint
            if num >= start && num <= lastLine { out.append(LogLine(number: num, text: slice)) }
            num += 1
            if num > lastLine { break }
        }
        return out
    }

    /// Async wrapper around `readRange` used by the UI's sliding window: runs the
    /// disk read on the tailer queue and delivers the lines back on the main queue.
    func fetchRange(start: Int64, count: Int, completion: @escaping ([LogLine]) -> Void) {
        queue.async { [weak self] in
            let lines = self?.readRange(start: start, count: count) ?? []
            DispatchQueue.main.async { completion(lines) }
        }
    }

    // MARK: - Background indexing (large files)

    private func scheduleBackgroundIndex(targetSize: Int64) {
        ioQueue.async { [weak self] in
            guard let self else { return }
            let (checkpoints, total, consumed) = Self.indexFile(path: self.path, upTo: targetSize,
                                                                stride: self.indexStride)
            self.queue.async {
                // Adopt the index as authoritative and fix up the state the
                // provisional tail read left approximate: true line count and the
                // byte offset where live tailing resumes.
                if total > 0 {
                    self.checkpoints = checkpoints
                    self.lineNum = total
                    self.offset = consumed
                    self.fileSize = targetSize
                }
                self.indexingComplete = true
                if self.running { self.startTimer() }   // resume live tailing
                if total > 0 { self.fire { self.onIndexed?(total) } }
            }
        }
    }

    /// Scans a file building a sparse byte-offset index (one checkpoint per
    /// `stride` lines), the total complete-line count, and the byte offset just
    /// past the last complete line (where live tailing resumes, so a trailing
    /// partial line is re-read whole on the next poll). Pure + testable.
    ///
    /// Each 1 MB chunk read is wrapped in an autoreleasepool so the scan's
    /// transient memory stays at one chunk rather than accumulating the whole file.
    static func indexFile(path: String, upTo size: Int64, stride: Int = 1000)
        -> (checkpoints: [Int64], total: Int64, consumed: Int64) {
        guard let fh = FileHandle(forReadingAtPath: path) else { return ([], 0, 0) }
        defer { try? fh.close() }
        let s = Int64(max(1, stride))
        var checkpoints: [Int64] = [0]      // line 1 starts at byte 0
        var total: Int64 = 0                // complete (newline-terminated) lines
        var consumed: Int64 = 0
        var pos: Int64 = 0
        let chunkSize = 1 << 20
        let nl = UInt8(ascii: "\n")
        var stop = false
        while pos < size && !stop {
            autoreleasepool {
                guard let chunk = try? fh.read(upToCount: chunkSize), !chunk.isEmpty else { stop = true; return }
                chunk.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
                    for k in 0..<raw.count where raw[k] == nl {
                        let next = pos + Int64(k) + 1
                        total += 1
                        consumed = next
                        // `next` starts line (total + 1); keep it as a checkpoint
                        // when that line number is 1 + k*stride (total a multiple).
                        if next < size && total % s == 0 { checkpoints.append(next) }
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

    /// Reads [from, to), splits complete lines, advances `lineNum`, and (when
    /// asked) records a sparse checkpoint for every `indexStride`-th line.
    private func readNewLines(from: Int64, to: Int64, appendOffsets: Bool) -> (lines: [LogLine], consumed: Int64) {
        guard let data = readBytes(from: from, to: to), !data.isEmpty else { return ([], from) }
        let (lines, offsets, consumed) = Self.splitLines(data, startingAt: lineNum, baseOffset: from)
        if !lines.isEmpty {
            lineNum = lines.last!.number
            if appendOffsets {
                let s = Int64(indexStride)
                for (idx, line) in lines.enumerated() where (line.number - 1) % s == 0 {
                    checkpoints.append(offsets[idx])     // line 1 + k*stride
                }
            }
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
