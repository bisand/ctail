import Foundation
import CtailCore

/// Log lines come straight from the engine's record type.
typealias LogLine = CtailCore.LogLine

/// Swift face of the Rust tail engine (`core/src/tailer.rs`, exposed through
/// UniFFI as `TailerHandle`). Keeps the surface the UI has always used:
/// closure callbacks delivered on the main queue, plus `fetchRange` for the
/// log view's sliding window.
///
/// Engine semantics (polling, inode rotation + truncation detection, instant
/// tail-first opens with a background line index, I/O timeouts for dead
/// mounts) live in Rust; see the crate docs. The previous Swift engine is kept
/// at `scripts/tailbench/LegacyTailer.swift` as the benchmark reference.
final class Tailer {
    // --- callbacks (invoked on the main queue) ---
    var onLines: (([LogLine]) -> Void)?
    var onReset: (() -> Void)?              // truncation or rotation: clear the view
    var onError: ((String) -> Void)?
    var onReady: (() -> Void)?
    var onBaseResolved: ((Int64) -> Void)?  // background count done; arg = base (lines before tail)

    private let handle: TailerHandle
    private let listener = Listener()

    init(path: String, pollInterval: TimeInterval = 0.25, readTimeout: TimeInterval = 30,
         tailFirstThreshold: Int64 = 1 * 1024 * 1024, tailSeekBack: Int64 = 512 * 1024,
         maxReadChunk: Int64 = 4 * 1024 * 1024) {
        let defaults = defaultTailerOptions()
        let options = TailerOptions(pollInterval: pollInterval, readTimeout: readTimeout,
                                    tailFirstThreshold: tailFirstThreshold, tailSeekBack: tailSeekBack,
                                    maxReadChunk: maxReadChunk, indexStride: defaults.indexStride)
        handle = TailerHandle(path: path, options: options, listener: listener)
        listener.owner = self
    }

    /// Total lines known so far (grows as the file is tailed; absolute once based).
    var totalLines: Int64 { handle.totalLines() }
    /// Whether absolute line numbers / scrollback are available yet.
    var indexingComplete: Bool { handle.indexingComplete() }

    /// Shows + follows the tail at once; large files count their head in the background.
    func start() { handle.start() }
    /// Pauses polling; `start` resumes it.
    func stop() { handle.stop() }
    /// Manual refresh: discard state and re-read from scratch.
    func refresh() { handle.refresh() }
    /// Adjusts the poll cadence at runtime (slow inactive/backgrounded tabs).
    func setPollInterval(_ interval: TimeInterval) { handle.setPollInterval(interval: interval) }

    /// Reads `count` lines from 1-based absolute `start` off disk; `completion`
    /// runs on the main queue. Empty until indexing is complete.
    func fetchRange(start: Int64, count: Int, completion: @escaping ([LogLine]) -> Void) {
        handle.fetchRange(start: start, count: UInt32(clamping: max(0, count)), reply: Reply(completion))
    }
}

/// Engine -> main-queue trampoline. Holds its owner weakly so a tab that is
/// closing can't be resurrected by an in-flight callback.
private final class Listener: TailerListener, @unchecked Sendable {
    weak var owner: Tailer?
    func onLines(lines: [LogLine]) { DispatchQueue.main.async { [weak owner] in owner?.onLines?(lines) } }
    func onReset() { DispatchQueue.main.async { [weak owner] in owner?.onReset?() } }
    func onError(message: String) { DispatchQueue.main.async { [weak owner] in owner?.onError?(message) } }
    func onReady() { DispatchQueue.main.async { [weak owner] in owner?.onReady?() } }
    func onBaseResolved(base: Int64) { DispatchQueue.main.async { [weak owner] in owner?.onBaseResolved?(base) } }
}

private final class Reply: FetchReply, @unchecked Sendable {
    private let completion: ([LogLine]) -> Void
    init(_ completion: @escaping ([LogLine]) -> Void) { self.completion = completion }
    func deliver(lines: [LogLine]) { DispatchQueue.main.async { [completion] in completion(lines) } }
}
