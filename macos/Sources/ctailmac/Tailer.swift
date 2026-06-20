import Foundation

/// A single log line with its 1-based number.
struct LogLine {
    let number: Int64
    let text: String
}

/// Polling-based file tailer ported from internal/tailer/tailer.go.
///
/// Design choices carried over from the Go original:
///   - Polling (not kqueue/FSEvents) so slow/unreachable network mounts can't
///     wedge the UI — every I/O op runs off the main thread.
///   - Inode-change detection for log rotation (file renamed + recreated).
///   - Truncation detection when the file shrinks.
///   - Only *complete* lines (ending in \n) are committed; a trailing partial
///     line is left in place until the next poll reads it whole.
///   - Tail-first: for large files we seek near the end on first read instead
///     of paging the whole thing, so opening a 2 GB log is instant.
final class Tailer {
    private let path: String
    private let pollInterval: TimeInterval
    private let tailFirstThreshold: Int64 = 1 * 1024 * 1024   // 1 MB
    private let tailSeekBack: Int64 = 512 * 1024              // 512 KB

    private let queue = DispatchQueue(label: "net.biseth.ctail.tailer")
    private var timer: DispatchSourceTimer?

    // --- state (only touched on `queue`) ---
    private var lineNum: Int64 = 0
    private var offset: Int64 = 0
    private var fileSize: Int64 = 0
    private var inode: UInt64 = 0
    private var inError = false
    private var running = false

    // --- callbacks (invoked on the main queue) ---
    var onLines: (([LogLine]) -> Void)?
    var onReset: (() -> Void)?          // truncation or rotation: clear the view
    var onError: ((String) -> Void)?
    var onReady: (() -> Void)?

    init(path: String, pollInterval: TimeInterval = 0.25) {
        self.path = path
        self.pollInterval = max(0.05, pollInterval)
    }

    func start() {
        queue.async { [weak self] in
            guard let self, !self.running else { return }
            self.running = true
            self.initialRead()
            self.fire { self.onReady?() }

            let t = DispatchSource.makeTimerSource(queue: self.queue)
            t.schedule(deadline: .now() + self.pollInterval, repeating: self.pollInterval)
            t.setEventHandler { [weak self] in self?.poll() }
            self.timer = t
            t.resume()
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.timer?.cancel()
            self?.timer = nil
            self?.running = false
        }
    }

    // MARK: - Core loop

    private func initialRead() {
        guard let st = stat() else { return }
        inode = st.inode
        fileSize = st.size

        var start: Int64 = 0
        if st.size > tailFirstThreshold {
            start = alignToLineBoundary(max(0, st.size - tailSeekBack))
        }
        let (lines, consumed) = readNewLines(from: start, to: st.size)
        offset = consumed
        if !lines.isEmpty { fire { self.onLines?(lines) } }
    }

    private func poll() {
        guard let st = stat() else {
            if !inError { inError = true; fire { self.onError?("file unavailable: \(self.path)") } }
            return
        }
        let wasInError = inError
        inError = false

        // Rotation: same path, different inode -> old file rolled away.
        let rotated = inode != 0 && st.inode != inode
        if rotated { inode = st.inode }

        // Recovered from an error, or rotated, or shrank -> state may be stale.
        if rotated || st.size < fileSize {
            reset(newSize: st.size)
            let (lines, consumed) = readNewLines(from: 0, to: st.size)
            offset = consumed
            if !lines.isEmpty { fire { self.onLines?(lines) } }
            return
        }

        if wasInError { fire { self.onReady?() } }

        // Nothing new.
        if st.size == offset { return }

        let (lines, consumed) = readNewLines(from: offset, to: st.size)
        offset = consumed
        fileSize = st.size
        if !lines.isEmpty { fire { self.onLines?(lines) } }
    }

    private func reset(newSize: Int64) {
        lineNum = 0
        offset = 0
        fileSize = newSize
        fire { self.onReset?() }
    }

    // MARK: - I/O

    private func stat() -> (size: Int64, inode: UInt64)? {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: path) else { return nil }
        let size = (attrs[.size] as? NSNumber)?.int64Value ?? 0
        let inode = (attrs[.systemFileNumber] as? NSNumber)?.uint64Value ?? 0
        return (size, inode)
    }

    /// Reads bytes in [from, to) and splits into complete lines. Returns the
    /// lines and the byte offset just past the last complete line — any trailing
    /// partial line is intentionally left for the next poll.
    private func readNewLines(from: Int64, to: Int64) -> (lines: [LogLine], consumed: Int64) {
        guard to > from, let fh = FileHandle(forReadingAtPath: path) else { return ([], from) }
        defer { try? fh.close() }
        do {
            try fh.seek(toOffset: UInt64(from))
        } catch { return ([], from) }

        let data = (try? fh.read(upToCount: Int(to - from))) ?? Data()
        if data.isEmpty { return ([], from) }

        var lines: [LogLine] = []
        var consumed = from
        var lineStart = data.startIndex
        let newline = UInt8(ascii: "\n")

        var i = data.startIndex
        while i < data.endIndex {
            if data[i] == newline {
                var slice = data[lineStart..<i]          // excludes the \n
                if slice.last == UInt8(ascii: "\r") { slice = slice.dropLast() }
                lineNum += 1
                let text = String(decoding: slice, as: UTF8.self)
                lines.append(LogLine(number: lineNum, text: text))
                consumed = from + Int64(data.distance(from: data.startIndex, to: i)) + 1
                lineStart = data.index(after: i)
            }
            i = data.index(after: i)
        }
        return (lines, consumed)
    }

    /// Given a byte offset that may land mid-line, return the offset of the next
    /// line start so tail-first reads never begin on a fragment.
    private func alignToLineBoundary(_ start: Int64) -> Int64 {
        guard start > 0, let fh = FileHandle(forReadingAtPath: path) else { return start }
        defer { try? fh.close() }
        try? fh.seek(toOffset: UInt64(start))
        let chunk = (try? fh.read(upToCount: 64 * 1024)) ?? Data()
        if let nl = chunk.firstIndex(of: UInt8(ascii: "\n")) {
            return start + Int64(chunk.distance(from: chunk.startIndex, to: nl)) + 1
        }
        return start
    }

    private func fire(_ block: @escaping () -> Void) {
        DispatchQueue.main.async(execute: block)
    }
}
