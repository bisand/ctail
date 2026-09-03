import Foundation
import CtailCore

/// Swift face of the engine's AI assistant (`core/src/ai.rs`): the providers,
/// their HTTP shapes, the Copilot sign-in and the CLI tools all live there.
/// Every engine call blocks on the network, so each one here runs on a global
/// queue and answers on the main queue, the way the window expects.
///
/// What is left on this side is what only this side can do: keep the Copilot
/// OAuth token between runs, and put an answer in a window.

/// The engine's errors, as the window shows them. `needsCopilotAuth` is its
/// own case because the window answers it by starting a sign-in.
enum AIError: LocalizedError {
    case message(String)
    case needsCopilotAuth

    init(_ error: Error) {
        switch error {
        case CtailCore.AiError.NeedsCopilotAuth: self = .needsCopilotAuth
        case CtailCore.AiError.Message(let message): self = .message(message)
        default: self = .message(error.localizedDescription)
        }
    }

    var errorDescription: String? {
        switch self {
        case .message(let m): return m
        case .needsCopilotAuth: return "Sign in to GitHub Copilot first."
        }
    }
}

/// Runtime environment checks that gate features by distribution channel.
enum AIEnvironment {
    /// True inside the App Sandbox (the Mac App Store build), where spawning
    /// an executable is not allowed, so the CLI providers are hidden.
    static var isSandboxed: Bool { aiIsSandboxed() }
}

enum AIService {
    static var apiProviders: [String] { aiApiProviders() }
    static var cliProviders: [String] { aiCliProviders() }
    static func defaultEndpoint(for provider: String) -> String { aiDefaultEndpoint(provider: provider) }
    static func defaultModel(for provider: String) -> String { aiDefaultModel(provider: provider) }

    /// One turn with the provider in `settings`. Fails with
    /// `AIError.needsCopilotAuth` when Copilot is chosen and nobody is signed in.
    static func chat(settings: AppSettings, messages: [AIMessage],
                     completion: @escaping (Result<String, Error>) -> Void) {
        offMain(completion) { try aiChat(settings: settings, copilotOauth: CopilotAuth.savedOAuthToken, messages: messages) }
    }
}

/// The provider's model list, for the Settings picker.
enum ModelCatalog {
    static func fetch(settings: AppSettings, completion: @escaping (Result<[String], Error>) -> Void) {
        offMain(completion) { try aiListModels(settings: settings, copilotOauth: CopilotAuth.savedOAuthToken) }
    }
}

/// GitHub Copilot's device-flow sign-in. The flow itself is the engine's; the
/// token it ends in is kept here, in the defaults, between runs.
enum CopilotAuth {
    private static let oauthKey = "copilotOAuthToken"

    static var savedOAuthToken: String? {
        get { UserDefaults.standard.string(forKey: oauthKey) }
        set { UserDefaults.standard.set(newValue, forKey: oauthKey) }
    }
    static var isSignedIn: Bool { savedOAuthToken != nil }
    static func signOut() { UserDefaults.standard.removeObject(forKey: oauthKey) }

    static func requestDeviceCode(completion: @escaping (Result<CopilotDeviceCode, Error>) -> Void) {
        offMain(completion) { try copilotRequestDeviceCode() }
    }

    /// Waits for the user to authorise at github.com — as long as that takes —
    /// then keeps the token and answers with it.
    static func pollForToken(deviceCode: String, interval: UInt32,
                             completion: @escaping (Result<String, Error>) -> Void) {
        offMain(completion) {
            let token = try copilotPollForToken(deviceCode: deviceCode, interval: interval)
            DispatchQueue.main.async { savedOAuthToken = token }
            return token
        }
    }
}

/// Runs `work` on a global queue and hands its outcome to `completion` on the
/// main queue, with the engine's error translated on the way.
private func offMain<T>(_ completion: @escaping (Result<T, Error>) -> Void, _ work: @escaping () throws -> T) {
    DispatchQueue.global(qos: .userInitiated).async {
        let outcome: Result<T, Error>
        do { outcome = .success(try work()) } catch { outcome = .failure(AIError(error)) }
        DispatchQueue.main.async { completion(outcome) }
    }
}
