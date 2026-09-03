import Foundation
import CtailCore

/// The prompts are the engine's (`core/src/ai.rs`), so every front end asks a
/// model the same thing in the same words.
enum AIPrompts {
    static func logMessages(logContent: String, question: String) -> [AIMessage] {
        aiLogMessages(logContent: logContent, question: question)
    }

    static func ruleGenMessages(logContent: String) -> [AIMessage] {
        aiRuleGenMessages(logContent: logContent)
    }

    /// The rules in a model's answer, wherever in its prose it put them.
    static func extractRules(_ text: String) -> [Rule]? {
        aiExtractRules(text: text)
    }
}
