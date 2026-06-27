import Foundation

// Codable models mirroring internal/config/types.go. JSON keys are kept
// identical so settings/profile files round-trip with the Go app. All decoders
// use decodeIfPresent + defaults so missing/extra keys never fail a load
// (matching the Go app's lenient parsing).

/// A single highlighting rule.
struct Rule: Codable, Equatable {
    var id: String = ""
    var name: String = ""
    var pattern: String = ""
    var matchType: String = "match"   // "line" or "match"
    var foreground: String = ""
    var background: String = ""
    var bold: Bool = false
    var italic: Bool = false
    var enabled: Bool = true
    var priority: Int = 0

    init(id: String = "", name: String = "", pattern: String = "", matchType: String = "match",
         foreground: String = "", background: String = "", bold: Bool = false, italic: Bool = false,
         enabled: Bool = true, priority: Int = 0) {
        self.id = id; self.name = name; self.pattern = pattern; self.matchType = matchType
        self.foreground = foreground; self.background = background; self.bold = bold
        self.italic = italic; self.enabled = enabled; self.priority = priority
    }

    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(String.self, forKey: .id) ?? ""
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        pattern = try c.decodeIfPresent(String.self, forKey: .pattern) ?? ""
        matchType = try c.decodeIfPresent(String.self, forKey: .matchType) ?? "match"
        foreground = try c.decodeIfPresent(String.self, forKey: .foreground) ?? ""
        background = try c.decodeIfPresent(String.self, forKey: .background) ?? ""
        bold = try c.decodeIfPresent(Bool.self, forKey: .bold) ?? false
        italic = try c.decodeIfPresent(Bool.self, forKey: .italic) ?? false
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        priority = try c.decodeIfPresent(Int.self, forKey: .priority) ?? 0
    }
}

/// A named set of highlighting rules.
struct Profile: Codable, Equatable {
    var name: String = ""
    var rules: [Rule] = []
}

/// Per-tab persisted state.
struct TabState: Codable, Equatable {
    var filePath: String = ""
    var profileId: String = ""
    var autoScroll: Bool = true
    var label: String = ""
    var color: String = ""
    var position: Int = 0

    init(filePath: String = "", profileId: String = "", autoScroll: Bool = true,
         label: String = "", color: String = "", position: Int = 0) {
        self.filePath = filePath; self.profileId = profileId; self.autoScroll = autoScroll
        self.label = label; self.color = color; self.position = position
    }

    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        filePath = try c.decodeIfPresent(String.self, forKey: .filePath) ?? ""
        profileId = try c.decodeIfPresent(String.self, forKey: .profileId) ?? ""
        autoScroll = try c.decodeIfPresent(Bool.self, forKey: .autoScroll) ?? true
        label = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        color = try c.decodeIfPresent(String.self, forKey: .color) ?? ""
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
    }
}

/// Window geometry.
struct WindowState: Codable, Equatable {
    var x: Int = 0
    var y: Int = 0
    var width: Int = 1200
    var height: Int = 800
    var maximised: Bool = false

    init(x: Int = 0, y: Int = 0, width: Int = 1200, height: Int = 800, maximised: Bool = false) {
        self.x = x; self.y = y; self.width = width; self.height = height; self.maximised = maximised
    }

    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        x = try c.decodeIfPresent(Int.self, forKey: .x) ?? 0
        y = try c.decodeIfPresent(Int.self, forKey: .y) ?? 0
        width = try c.decodeIfPresent(Int.self, forKey: .width) ?? 1200
        height = try c.decodeIfPresent(Int.self, forKey: .height) ?? 800
        maximised = try c.decodeIfPresent(Bool.self, forKey: .maximised) ?? false
    }
}

/// Global application settings (superset; Linux-only keys kept for round-trip).
struct AppSettings: Codable, Equatable {
    var pollIntervalMs: Int = 500
    var bufferSize: Int = 10000
    var scrollBuffer: Int = 500
    var scrollSpeed: Int = 1
    var smoothScroll: Bool = false
    var theme: String = "catppuccin"
    var themeMode: String = "dark"
    var fontSize: Int = 14
    var showLineNumbers: Bool = false
    var wordWrap: Bool = false
    var restoreTabs: Bool = true
    var newTabPosition: String = "end"
    var lastActiveTabPath: String = ""
    var activeProfile: String = "Common Logs"
    var tabs: [TabState] = []
    var recentFiles: [String] = []
    var window: WindowState = WindowState()
    var displayBackend: String = "auto"
    var disableDmabuf: Bool = false
    var gpuPolicy: String = ""
    var readTimeoutSec: Int = 30
    var disableUpdateCheck: Bool = false
    var updateCheckIntervalHours: Int = 24
    var aiProvider: String = ""
    var aiEndpoint: String = ""
    var aiKey: String = ""
    var aiModel: String = ""

    init() {}

    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        func s(_ k: CodingKeys, _ def: String) throws -> String { try c.decodeIfPresent(String.self, forKey: k) ?? def }
        func i(_ k: CodingKeys, _ def: Int) throws -> Int { try c.decodeIfPresent(Int.self, forKey: k) ?? def }
        func b(_ k: CodingKeys, _ def: Bool) throws -> Bool { try c.decodeIfPresent(Bool.self, forKey: k) ?? def }
        pollIntervalMs = try i(.pollIntervalMs, 500)
        bufferSize = try i(.bufferSize, 10000)
        scrollBuffer = try i(.scrollBuffer, 500)
        scrollSpeed = try i(.scrollSpeed, 1)
        smoothScroll = try b(.smoothScroll, false)
        theme = try s(.theme, "catppuccin")
        themeMode = try s(.themeMode, "dark")
        fontSize = try i(.fontSize, 14)
        showLineNumbers = try b(.showLineNumbers, false)
        wordWrap = try b(.wordWrap, false)
        restoreTabs = try b(.restoreTabs, true)
        newTabPosition = try s(.newTabPosition, "end")
        lastActiveTabPath = try s(.lastActiveTabPath, "")
        activeProfile = try s(.activeProfile, "Common Logs")
        tabs = try c.decodeIfPresent([TabState].self, forKey: .tabs) ?? []
        recentFiles = try c.decodeIfPresent([String].self, forKey: .recentFiles) ?? []
        window = try c.decodeIfPresent(WindowState.self, forKey: .window) ?? WindowState()
        displayBackend = try s(.displayBackend, "auto")
        disableDmabuf = try b(.disableDmabuf, false)
        gpuPolicy = try s(.gpuPolicy, "")
        readTimeoutSec = try i(.readTimeoutSec, 30)
        disableUpdateCheck = try b(.disableUpdateCheck, false)
        updateCheckIntervalHours = try i(.updateCheckIntervalHours, 24)
        aiProvider = try s(.aiProvider, "")
        aiEndpoint = try s(.aiEndpoint, "")
        aiKey = try s(.aiKey, "")
        aiModel = try s(.aiModel, "")
    }
}

enum Defaults {
    /// The built-in "Common Logs" profile (matches config.DefaultProfile in Go).
    static func commonLogsProfile() -> Profile {
        Profile(name: "Common Logs", rules: [
            Rule(id: "error", name: "Error", pattern: #"(?i)\bERROR\b"#, matchType: "line",
                 foreground: "#ff6b6b", background: "#3d1f1f", bold: true, enabled: true, priority: 100),
            Rule(id: "fatal", name: "Fatal", pattern: #"(?i)\bFATAL\b"#, matchType: "line",
                 foreground: "#ffffff", background: "#cc0000", bold: true, enabled: true, priority: 110),
            Rule(id: "warn", name: "Warning", pattern: #"(?i)\bWARN(ING)?\b"#, matchType: "line",
                 foreground: "#ffd93d", background: "#3d3520", enabled: true, priority: 90),
            Rule(id: "info", name: "Info", pattern: #"(?i)\bINFO?\b"#, matchType: "match",
                 foreground: "#6bcbff", enabled: true, priority: 50),
            Rule(id: "debug", name: "Debug", pattern: #"(?i)\bDEBUG\b"#, matchType: "match",
                 foreground: "#888888", enabled: true, priority: 40),
            Rule(id: "timestamp", name: "Timestamp", pattern: #"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}"#,
                 matchType: "match", foreground: "#88cc88", enabled: true, priority: 30),
        ])
    }
}
