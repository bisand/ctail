import Foundation

/// OpenAI-compatible chat client, ported from internal/ai/client.go. Works with
/// OpenAI, GitHub Models, GitHub Copilot, and any OpenAI-compatible server
/// (Ollama, LM Studio, …). For Copilot, callers pass the exchanged short-lived
/// token and the editor identification headers via `extraHeaders`.
struct AIClient {
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
    static func defaultEndpoint(for provider: String) -> String {
        switch provider {
        case "openai":  return "https://api.openai.com"
        case "github":  return "https://models.inference.ai.azure.com"
        case "copilot": return "https://api.githubcopilot.com"
        default:        return ""
        }
    }

    /// Builds a client for the current settings. For Copilot, exchanges the saved
    /// OAuth token for a short-lived API token (throws .needsCopilotAuth if not
    /// signed in).
    static func makeClient(settings: AppSettings, completion: @escaping (Result<AIClient, Error>) -> Void) {
        let endpoint = settings.aiEndpoint.isEmpty ? defaultEndpoint(for: settings.aiProvider) : settings.aiEndpoint
        let model = settings.aiModel.isEmpty ? "gpt-4o-mini" : settings.aiModel

        if settings.aiProvider == "copilot" {
            guard let oauth = CopilotAuth.savedOAuthToken else {
                return completion(.failure(AIError.needsCopilotAuth))
            }
            CopilotAuth.exchangeToken(oauth: oauth) { result in
                switch result {
                case .success(let token):
                    completion(.success(AIClient(endpoint: endpoint, apiKey: token, model: model,
                                                 extraHeaders: CopilotAuth.editorHeaders)))
                case .failure(let e): completion(.failure(e))
                }
            }
        } else {
            completion(.success(AIClient(endpoint: endpoint, apiKey: settings.aiKey, model: model)))
        }
    }
}
