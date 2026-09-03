import AppKit
import CtailCore

// `Theme` and `ThemeColors` are engine records (hex strings, Go-compatible JSON
// keys); the catalogue and custom-theme loading live in core/src/themes.rs.
// This file layers the AppKit conveniences on top.

extension ThemeColors {
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

extension Theme {
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
    static var builtIns: [Theme] { builtInThemes() }

    static func all(custom dir: URL? = nil) -> [Theme] { allThemes(customDir: dir?.path) }

    /// Resolves a theme name + mode to a concrete palette, falling back to the
    /// default (Catppuccin dark) if the name is unknown.
    static func palette(name: String, mode: String, custom dir: URL? = nil) -> ThemeColors {
        resolvePalette(name: name, mode: mode, customDir: dir?.path)
    }
}
