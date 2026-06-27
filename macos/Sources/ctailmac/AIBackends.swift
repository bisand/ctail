import Foundation

/// A chat backend the assistant can talk to. Implemented by the OpenAI-compatible
/// `AIClient`, the `AnthropicClient`, and the CLI-tool backends. Keeping this
/// behind a protocol lets `AIService` pick a provider without the UI caring how
/// the request is actually fulfilled (HTTP vs. a local subprocess).
protocol ChatBackend {
    func chat(_ messages: [AIMessage], completion: @escaping (Result<String, Error>) -> Void)
}

/// Runtime environment checks that gate features by distribution channel.
enum AIEnvironment {
    /// True inside the App Sandbox (the Mac App Store build). Sandboxed apps may
    /// not spawn external executables, so CLI providers are hidden there.
    static var isSandboxed: Bool {
        ProcessInfo.processInfo.environment["APP_SANDBOX_CONTAINER_ID"] != nil
    }
}

// MARK: - Anthropic (Claude API)

/// Anthropic Messages API client (https://docs.anthropic.com). Unlike the
/// OpenAI-compatible shape, the system prompt is a top-level field, auth is the
/// `x-api-key` header, and `max_tokens` is required.
struct AnthropicClient: ChatBackend {
    let endpoint: String       // base URL, e.g. "https://api.anthropic.com"
    let apiKey: String
    let model: String
    var maxTokens = 2048
    var version = "2023-06-01"

    private struct Req: Encodable {
        let model: String
        let max_tokens: Int
        let system: String?
        let messages: [AIMessage]
    }
    private struct Resp: Decodable {
        struct Block: Decodable { let type: String; let text: String? }
        struct Err: Decodable { let message: String }
        let content: [Block]?
        let error: Err?
    }

    var messagesURL: String {
        var base = endpoint
        while base.hasSuffix("/") { base.removeLast() }
        if base.hasSuffix("/v1/messages") { return base }
        if base.hasSuffix("/v1") { return base + "/messages" }
        return base + "/v1/messages"
    }

    func chat(_ messages: [AIMessage], completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: messagesURL) else {
            return completion(.failure(AIError.message("Invalid Anthropic endpoint")))
        }
        // Anthropic takes the system prompt separately; the rest are user/assistant turns.
        let system = messages.filter { $0.role == "system" }.map { $0.content }.joined(separator: "\n\n")
        let turns = messages.filter { $0.role != "system" }

        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.timeoutInterval = 60
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue(apiKey, forHTTPHeaderField: "x-api-key")
        req.setValue(version, forHTTPHeaderField: "anthropic-version")
        req.httpBody = try? JSONEncoder().encode(
            Req(model: model, max_tokens: maxTokens, system: system.isEmpty ? nil : system, messages: turns))

        URLSession.shared.dataTask(with: req) { data, resp, err in
            let done: (Result<String, Error>) -> Void = { r in DispatchQueue.main.async { completion(r) } }
            if let err { return done(.failure(err)) }
            guard let data else { return done(.failure(AIError.message("No response"))) }
            let parsed = try? JSONDecoder().decode(Resp.self, from: data)
            if let msg = parsed?.error?.message { return done(.failure(AIError.message(msg))) }
            if let http = resp as? HTTPURLResponse, http.statusCode != 200 {
                let body = String(data: data, encoding: .utf8)?.prefix(300) ?? ""
                return done(.failure(AIError.message("Anthropic request failed (HTTP \(http.statusCode)): \(body)")))
            }
            let text = parsed?.content?.compactMap { $0.text }.joined() ?? ""
            guard !text.isEmpty else { return done(.failure(AIError.message("Empty AI response"))) }
            done(.success(text))
        }.resume()
    }
}

// MARK: - CLI tools (Claude Code / Codex) — non-sandboxed builds only

/// Runs a locally-installed AI CLI (the `claude` or `codex` command) as a
/// subprocess, feeding the prompt on stdin and returning stdout. Only usable in a
/// non-sandboxed (direct-download / notarized) build — `AIService` refuses it
/// when sandboxed, and Settings hides it.
struct CLIChatBackend: ChatBackend {
    enum Tool {
        case claude, codex
        var binary: String { self == .claude ? "claude" : "codex" }
        /// Non-interactive invocation for each tool (prompt arrives on stdin).
        func args(model: String) -> [String] {
            switch self {
            case .claude:
                var a = ["-p"]; if !model.isEmpty { a += ["--model", model] }; return a
            case .codex:
                var a = ["exec"]; if !model.isEmpty { a += ["--model", model] }; return a
            }
        }
    }

    let tool: Tool
    let model: String

    func chat(_ messages: [AIMessage], completion: @escaping (Result<String, Error>) -> Void) {
        let prompt = Self.combinedPrompt(messages)
        let tool = self.tool, model = self.model
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Self.run(binary: tool.binary, args: tool.args(model: model), stdin: prompt)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Flattens the chat into one prompt: system guidance first, then the turns.
    static func combinedPrompt(_ messages: [AIMessage]) -> String {
        let system = messages.filter { $0.role == "system" }.map { $0.content }.joined(separator: "\n\n")
        let rest = messages.filter { $0.role != "system" }.map { $0.content }.joined(separator: "\n\n")
        return system.isEmpty ? rest : system + "\n\n" + rest
    }

    private static func run(binary: String, args: [String], stdin: String) -> Result<String, Error> {
        guard let path = resolve(binary) else {
            return .failure(AIError.message("`\(binary)` not found. Install it and make sure it's on your PATH."))
        }
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        proc.arguments = args
        let inPipe = Pipe(), outPipe = Pipe(), errPipe = Pipe()
        proc.standardInput = inPipe
        proc.standardOutput = outPipe
        proc.standardError = errPipe
        do { try proc.run() } catch {
            return .failure(AIError.message("Failed to launch `\(binary)`: \(error.localizedDescription)"))
        }
        inPipe.fileHandleForWriting.write(Data(stdin.utf8))
        try? inPipe.fileHandleForWriting.close()
        let outData = outPipe.fileHandleForReading.readDataToEndOfFile()
        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()

        let out = (String(data: outData, encoding: .utf8) ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if proc.terminationStatus != 0 {
            let errText = String(data: errData, encoding: .utf8) ?? ""
            let detail = (errText.isEmpty ? out : errText).prefix(300)
            return .failure(AIError.message("`\(binary)` failed (exit \(proc.terminationStatus)): \(detail)"))
        }
        return out.isEmpty ? .failure(AIError.message("`\(binary)` returned no output")) : .success(out)
    }

    /// GUI apps launched from Finder inherit a minimal PATH, so resolve the binary
    /// from common install dirs first, then fall back to the login shell's PATH.
    private static var cache: [String: String] = [:]
    private static func resolve(_ binary: String) -> String? {
        if let hit = cache[binary] { return hit.isEmpty ? nil : hit }
        let home = NSHomeDirectory()
        let dirs = ["/opt/homebrew/bin/", "/usr/local/bin/", "/usr/bin/",
                    home + "/.local/bin/", home + "/.npm-global/bin/", home + "/.bun/bin/"]
        for d in dirs where FileManager.default.isExecutableFile(atPath: d + binary) {
            cache[binary] = d + binary; return d + binary
        }
        let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: shell)
        proc.arguments = ["-lc", "command -v \(binary)"]
        let pipe = Pipe(); proc.standardOutput = pipe; proc.standardError = Pipe()
        guard (try? proc.run()) != nil else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let path = (String(data: data, encoding: .utf8) ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        cache[binary] = path
        return path.isEmpty ? nil : path
    }
}
