// Swift twin of core/examples/tailbench.rs: drives a `Tailer` on a file and
// prints the same measurements. Compiles against either LegacyTailer.swift (the
// old Swift engine) or Sources/ctailmac/Tailer.swift (the Rust engine through
// UniFFI); see "Engine benchmark" in macos/README.md for both command lines.
//
// Generate the file with the Rust harness (`--gen SIZE [--cold] --gen-only`) so
// every engine sees byte-identical input.
import Foundation

var args = Array(CommandLine.arguments.dropFirst())
var file = NSTemporaryDirectory() + "ctail-bench.log"
var page = 10_000
var i = 0
while i < args.count {
    switch args[i] {
    case "--file": file = args[i + 1]; i += 1
    case "--page": page = Int(args[i + 1])!; i += 1
    default: fatalError("unknown arg \(args[i])")
    }
    i += 1
}
let size = (try? FileManager.default.attributesOfItem(atPath: file)[.size] as? Int64) ?? 0
func human(_ b: Int64) -> String {
    b >= 1 << 30 ? String(format: "%.2f GB", Double(b) / Double(1 << 30)) : String(format: "%.1f MB", Double(b) / Double(1 << 20))
}
print("engine=swift file=\(file) size=\(human(size))")

let t0 = Date()
var firstLines: (Double, Int)? = nil
var baseAt: (Double, Int64)? = nil
var ready = false
let tailer = Tailer(path: file)
tailer.onLines = { lines in if firstLines == nil { firstLines = (Date().timeIntervalSince(t0) * 1000, lines.count) } }
tailer.onBaseResolved = { base in baseAt = (Date().timeIntervalSince(t0) * 1000, base) }
tailer.onReady = { ready = true }
tailer.start()

let deadline = Date().addingTimeInterval(600)
while Date() < deadline && !(ready && tailer.indexingComplete) {
    RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.005))
}
let total = tailer.totalLines
print(String(format: "first_lines_ms=%.2f first_batch=%d", firstLines?.0 ?? 0, firstLines?.1 ?? 0))
if let (ms, base) = baseAt {
    print(String(format: "index_ms=%.1f base=%lld total_lines=%lld (%.2f GB/s)", ms, base, total,
                 Double(size) / Double(1 << 30) / (ms / 1000)))
} else {
    print("index_ms=0 (small file) total_lines=\(total)")
}

for (label, start) in [("head", Int64(1)), ("middle", total / 2), ("tail", max(1, total - Int64(page)))] {
    let t = Date()
    var got: [LogLine]? = nil
    tailer.fetchRange(start: start, count: page) { got = $0 }
    while got == nil { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.001)) }
    print(String(format: "page_%@_ms=%.2f lines=%d first=%lld", label, Date().timeIntervalSince(t) * 1000,
                 got!.count, got!.first?.number ?? 0))
}
var ru = rusage()
getrusage(RUSAGE_SELF, &ru)
print(String(format: "peak_rss_mb=%.1f", Double(ru.ru_maxrss) / Double(1 << 20)))
