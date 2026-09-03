import Foundation
import CtailCore

/// Swift face of the engine's whole-file search (`core/src/filesearch.rs`,
/// exposed through UniFFI as `CoreFileSearch`).
///
/// A find bar can only match what its view holds, and a view of a tailed file
/// holds its last few thousand lines. This scans what is on disk, so the
/// counter is the truth about the file and ↓ can reach a match a million lines
/// above anything loaded. The engine does the waiting (a scan starts once the
/// typing pauses), the cancelling (the next query calls the last scan off) and
/// the stepping; the count and the match list stay there — ask with `status`
/// and `step`. `onResult` is delivered on the main queue.
final class FileSearch {
    var onResult: ((FileSearchQuery, UInt32) -> Void)?

    private let handle: CoreFileSearch
    private let listener = Listener()

    init() {
        handle = CoreFileSearch(listener: listener)
        listener.owner = self
    }

    /// Safe to call on every keystroke: a query already answered or under way
    /// is not scanned for twice.
    func request(_ query: FileSearchQuery) { handle.request(query: query) }
    /// Forgets everything: the bar is closed, or has nothing usable in it.
    func clear() { handle.clear() }
    /// Idle for any query but the one the engine has an answer to or is
    /// scanning for, so a stale count is never shown against the wrong query.
    func status(_ query: FileSearchQuery) -> FileSearchStatus { handle.status(query: query) }
    func matches(_ query: FileSearchQuery) -> [Int64] { handle.matches(query: query) }
    /// The next or previous match's line number, wrapping at the ends. `from`
    /// is the line the view is showing: the first step goes to the match
    /// nearest it rather than to the top of the file.
    func step(_ query: FileSearchQuery, forward: Bool, from: Int64?) -> Int64? {
        handle.step(query: query, forward: forward, from: from)
    }
}

/// Engine -> main-queue trampoline, holding its owner weakly so a closed
/// controller is not resurrected by a scan that finished late.
private final class Listener: FileSearchListener, @unchecked Sendable {
    weak var owner: FileSearch?
    func onResult(query: FileSearchQuery, total: UInt32) {
        DispatchQueue.main.async { [weak owner] in owner?.onResult?(query, total) }
    }
}
