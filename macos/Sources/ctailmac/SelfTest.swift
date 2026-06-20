import Foundation

// Lightweight in-process test harness. XCTest/Testing aren't available under the
// Command Line Tools toolchain (they ship with full Xcode), so tests run via
// `ctailmac --selftest` and report a pass/fail summary. The check() helpers
// mirror XCTAssert* closely enough to migrate later with a find/replace.
enum SelfTest {
    nonisolated(unsafe) static var failures = 0
    nonisolated(unsafe) static var checks = 0

    static func check(_ cond: Bool, _ msg: @autoclosure () -> String,
                      _ file: StaticString = #file, _ line: UInt = #line) {
        checks += 1
        if !cond {
            failures += 1
            FileHandle.standardError.write(Data("  ✘ FAIL [\(file):\(line)] \(msg())\n".utf8))
        }
    }

    static func eq<T: Equatable>(_ a: T, _ b: T, _ label: String = "",
                                 _ file: StaticString = #file, _ line: UInt = #line) {
        check(a == b, "\(label): \(a) != \(b)", file, line)
    }

    /// Runs every suite and returns the process exit code (0 = all passed).
    static func run() -> Int32 {
        let suites: [(String, () -> Void)] = [
            ("ConfigStore", configStoreSuite),
            ("Themes", themesSuite),
            ("Tailer", tailerSuite),
        ]
        for (name, body) in suites {
            let before = failures
            body()
            let status = (failures == before) ? "ok" : "FAILED"
            print("• \(name): \(status)")
        }
        print("\n\(checks) checks, \(failures) failures")
        return failures == 0 ? 0 : 1
    }

    // MARK: - ConfigStore suite

    static func configStoreSuite() {
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("ctail-selftest-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: tmp) }
        let store = ConfigStore(root: tmp)

        // round-trip
        var s = AppSettings()
        s.bufferSize = 42_000; s.theme = "nord"; s.recentFiles = ["/a.log", "/b.log"]
        check(store.saveSettings(s), "saveSettings")
        eq(store.loadSettings(), s, "settings round-trip")

        // defaults when missing
        let store2 = ConfigStore(root: tmp.appendingPathComponent("empty"))
        eq(store2.loadSettings().bufferSize, 10_000, "default bufferSize")
        eq(store2.loadSettings().activeProfile, "Common Logs", "default activeProfile")

        // lenient decode
        let json = #"{"bufferSize": 500, "theme": "dracula", "unknownKey": true}"#
        if let d = try? JSONDecoder().decode(AppSettings.self, from: Data(json.utf8)) {
            eq(d.bufferSize, 500, "lenient bufferSize")
            eq(d.theme, "dracula", "lenient theme")
            eq(d.fontSize, 14, "lenient default fontSize")
        } else { check(false, "lenient decode threw") }

        // profile CRUD
        store.ensureDefaultProfile()
        eq(store.listProfiles(), ["Common Logs"], "default profile present")
        let p = Profile(name: "My Profile",
                        rules: [Rule(id: "x", name: "X", pattern: "foo", matchType: "line")])
        check(store.saveProfile(p), "saveProfile")
        eq(store.listProfiles(), ["Common Logs", "My Profile"], "profile listed")
        eq(store.loadProfile("My Profile"), p, "profile round-trip")
        check(store.renameProfile("My Profile", to: "Renamed"), "renameProfile")
        check(store.loadProfile("My Profile") == nil, "old profile gone")
        eq(store.loadProfile("Renamed")?.rules.first?.pattern, "foo", "renamed keeps rules")
        store.deleteProfile("Renamed")
        eq(store.listProfiles(), ["Common Logs"], "profile deleted")

        // recent files MRU + cap
        for i in 0..<20 { store.addRecentFile("/log/\(i).log") }
        eq(store.loadSettings().recentFiles.count, 15, "recent capped at 15")
        eq(store.loadSettings().recentFiles.first, "/log/19.log", "recent MRU order")
        store.addRecentFile("/log/5.log")
        eq(store.loadSettings().recentFiles.first, "/log/5.log", "re-add moves to front")
        eq(store.loadSettings().recentFiles.filter { $0 == "/log/5.log" }.count, 1, "no dupes")

        // sanitize
        eq(ConfigStore.sanitize("a/b:c"), "a_b_c", "sanitize strips path chars")
        eq(ConfigStore.sanitize(""), "profile", "sanitize empty fallback")
    }

    // MARK: - Themes suite

    static func themesSuite() {
        eq(ThemeCatalog.builtIns.count, 21, "21 built-in themes")
        check(ThemeCatalog.builtIns.allSatisfy { !$0.name.isEmpty && !$0.displayName.isEmpty },
              "themes have names")

        // Known palette values from themes.go.
        let cat = ThemeCatalog.palette(name: "catppuccin", mode: "dark")
        eq(cat.bgPrimary, "#1e1e2e", "catppuccin dark bg")
        let catLight = ThemeCatalog.palette(name: "catppuccin", mode: "light")
        eq(catLight.bgPrimary, "#eff1f5", "catppuccin light bg")
        eq(ThemeCatalog.palette(name: "nord", mode: "dark").accent, "#88c0d0", "nord accent")

        // Unknown name falls back to catppuccin.
        eq(ThemeCatalog.palette(name: "does-not-exist", mode: "dark").bgPrimary, "#1e1e2e", "fallback theme")

        // Hex parsing: 3-digit shorthand + alpha=1.
        let c = Theme.hex("#fff")
        eq(Int((c.redComponent * 255).rounded()), 255, "hex shorthand expands")

        // Custom theme JSON round-trips with Go keys and overrides built-ins.
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("ctail-themes-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: tmp) }
        try? FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        let json = #"""
        {"name":"nord","displayName":"My Nord","dark":{"bg-primary":"#010203","text-primary":"#ffffff"}}
        """#
        try? Data(json.utf8).write(to: tmp.appendingPathComponent("nord.json"))
        let p = ThemeCatalog.palette(name: "nord", mode: "dark", custom: tmp)
        eq(p.bgPrimary, "#010203", "custom theme overrides built-in")
    }

    // MARK: - Tailer suite (filled out in issue #3)

    static func tailerSuite() {
        // Placeholder so the suite list is wired; real cases land with engine parity.
    }
}
