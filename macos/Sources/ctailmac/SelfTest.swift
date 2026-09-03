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
            ("Search", searchSuite),
            ("Updates", updatesSuite),
            ("AI", aiSuite),
            ("Bookmarks", bookmarksSuite),
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

    // MARK: - Search suite

    static func searchSuite() {
        // Plain substring, case-insensitive by default.
        let q1 = SearchQuery("error", caseSensitive: false, wholeWord: false, isRegex: false)
        check(q1.matches("an ERROR happened"), "case-insensitive plain match")
        check(!q1.matches("all good"), "non-match")

        // Case-sensitive.
        let q2 = SearchQuery("Error", caseSensitive: true, wholeWord: false, isRegex: false)
        check(!q2.matches("an error happened"), "case-sensitive excludes lowercase")
        check(q2.matches("an Error happened"), "case-sensitive matches exact case")

        // Whole word.
        let q3 = SearchQuery("err", caseSensitive: false, wholeWord: true, isRegex: false)
        check(!q3.matches("error"), "whole-word excludes substring")
        check(q3.matches("an err here"), "whole-word matches standalone")

        // Regex + ranges.
        let q4 = SearchQuery(#"\d{3}"#, caseSensitive: false, wholeWord: false, isRegex: true)
        check(q4.matches("code 404 returned"), "regex match")
        eq(q4.ranges(in: "a 404 b 500").count, 2, "regex finds all ranges")

        // Plain query escapes regex metacharacters.
        let q5 = SearchQuery("a.b", caseSensitive: false, wholeWord: false, isRegex: false)
        check(q5.matches("a.b"), "plain matches literal dot")
        check(!q5.matches("axb"), "plain does not treat dot as wildcard")

        // Invalid regex flagged.
        let q6 = SearchQuery("(unclosed", caseSensitive: false, wholeWord: false, isRegex: true)
        check(!q6.isValid, "invalid regex reported")
        let q7 = SearchQuery("", caseSensitive: false, wholeWord: false, isRegex: true)
        check(q7.isValid && q7.isEmpty, "empty query is valid + empty")
    }

    // MARK: - Updates suite

    static func updatesSuite() {
        let c = UpdateChecker.compareVersions
        check(c("1.0.1", "1.0.0") > 0, "patch newer")
        check(c("1.0.0", "1.0.1") < 0, "patch older")
        check(c("1.2.0", "1.10.0") < 0, "numeric (not lexical) compare")
        eq(c("0.9.9", "0.9.9"), 0, "equal versions")
        check(c("1.0", "1.0.0") == 0, "missing components treated as 0")
        check(c("2.0.0", "1.9.9") > 0, "major newer")
        check(c("0.9.9+255", "0.9.9") == 0, "build suffix ignored")
    }

    // MARK: - AI suite (network-free parts)

    static func aiSuite() {
        // Endpoint resolution per provider/base.
        eq(AIClient(endpoint: "https://api.openai.com", apiKey: "", model: "x").completionsURL,
           "https://api.openai.com/v1/chat/completions", "openai endpoint")
        eq(AIClient(endpoint: "https://api.githubcopilot.com", apiKey: "", model: "x").completionsURL,
           "https://api.githubcopilot.com/chat/completions", "copilot endpoint")
        eq(AIClient(endpoint: "http://localhost:11434/v1", apiKey: "", model: "x").completionsURL,
           "http://localhost:11434/v1/chat/completions", "ollama /v1 endpoint")
        eq(AIClient(endpoint: "https://x/v1/chat/completions", apiKey: "", model: "x").completionsURL,
           "https://x/v1/chat/completions", "already-full endpoint untouched")
        eq(AIService.defaultEndpoint(for: "openai"), "https://api.openai.com", "default openai endpoint")

        // Anthropic provider: endpoint/model defaults + /v1/messages URL building.
        eq(AIService.defaultEndpoint(for: "anthropic"), "https://api.anthropic.com", "default anthropic endpoint")
        eq(AIService.defaultModel(for: "anthropic"), "claude-sonnet-4-6", "default anthropic model")
        eq(AnthropicClient(endpoint: "https://api.anthropic.com", apiKey: "", model: "x").messagesURL,
           "https://api.anthropic.com/v1/messages", "anthropic messages URL")
        eq(AnthropicClient(endpoint: "https://proxy/v1", apiKey: "", model: "x").messagesURL,
           "https://proxy/v1/messages", "anthropic /v1 endpoint")
        eq(AnthropicClient(endpoint: "https://proxy/v1/messages", apiKey: "", model: "x").messagesURL,
           "https://proxy/v1/messages", "anthropic already-full endpoint untouched")

        // CLI backend: prompt flattening (system first, then turns) + arg shapes.
        let prompt = CLIChatBackend.combinedPrompt(
            [AIMessage(role: "system", content: "SYS"), AIMessage(role: "user", content: "USER")])
        eq(prompt, "SYS\n\nUSER", "CLI prompt flattens system then user")
        eq(CLIChatBackend.Tool.claude.args(model: "claude-x"), ["-p", "--model", "claude-x"], "claude CLI args")
        eq(CLIChatBackend.Tool.codex.args(model: ""), ["exec"], "codex CLI args without model")

        // Rule-array JSON (what generate-rules returns) decodes into [Rule].
        let json = #"""
        [{"id":"err","name":"Error","pattern":"(?i)ERROR","matchType":"line","foreground":"#ff0000","background":"","bold":true,"italic":false,"enabled":true,"priority":100}]
        """#
        let rules = try? JSONDecoder().decode([Rule].self, from: Data(json.utf8))
        eq(rules?.count, 1, "rule array decodes")
        eq(rules?.first?.matchType, "line", "rule field decoded")

        // Copilot editor headers are present.
        check(CopilotAuth.editorHeaders["Copilot-Integration-Id"] == "vscode-chat", "copilot integration header")
    }

    // MARK: - Bookmarks suite (graceful no-bookmark behavior)

    static func bookmarksSuite() {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("ctail-bm-\(UUID().uuidString)", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = BookmarkStore(dir: dir)
        check(!store.hasBookmark("/no/such/file"), "no bookmark for unknown path")
        check(!store.beginAccess("/no/such/file"), "beginAccess false without bookmark")
        store.endAccess("/no/such/file")   // must not crash
        check(true, "endAccess on unknown path is safe")
    }

    // MARK: - Tailer suite

    /// End-to-end checks through the Rust engine (the engine's own parity suite
    /// lives in core/tests/parity.rs). Callbacks land on the main queue, so the
    /// suite pumps the main run loop while it waits.
    static func tailerSuite() {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("ctail-tailer-\(UUID().uuidString)", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("app.log")
        func write(_ s: String) { try? Data(s.utf8).write(to: file) }
        func appendText(_ s: String) {
            let fh = try? FileHandle(forWritingTo: file)
            fh?.seekToEndOfFile(); fh?.write(Data(s.utf8)); try? fh?.close()
        }
        /// Runs the main run loop until `cond` holds or `timeout` elapses.
        @discardableResult
        func pump(_ timeout: TimeInterval = 5, until cond: () -> Bool) -> Bool {
            let deadline = Date().addingTimeInterval(timeout)
            while !cond() && Date() < deadline {
                RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01))
            }
            return cond()
        }
        func settle(_ seconds: TimeInterval) { pump(seconds) { false } }

        var received: [LogLine] = []
        var resets = 0
        var readies = 0
        var errors: [String] = []

        write("line1\nline2\nline3\n")
        let t = Tailer(path: file.path, pollInterval: 0.05)
        t.onLines = { received += $0 }
        t.onReset = { resets += 1 }
        t.onReady = { readies += 1 }
        t.onError = { errors.append($0) }
        t.start()
        check(pump { readies == 1 && received.count == 3 }, "initial read delivers 3 lines + ready")
        eq(received.map { $0.text }, ["line1", "line2", "line3"], "initial content")
        eq(received.map { $0.number }, [1, 2, 3], "numbers are 1-based")
        eq(t.totalLines, 3, "totalLines after initial read")
        eq(t.indexingComplete, true, "small file is indexed immediately")

        var got: [LogLine]?
        t.fetchRange(start: 2, count: 1) { got = $0 }
        check(pump { got != nil }, "fetchRange completes")
        eq(got?.map { $0.text }, ["line2"], "fetchRange windowed")
        got = nil
        t.fetchRange(start: 1, count: 10) { got = $0 }
        check(pump { got != nil }, "fetchRange completes (clamped)")
        eq(got?.count, 3, "fetchRange clamps at EOF")

        // Append -> poll picks up only the new line.
        appendText("line4\n")
        check(pump { received.count == 4 }, "poll delivers the appended line")
        eq(received.last?.text, "line4", "appended text")
        eq(received.last?.number, 4, "appended number")

        // Partial line not committed until its newline arrives.
        appendText("partial-no-newline")
        settle(0.25)
        eq(received.count, 4, "partial line not delivered yet")
        eq(t.totalLines, 4, "partial line not counted yet")
        appendText("\n")
        check(pump { received.count == 5 }, "partial completes on newline")
        eq(received.last?.text, "partial-no-newline", "completed partial text")

        // Truncation -> reset + re-read.
        received = []
        write("fresh1\nfresh2\n")
        check(pump { resets == 1 && received.count == 2 }, "truncation resets and re-reads")
        eq(received.map { $0.text }, ["fresh1", "fresh2"], "post-truncation content")
        eq(t.totalLines, 2, "totalLines after truncation")

        // Rotation (new inode, larger file) -> reset + re-read.
        received = []
        try? FileManager.default.removeItem(at: file)
        write("rotated-1\nrotated-2\nrotated-3\n")
        check(pump { resets == 2 && received.count == 3 }, "rotation re-reads the new inode")
        eq(received.first?.text, "rotated-1", "rotated content")

        // File gone -> one error; back -> ready again + content.
        received = []
        try? FileManager.default.removeItem(at: file)
        check(pump { !errors.isEmpty }, "missing file reports an error")
        settle(0.2)
        eq(errors.count, 1, "error reported once per outage")
        check(errors.first?.contains("file unavailable") == true, "error message")
        write("back\n")
        check(pump { received.last?.text == "back" }, "recreated file is read")

        // Manual refresh re-reads from scratch.
        received = []
        t.refresh()
        check(pump { resets == 4 && received.count == 1 }, "refresh resets and re-reads")
        t.setPollInterval(0.1)
        t.stop()

        // Tail-first (instant tail): tiny thresholds force the large-file path.
        var tfBody = ""
        for n in 1...50 { tfBody += "L\(n)\n" }
        write(tfBody)
        let tf = Tailer(path: file.path, pollInterval: 0.05, tailFirstThreshold: 20, tailSeekBack: 12)
        var tfLines: [LogLine] = []
        var base: Int64?
        tf.onLines = { tfLines += $0 }
        tf.onBaseResolved = { base = $0 }
        tf.start()
        check(pump { base != nil }, "head count resolves in the background")
        eq(tf.indexingComplete, true, "indexing complete after head count")
        eq(tf.totalLines, 50, "absolute total after head count")
        check((base ?? 0) > 0 && (base ?? 0) < 50, "base is the head line count")
        eq(tfLines.last?.text, "L50", "tail shown first")
        eq(Int64(tfLines.count) + (base ?? 0), 50, "tail lines + base = total")
        var head: [LogLine]?
        tf.fetchRange(start: 1, count: 1) { head = $0 }
        check(pump { head != nil }, "head-region scrollback completes")
        eq(head?.first?.text, "L1", "head-region line via scrollback")
        var tail: [LogLine]?
        tf.fetchRange(start: 50, count: 1) { tail = $0 }
        check(pump { tail != nil }, "tail-region scrollback completes")
        eq(tail?.first?.text, "L50", "tail-region last line")
        eq(tail?.first?.number, 50, "absolute number after base")
        tf.stop()
    }
}
