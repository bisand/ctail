import AppKit

/// A full theme palette — mirrors config.ThemeColors in themes.go. Stored as hex
/// strings (Codable with the same JSON keys as the Go app, so custom theme files
/// drop in unchanged) with NSColor accessors layered on top.
struct ThemeColors: Codable, Equatable {
    var bgPrimary, bgSecondary, bgSurface, bgHover: String
    var textPrimary, textSecondary, textMuted: String
    var accent, accentHover, border, danger, success, warning: String
    var tabActive, tabInactive, badgeColor, scrollbarTrack, scrollbarThumb: String

    enum CodingKeys: String, CodingKey {
        case bgPrimary = "bg-primary", bgSecondary = "bg-secondary", bgSurface = "bg-surface"
        case bgHover = "bg-hover", textPrimary = "text-primary", textSecondary = "text-secondary"
        case textMuted = "text-muted", accent, accentHover = "accent-hover", border
        case danger, success, warning, tabActive = "tab-active", tabInactive = "tab-inactive"
        case badgeColor = "badge-color", scrollbarTrack = "scrollbar-track", scrollbarThumb = "scrollbar-thumb"
    }

    init(bgPrimary: String, bgSecondary: String, bgSurface: String, bgHover: String,
         textPrimary: String, textSecondary: String, textMuted: String, accent: String,
         accentHover: String, border: String, danger: String, success: String, warning: String,
         tabActive: String, tabInactive: String, badgeColor: String,
         scrollbarTrack: String, scrollbarThumb: String) {
        self.bgPrimary = bgPrimary; self.bgSecondary = bgSecondary; self.bgSurface = bgSurface
        self.bgHover = bgHover; self.textPrimary = textPrimary; self.textSecondary = textSecondary
        self.textMuted = textMuted; self.accent = accent; self.accentHover = accentHover
        self.border = border; self.danger = danger; self.success = success; self.warning = warning
        self.tabActive = tabActive; self.tabInactive = tabInactive; self.badgeColor = badgeColor
        self.scrollbarTrack = scrollbarTrack; self.scrollbarThumb = scrollbarThumb
    }

    /// Lenient decode so a custom theme JSON can override only some keys; any
    /// omitted color defaults to a neutral so it never fails the whole load.
    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        func s(_ k: CodingKeys) -> String { (try? c.decodeIfPresent(String.self, forKey: k)) ?? nil ?? "#808080" }
        bgPrimary = s(.bgPrimary); bgSecondary = s(.bgSecondary); bgSurface = s(.bgSurface)
        bgHover = s(.bgHover); textPrimary = s(.textPrimary); textSecondary = s(.textSecondary)
        textMuted = s(.textMuted); accent = s(.accent); accentHover = s(.accentHover)
        border = s(.border); danger = s(.danger); success = s(.success); warning = s(.warning)
        tabActive = s(.tabActive); tabInactive = s(.tabInactive); badgeColor = s(.badgeColor)
        scrollbarTrack = s(.scrollbarTrack); scrollbarThumb = s(.scrollbarThumb)
    }

    /// Neutral grey palette used only as a pre-launch placeholder.
    static let placeholder = ThemeColors(
        bgPrimary: "#1e1e1e", bgSecondary: "#181818", bgSurface: "#2a2a2a", bgHover: "#3a3a3a",
        textPrimary: "#e0e0e0", textSecondary: "#b0b0b0", textMuted: "#808080", accent: "#6bcbff",
        accentHover: "#5bb0e0", border: "#3a3a3a", danger: "#ff6b6b", success: "#a6e3a1",
        warning: "#f9e2af", tabActive: "#1e1e1e", tabInactive: "#181818", badgeColor: "#f9e2af",
        scrollbarTrack: "#181818", scrollbarThumb: "#3a3a3a")

    // Convenience NSColors used by the log surface and chrome.
    var background: NSColor { Theme.hex(bgPrimary) }
    var backgroundAlt: NSColor { Theme.hex(bgSecondary) }
    var surface: NSColor { Theme.hex(bgSurface) }
    var hover: NSColor { Theme.hex(bgHover) }
    var foreground: NSColor { Theme.hex(textPrimary) }
    var muted: NSColor { Theme.hex(textMuted) }
    var gutter: NSColor { Theme.hex(textMuted) }
    var selection: NSColor { Theme.hex(bgHover) }
    var accentColor: NSColor { Theme.hex(accent) }
    var borderColor: NSColor { Theme.hex(border) }
    var dangerColor: NSColor { Theme.hex(danger) }
    var successColor: NSColor { Theme.hex(success) }
    var warningColor: NSColor { Theme.hex(warning) }
    var badge: NSColor { Theme.hex(badgeColor) }
}

/// A named theme with dark and light variants.
struct Theme: Codable, Equatable {
    var name: String
    var displayName: String
    var dark: ThemeColors
    var light: ThemeColors
    var builtIn: Bool = true

    enum CodingKeys: String, CodingKey { case name, displayName, dark, light, builtIn }

    init(name: String, displayName: String, dark: ThemeColors, light: ThemeColors, builtIn: Bool = true) {
        self.name = name; self.displayName = displayName
        self.dark = dark; self.light = light; self.builtIn = builtIn
    }

    init(from d: Decoder) throws {
        let c = try d.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        displayName = try c.decodeIfPresent(String.self, forKey: .displayName) ?? name
        dark = try c.decode(ThemeColors.self, forKey: .dark)
        light = try c.decodeIfPresent(ThemeColors.self, forKey: .light) ?? dark
        builtIn = try c.decodeIfPresent(Bool.self, forKey: .builtIn) ?? false
    }

    func palette(mode: String) -> ThemeColors { mode == "light" ? light : dark }

    /// NSColor -> "#rrggbb".
    static func hexString(_ color: NSColor) -> String {
        let c = color.usingColorSpace(.sRGB) ?? color
        return String(format: "#%02x%02x%02x",
                      Int((c.redComponent * 255).rounded()),
                      Int((c.greenComponent * 255).rounded()),
                      Int((c.blueComponent * 255).rounded()))
    }

    static func hex(_ s: String) -> NSColor {
        var h = s.trimmingCharacters(in: .whitespaces)
        if h.hasPrefix("#") { h.removeFirst() }
        if h.count == 3 { h = h.map { "\($0)\($0)" }.joined() }   // #abc -> #aabbcc
        var v: UInt64 = 0
        Scanner(string: h).scanHexInt64(&v)
        return NSColor(srgbRed: CGFloat((v >> 16) & 0xff) / 255,
                       green:   CGFloat((v >> 8) & 0xff) / 255,
                       blue:    CGFloat(v & 0xff) / 255, alpha: 1)
    }
}

/// All themes (21 built-ins + user-supplied custom themes from the config dir).
enum ThemeCatalog {
    // `builtIns` is provided by Themes.generated.swift.

    static func all(custom dir: URL? = nil) -> [Theme] {
        var themes = builtIns
        if let dir, let urls = try? FileManager.default.contentsOfDirectory(
            at: dir, includingPropertiesForKeys: nil) {
            for url in urls where url.pathExtension == "json" {
                if let data = try? Data(contentsOf: url),
                   var t = try? JSONDecoder().decode(Theme.self, from: data) {
                    t.builtIn = false
                    themes.removeAll { $0.name == t.name }   // custom overrides built-in
                    themes.append(t)
                }
            }
        }
        return themes
    }

    /// Resolves a theme name + mode to a concrete palette, falling back to the
    /// default (Catppuccin dark) if the name is unknown.
    static func palette(name: String, mode: String, custom dir: URL? = nil) -> ThemeColors {
        let themes = all(custom: dir)
        let theme = themes.first { $0.name == name } ?? themes.first { $0.name == "catppuccin" } ?? builtIns[0]
        return theme.palette(mode: mode)
    }
}
