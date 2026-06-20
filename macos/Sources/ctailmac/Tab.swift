import AppKit

/// One open file: its own tailer + log view + per-tab metadata. Mirrors the
/// per-tab model in app.go / stores/tabs.js.
final class Tab {
    let id = UUID().uuidString
    let filePath: String
    var label: String          // user-overridable; empty -> derived from filename
    var color: String          // hex tab color, "" for none
    var profileName: String
    let tailer: Tailer
    let logView: LogView

    init(filePath: String, palette: ThemeColors, rules: [HighlightRule],
         profileName: String, pollInterval: TimeInterval, readTimeout: TimeInterval,
         fontSize: CGFloat, showLineNumbers: Bool) {
        self.filePath = filePath
        self.label = ""
        self.color = ""
        self.profileName = profileName
        self.logView = LogView(palette: palette, rules: rules,
                               fontSize: fontSize, showLineNumbers: showLineNumbers)
        self.tailer = Tailer(path: filePath, pollInterval: pollInterval, readTimeout: readTimeout)
    }

    var displayName: String {
        label.isEmpty ? (filePath as NSString).lastPathComponent : label
    }

    func toState(position: Int) -> TabState {
        TabState(filePath: filePath, profileId: profileName, autoScroll: logView.following,
                 label: label, color: color, position: position)
    }
}
