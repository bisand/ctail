import Foundation

/// OpenAI-compatible chat client, ported from internal/ai/client.go. Works with
/// OpenAI, GitHub Models, GitHub Copilot, and any OpenAI-compatible server
/// (Ollama, LM Studio, …). For Copilot, callers pass the exchanged short-lived
/// token and the editor identification headers via `extraHeaders`.
struct AIClient: ChatBackend {
    let endpoint: String      // base URL, e.g. "https://api.openai.com"
    let apiKey: String
    let model: String
    var extraHeaders: [String: String] = [:]

    private struct ChatRequest: Encodable { let model: String; let messages: [AIMessage] }
    private struct ChatResponse: Decodable {
        struct Choice: Decodable { struct Msg: Decodable { let content: String }; let message: Msg }
        struct Err: Decodable { let message: String }
        let choices: [Choice]?
        let error: Err?
    }

    /// Resolves the full chat completions URL (mirrors client.go completionsURL).
    var completionsURL: String {
        var base = endpoint
        while base.hasSuffix("/") { base.removeLast() }
        if base.hasSuffix("/chat/completions") || base.hasSuffix("/v1/chat/completions") { return base }
        if base.hasSuffix("/v1") { return base + "/chat/completions" }
        // Copilot and GitHub Models expose /chat/completions without /v1.
        if base.contains("githubcopilot") || base.contains("models.inference") || base.contains("models.github") {
            return base + "/chat/completions"
        }
        return base + "/v1/chat/completions"
    }

    func chat(_ messages: [AIMessage], completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: completionsURL) else {
            return completion(.failure(AIError.message("Invalid AI endpoint"))) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.timeoutInterval = 60
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if !apiKey.isEmpty { req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization") }
        extraHeaders.forEach { req.setValue($1, forHTTPHeaderField: $0) }
        req.httpBody = try? JSONEncoder().encode(ChatRequest(model: model, messages: messages))

        URLSession.shared.dataTask(with: req) { data, resp, err in
            let done: (Result<String, Error>) -> Void = { r in DispatchQueue.main.async { completion(r) } }
            if let err { return done(.failure(err)) }
            guard let data else { return done(.failure(AIError.message("No response"))) }
            let parsed = try? JSONDecoder().decode(ChatResponse.self, from: data)
            if let msg = parsed?.error?.message { return done(.failure(AIError.message(msg))) }
            if let http = resp as? HTTPURLResponse, http.statusCode != 200 {
                let body = String(data: data, encoding: .utf8)?.prefix(300) ?? ""
                return done(.failure(AIError.message("AI request failed (HTTP \(http.statusCode)): \(body)")))
            }
            guard let content = parsed?.choices?.first?.message.content else {
                return done(.failure(AIError.message("Empty AI response")))
            }
            done(.success(content))
        }.resume()
    }
}

enum AIError: LocalizedError {
    case message(String)
    case needsCopilotAuth
    var errorDescription: String? {
        switch self {
        case .message(let m): return m
        case .needsCopilotAuth: return "Sign in to GitHub Copilot first."
        }
    }
}

/// Resolves provider settings into a ready-to-use AIClient, handling Copilot's
/// token exchange. Default endpoints match the Go app.
enum AIService {
    /// Providers the app knows about. CLI tools are only offered off-sandbox.
    static let apiProviders = ["openai", "github", "copilot", "anthropic", "custom"]
    static let cliProviders = ["claude-cli", "codex-cli"]

    static func defaultEndpoint(for provider: String) -> String {
        switch provider {
        case "openai":    return "https://api.openai.com"
        case "github":    return "https://models.inference.ai.azure.com"
        case "copilot":   return "https://api.githubcopilot.com"
        case "anthropic": return "https://api.anthropic.com"
        default:          return ""
        }
    }

    static func defaultModel(for provider: String) -> String {
        switch provider {
        case "anthropic":              return "claude-sonnet-4-6"
        case "claude-cli", "codex-cli": return ""    // the CLI picks its own default
        default:                       return "gpt-4o-mini"
        }
    }

    /// Builds a backend for the current settings, picking the right transport for
    /// the provider. Copilot exchanges its OAuth token for a short-lived API token
    /// (fails with .needsCopilotAuth if not signed in); CLI tools are refused in a
    /// sandboxed build.
    static func makeClient(settings: AppSettings, completion: @escaping (Result<any ChatBackend, Error>) -> Void) {
        let provider = settings.aiProvider
        let model = settings.aiModel.isEmpty ? defaultModel(for: provider) : settings.aiModel

        switch provider {
        case "anthropic":
            let endpoint = settings.aiEndpoint.isEmpty ? defaultEndpoint(for: "anthropic") : settings.aiEndpoint
            completion(.success(AnthropicClient(endpoint: endpoint, apiKey: settings.aiKey, model: model)))

        case "claude-cli", "codex-cli":
            guard !AIEnvironment.isSandboxed else {
                return completion(.failure(AIError.message(
                    "CLI tools aren't available in the App Store build. Choose an API provider, or use the direct-download build of ctail.")))
            }
            let tool: CLIChatBackend.Tool = (provider == "claude-cli") ? .claude : .codex
            completion(.success(CLIChatBackend(tool: tool, model: model)))

        case "copilot":
            guard let oauth = CopilotAuth.savedOAuthToken else {
                return completion(.failure(AIError.needsCopilotAuth))
            }
            let endpoint = settings.aiEndpoint.isEmpty ? defaultEndpoint(for: "copilot") : settings.aiEndpoint
            CopilotAuth.exchangeToken(oauth: oauth) { result in
                switch result {
                case .success(let token):
                    completion(.success(AIClient(endpoint: endpoint, apiKey: token, model: model,
                                                 extraHeaders: CopilotAuth.editorHeaders)))
                case .failure(let e): completion(.failure(e))
                }
            }

        default:
            let endpoint = settings.aiEndpoint.isEmpty ? defaultEndpoint(for: provider) : settings.aiEndpoint
            completion(.success(AIClient(endpoint: endpoint, apiKey: settings.aiKey, model: model)))
        }
    }
}
