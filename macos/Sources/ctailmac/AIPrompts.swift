import Foundation

/// Chat message, matching ai.Message in client.go.
struct AIMessage: Codable { let role: String; let content: String }

/// Prompt builders ported verbatim from internal/ai/prompts.go.
enum AIPrompts {
    static let systemLogAnalysis = """
    You are a log analysis assistant integrated into ctail, a log file viewer.
    The user will provide log file content and ask questions about it.

    Guidelines:
    - Be concise and direct in your answers.
    - When identifying issues, reference specific line numbers or patterns.
    - If you see errors, warnings, or anomalies, highlight them.
    - Suggest possible root causes when appropriate.
    - Format your response in plain text (no markdown), keeping it readable in a desktop app.
    """

    static let systemRuleGeneration = """
    You are a highlighting rule generator for ctail, a log file viewer.
    Analyze the provided log content and generate highlighting rules that help the user visually parse the logs.

    Each rule is a JSON object with these fields:
      id          - unique kebab-case identifier (e.g. "error-line", "http-status")
      name        - short human-readable name (e.g. "Error", "HTTP Status")
      pattern     - regex pattern (RE2/ICU compatible)
      matchType   - "line" (highlight entire line) or "match" (highlight matched text only)
      foreground  - hex color for text (e.g. "#ff6b6b"), or "" for default
      background  - hex color for background (e.g. "#3d1f1f"), or "" for transparent
      bold        - boolean
      italic      - boolean
      enabled     - always true
      priority    - integer, higher = takes precedence (use 0-200 range)

    Important:
    - Use (?i) for case-insensitive matching where appropriate.
    - Choose contrasting, readable colors. Use background sparingly (for important items like errors).
    - Generate 5-15 rules covering the main patterns visible in the logs.
    - Return ONLY a JSON array of rule objects. No explanation, no wrapping, no markdown fences.
    """

    static func logMessages(logContent: String, question: String) -> [AIMessage] {
        [AIMessage(role: "system", content: systemLogAnalysis),
         AIMessage(role: "user", content: "Here are the log contents:\n\n\(logContent)\n\nQuestion: \(question)")]
    }

    static func ruleGenMessages(logContent: String) -> [AIMessage] {
        [AIMessage(role: "system", content: systemRuleGeneration),
         AIMessage(role: "user", content: "Analyze these logs and generate highlighting rules:\n\n\(logContent)")]
    }
}
