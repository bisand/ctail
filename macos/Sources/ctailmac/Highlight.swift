import AppKit
import CtailCore

/// Renders a log line to an attributed string. Rule compilation and matching
/// happen in the engine (core/src/highlight.rs): the highest-priority matching
/// line-level rule paints the whole line, then match-level spans are painted in
/// ascending priority so higher priorities win. Colours and fonts are resolved
/// once per compiled rule, never per row.
struct HighlightEngine {
    private struct Style {
        let fg: NSColor?
        let bg: NSColor?
        let font: NSFont?
    }

    private let core: CoreHighlighter
    private let styles: [Style]
    let palette: ThemeColors
    let font: NSFont

    /// Disabled and invalid rules are skipped; order/priority is the engine's.
    init(rules: [Rule], palette: ThemeColors, font: NSFont) {
        self.palette = palette
        self.font = font
        core = CoreHighlighter(rules: rules)
        // `NSFontManager.convert` is costly and was previously called per bold
        // match, per visible row, on every reload (~4×/sec) — precompute.
        let fm = NSFontManager.shared
        let bold = fm.convert(font, toHaveTrait: .boldFontMask)
        let italic = fm.convert(font, toHaveTrait: .italicFontMask)
        let boldItalic = fm.convert(bold, toHaveTrait: .italicFontMask)
        styles = core.rules().map { r in
            Style(fg: r.foreground.isEmpty ? nil : Theme.hex(r.foreground),
                  bg: r.background.isEmpty ? nil : Theme.hex(r.background),
                  font: r.bold && r.italic ? boldItalic : r.bold ? bold : r.italic ? italic : nil)
        }
    }

    /// The compiled rules, in engine order.
    var rules: [Rule] { core.rules() }

    func render(_ line: String) -> NSAttributedString {
        let attr = NSMutableAttributedString(
            string: line,
            attributes: [.font: font, .foregroundColor: palette.foreground]
        )
        if styles.isEmpty { return attr }
        let style = core.apply(line: line)
        let full = NSRange(location: 0, length: (line as NSString).length)
        if style.lineRule >= 0 { paint(attr, styles[Int(style.lineRule)], full) }
        for span in style.spans {
            paint(attr, styles[Int(span.rule)], NSRange(location: Int(span.start), length: Int(span.end - span.start)))
        }
        return attr
    }

    private func paint(_ attr: NSMutableAttributedString, _ s: Style, _ range: NSRange) {
        if let fg = s.fg { attr.addAttribute(.foregroundColor, value: fg, range: range) }
        if let bg = s.bg { attr.addAttribute(.backgroundColor, value: bg, range: range) }
        if let f = s.font { attr.addAttribute(.font, value: f, range: range) }
    }
}
