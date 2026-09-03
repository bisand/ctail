import Foundation
import CtailCore

/// A compiled search (case-sensitive, whole-word and regex toggles), backed by
/// the engine's matcher (core/src/search.rs). Plain queries are escaped into a
/// regex so match ranges and boolean matching share one code path.
struct SearchQuery {
    private let core: CoreSearchMatcher
    let isEmpty: Bool

    init(_ text: String, caseSensitive: Bool, wholeWord: Bool, isRegex: Bool) {
        isEmpty = text.isEmpty
        core = CoreSearchMatcher(text: text, caseSensitive: caseSensitive, wholeWord: wholeWord, isRegex: isRegex)
    }

    var isValid: Bool { core.isValid() }

    func matches(_ s: String) -> Bool { core.matches(line: s) }

    func ranges(in s: String) -> [NSRange] {
        core.ranges(line: s).map { NSRange(location: Int($0.start), length: Int($0.end - $0.start)) }
    }

    /// Indices of the matching lines — one engine call for a whole window.
    func matchingIndices(_ lines: [String]) -> [Int] {
        isEmpty ? [] : core.matchingIndices(lines: lines).map { Int($0) }
    }
}
