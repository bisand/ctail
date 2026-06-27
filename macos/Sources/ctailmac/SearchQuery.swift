import Foundation

/// A compiled search, mirroring the SearchTab options in app.go: case-sensitive,
/// whole-word, and regex toggles. Plain queries are escaped into a regex so match
/// ranges (for highlighting) and boolean matching share one code path.
struct SearchQuery {
    let regex: NSRegularExpression?
    let isEmpty: Bool

    init(_ text: String, caseSensitive: Bool, wholeWord: Bool, isRegex: Bool) {
        isEmpty = text.isEmpty
        if text.isEmpty { regex = nil; return }
        var pattern = isRegex ? text : NSRegularExpression.escapedPattern(for: text)
        if wholeWord { pattern = "\\b" + pattern + "\\b" }
        let opts: NSRegularExpression.Options = caseSensitive ? [] : [.caseInsensitive]
        regex = try? NSRegularExpression(pattern: pattern, options: opts)
    }

    var isValid: Bool { isEmpty || regex != nil }

    func matches(_ s: String) -> Bool {
        guard let regex else { return false }
        return regex.firstMatch(in: s, range: NSRange(location: 0, length: (s as NSString).length)) != nil
    }

    func ranges(in s: String) -> [NSRange] {
        guard let regex else { return [] }
        let full = NSRange(location: 0, length: (s as NSString).length)
        return regex.matches(in: s, range: full).map { $0.range }
    }
}
