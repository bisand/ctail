import Foundation

/// GitHub Copilot device-flow OAuth, ported from internal/ai/device_flow.go.
/// Flow: request a device code → user enters it at github.com → poll for the
/// OAuth token → exchange it for a short-lived Copilot API token per request.
enum CopilotAuth {
    static let clientID = "Iv1.b507a08c87ecfe98"   // public Copilot editor client id
    static let deviceCodeEndpoint = "https://github.com/login/device/code"
    static let pollEndpoint = "https://github.com/login/oauth/access_token"
    static let exchangeEndpoint = "https://api.github.com/copilot_internal/v2/token"
    private static let oauthKey = "copilotOAuthToken"

    static let editorHeaders = [
        "Editor-Version": "vscode/1.100.0",
        "Editor-Plugin-Version": "copilot/1.300.0",
        "User-Agent": "GithubCopilot/1.300.0",
        "Copilot-Integration-Id": "vscode-chat",
    ]

    struct DeviceCode { let deviceCode: String; let userCode: String; let verificationURI: String; let interval: Int }

    static var savedOAuthToken: String? {
        get { UserDefaults.standard.string(forKey: oauthKey) }
        set { UserDefaults.standard.set(newValue, forKey: oauthKey) }
    }
    static var isSignedIn: Bool { savedOAuthToken != nil }
    static func signOut() { UserDefaults.standard.removeObject(forKey: oauthKey) }

    // MARK: - Device flow

    static func requestDeviceCode(completion: @escaping (Result<DeviceCode, Error>) -> Void) {
        postForm(deviceCodeEndpoint,
                 ["client_id": clientID, "scope": "read:user"]) { result in
            switch result {
            case .failure(let e): completion(.failure(e))
            case .success(let obj):
                guard let dc = obj["device_code"] as? String, let uc = obj["user_code"] as? String,
                      let uri = obj["verification_uri"] as? String else {
                    return completion(.failure(AIError.message("Unexpected device-code response")))
                }
                completion(.success(DeviceCode(deviceCode: dc, userCode: uc, verificationURI: uri,
                                               interval: (obj["interval"] as? Int) ?? 5)))
            }
        }
    }

    /// Polls until the user authorizes (or it errors). Honors slow_down/pending.
    static func pollForToken(deviceCode: String, interval: Int,
                             completion: @escaping (Result<String, Error>) -> Void) {
        var wait = max(5, interval)
        func tick() {
            DispatchQueue.global().asyncAfter(deadline: .now() + Double(wait)) {
                postForm(pollEndpoint, ["client_id": clientID, "device_code": deviceCode,
                                        "grant_type": "urn:ietf:params:oauth:grant-type:device_code"]) { result in
                    switch result {
                    case .failure(let e): DispatchQueue.main.async { completion(.failure(e)) }
                    case .success(let obj):
                        if let token = obj["access_token"] as? String {
                            savedOAuthToken = token
                            DispatchQueue.main.async { completion(.success(token)) }
                        } else if let err = obj["error"] as? String {
                            switch err {
                            case "authorization_pending": tick()
                            case "slow_down": wait += 5; tick()
                            case "expired_token":
                                DispatchQueue.main.async { completion(.failure(AIError.message("Code expired — try again"))) }
                            default:
                                DispatchQueue.main.async { completion(.failure(AIError.message(err))) }
                            }
                        } else { tick() }
                    }
                }
            }
        }
        tick()
    }

    /// Exchanges the GitHub OAuth token for a short-lived Copilot API token.
    static func exchangeToken(oauth: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: exchangeEndpoint) else {
            return completion(.failure(AIError.message("bad exchange URL"))) }
        var req = URLRequest(url: url)
        req.setValue("token \(oauth)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        editorHeaders.forEach { req.setValue($1, forHTTPHeaderField: $0) }
        URLSession.shared.dataTask(with: req) { data, resp, err in
            let done: (Result<String, Error>) -> Void = { r in DispatchQueue.main.async { completion(r) } }
            if let err { return done(.failure(err)) }
            guard let data, let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let token = obj["token"] as? String, !token.isEmpty else {
                return done(.failure(AIError.message("Copilot token exchange failed")))
            }
            done(.success(token))
        }.resume()
    }

    // MARK: - HTTP helper

    private static func postForm(_ urlString: String, _ params: [String: String],
                                 completion: @escaping (Result<[String: Any], Error>) -> Void) {
        guard let url = URL(string: urlString) else { return completion(.failure(AIError.message("bad URL"))) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.httpBody = try? JSONSerialization.data(withJSONObject: params)
        URLSession.shared.dataTask(with: req) { data, _, err in
            if let err { return completion(.failure(err)) }
            guard let data, let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                return completion(.failure(AIError.message("Invalid response")))
            }
            completion(.success(obj))
        }.resume()
    }
}
