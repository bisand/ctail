import AppKit

/// A subset of ctail's theme model (internal/config/themes.go). Real app has 21;
/// the POC ships the default — Catppuccin Mocha — to match the look and feel.
struct Theme {
    let background: NSColor
    let backgroundAlt: NSColor   // zebra-striped alternate rows
    let foreground: NSColor
    let selection: NSColor
    let gutter: NSColor          // line-number column text

    static let catppuccinMocha = Theme(
        background:    hex("#1e1e2e"),
        backgroundAlt: hex("#181825"),
        foreground:    hex("#cdd6f4"),
        selection:     hex("#45475a"),
        gutter:        hex("#6c7086")
    )

    static func hex(_ s: String) -> NSColor {
        var h = s
        if h.hasPrefix("#") { h.removeFirst() }
        var v: UInt64 = 0
        Scanner(string: h).scanHexInt64(&v)
        return NSColor(
            srgbRed: CGFloat((v >> 16) & 0xff) / 255,
            green:   CGFloat((v >> 8) & 0xff) / 255,
            blue:    CGFloat(v & 0xff) / 255,
            alpha: 1
        )
    }
}
