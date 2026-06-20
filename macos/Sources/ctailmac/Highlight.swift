import AppKit

/// A highlighting rule, mirroring internal/rules/engine.go. A rule matches a
/// regex and applies colors/weight either to the whole line (`lineLevel`) or to
/// just the matched substrings.
struct HighlightRule {
    let regex: NSRegularExpression
    let fg: NSColor?
    let bg: NSColor?
    let bold: Bool
    let lineLevel: Bool

    init?(pattern: String, fg: NSColor? = nil, bg: NSColor? = nil, bold: Bool = false, lineLevel: Bool = false) {
        guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return nil }
        self.regex = re
        self.fg = fg; self.bg = bg; self.bold = bold; self.lineLevel = lineLevel
    }
}

/// Renders a log line to an attributed string by applying the first matching
/// line-level rule, then layering all match-level rules on top.
struct HighlightEngine {
    var rules: [HighlightRule]
    let theme: Theme
    let font: NSFont

    func render(_ line: String) -> NSAttributedString {
        let attr = NSMutableAttributedString(
            string: line,
            attributes: [.font: font, .foregroundColor: theme.foreground]
        )
        let full = NSRange(location: 0, length: (line as NSString).length)

        // Line-level: first match wins, paints the entire line.
        for rule in rules where rule.lineLevel {
            if rule.regex.firstMatch(in: line, range: full) != nil {
                if let fg = rule.fg { attr.addAttribute(.foregroundColor, value: fg, range: full) }
                if let bg = rule.bg { attr.addAttribute(.backgroundColor, value: bg, range: full) }
                if rule.bold { attr.addAttribute(.font, value: bold(font), range: full) }
                break
            }
        }

        // Match-level: paint each matched span.
        for rule in rules where !rule.lineLevel {
            rule.regex.enumerateMatches(in: line, range: full) { m, _, _ in
                guard let r = m?.range else { return }
                if let fg = rule.fg { attr.addAttribute(.foregroundColor, value: fg, range: r) }
                if let bg = rule.bg { attr.addAttribute(.backgroundColor, value: bg, range: r) }
                if rule.bold { attr.addAttribute(.font, value: bold(font), range: r) }
            }
        }
        return attr
    }

    private func bold(_ f: NSFont) -> NSFont {
        NSFontManager.shared.convert(f, toHaveTrait: .boldFontMask)
    }
}
