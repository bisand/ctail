import AppKit

/// Native file plumbing, mirroring app.go's RevealInFileManager / clipboard
/// helpers. File associations (.log/.txt/.csv) are declared in Info.plist and
/// routed through AppDelegate.application(_:open:).
enum FileOps {
    /// Reveal a file in Finder (selects it in its containing folder).
    static func revealInFinder(_ path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    /// Copy a file path to the clipboard.
    static func copyPath(_ path: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(path, forType: .string)
    }

    /// Copy arbitrary text (e.g. selected log lines) to the clipboard.
    static func copyText(_ text: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
    }
}
