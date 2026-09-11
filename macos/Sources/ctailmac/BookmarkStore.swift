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
    ///
    /// The bookmark asks for read access only. The app is entitled to read what
    /// the user picks and nothing more, and a read-write scope is refused under
    /// that entitlement — which, swallowed, left every file unreadable after a
    /// relaunch: the tab came back, its contents did not.
    func save(_ url: URL) {
        do {
            let data = try url.bookmarkData(options: [.withSecurityScope, .securityScopeAllowOnlyReadAccess],
                                            includingResourceValuesForKeys: nil, relativeTo: nil)
            map[url.path] = data.base64EncodedString()
            persist()
        } catch {
            NSLog("ctail: cannot keep access to %@ across launches: %@", url.path, error.localizedDescription)
        }
    }

    /// Resolves + starts accessing the bookmark for `path`. Returns false if no
    /// bookmark exists (caller proceeds; unsandboxed builds can read anyway).
    @discardableResult
    func beginAccess(_ path: String) -> Bool {
        // Already accessing: don't resolve/start a second time — that would
        // overwrite the stored URL and leak the first scoped-access claim.
        if active[path] != nil { return true }
        guard let b64 = map[path], let data = Data(base64Encoded: b64) else { return false }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, options: .withSecurityScope,
                                 relativeTo: nil, bookmarkDataIsStale: &stale) else { return false }
        let ok = url.startAccessingSecurityScopedResource()
        if ok { active[path] = url }
        // A stale bookmark is renewed from the resolved URL, which can only be
        // bookmarked again while it is being accessed.
        if stale { save(url) }
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
