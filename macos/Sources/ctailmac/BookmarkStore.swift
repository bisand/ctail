import Foundation

/// Security-scoped bookmark persistence (issue #2) — the key to App Store
/// distribution. A sandboxed app can only keep access to a user-selected file
/// across launches via a security-scoped bookmark. When the user opens a file
/// (NSOpenPanel or Finder), we store a bookmark; before tailing we resolve it
/// and call startAccessingSecurityScopedResource.
///
/// Outside the sandbox (e.g. a direct-download / dev build) bookmark creation
/// may be unavailable; everything is best-effort so file access still works.
final class BookmarkStore {
    private let file: URL
    private var map: [String: String]            // path -> base64 bookmark data
    private var active: [String: URL] = [:]      // paths currently being accessed

    init(dir: URL) {
        file = dir.appendingPathComponent("bookmarks.json")
        map = (try? JSONDecoder().decode([String: String].self, from: Data(contentsOf: file))) ?? [:]
    }

    /// Records a security-scoped bookmark for a user-granted URL.
    func save(_ url: URL) {
        guard let data = try? url.bookmarkData(options: .withSecurityScope,
                                               includingResourceValuesForKeys: nil, relativeTo: nil)
        else { return }
        map[url.path] = data.base64EncodedString()
        persist()
    }

    /// Resolves + starts accessing the bookmark for `path`. Returns false if no
    /// bookmark exists (caller proceeds; unsandboxed builds can read anyway).
    @discardableResult
    func beginAccess(_ path: String) -> Bool {
        guard let b64 = map[path], let data = Data(base64Encoded: b64) else { return false }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, options: .withSecurityScope,
                                 relativeTo: nil, bookmarkDataIsStale: &stale) else { return false }
        if stale { save(url) }
        let ok = url.startAccessingSecurityScopedResource()
        if ok { active[path] = url }
        return ok
    }

    func endAccess(_ path: String) {
        active[path]?.stopAccessingSecurityScopedResource()
        active[path] = nil
    }

    /// Whether a persisted bookmark exists for a path (used to decide if a saved
    /// tab can be restored under the sandbox).
    func hasBookmark(_ path: String) -> Bool { map[path] != nil }

    private func persist() {
        try? JSONEncoder().encode(map).write(to: file, options: .atomic)
    }
}
