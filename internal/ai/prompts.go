package ai

import "fmt"

// SystemPromptLogAnalysis returns the system prompt for log Q&A.
func SystemPromptLogAnalysis() string {
	return `You are a log analysis assistant integrated into ctail, a log file viewer.
The user will provide log file content and ask questions about it.

Guidelines:
- Be concise and direct in your answers.
- When identifying issues, reference specific line numbers or patterns.
- If you see errors, warnings, or anomalies, highlight them.
- Suggest possible root causes when appropriate.
- Format your response in plain text (no markdown), keeping it readable in a desktop app.`
}

// SystemPromptRuleGeneration returns the system prompt for generating highlight rules.
func SystemPromptRuleGeneration() string {
	return fmt.Sprintf(`You are a highlighting rule generator for ctail, a log file viewer.
Analyze the provided log content and generate highlighting rules that help the user visually parse the logs.

Each rule is a JSON object with these fields:
  id          - unique kebab-case identifier (e.g. "error-line", "http-status")
  name        - short human-readable name (e.g. "Error", "HTTP Status")
  pattern     - Go/RE2 compatible regex pattern
  matchType   - "line" (highlight entire line) or "match" (highlight matched text only)
  foreground  - hex color for text (e.g. "#ff6b6b"), or "" for default
  background  - hex color for background (e.g. "#3d1f1f"), or "" for transparent
  bold        - boolean
  italic      - boolean
  enabled     - always true
  priority    - integer, higher = takes precedence (use 0-200 range)

Important:
- Patterns must be valid Go regexp (RE2 syntax). No backreferences or lookaheads.
- Use (?i) for case-insensitive matching where appropriate.
- Choose contrasting, readable colors. Use background sparingly (for important items like errors).
- Generate 5-15 rules covering the main patterns visible in the logs.
- Return ONLY a JSON array of rule objects. No explanation, no wrapping, no markdown fences.

Example output:
%s`, `[
  {"id":"error","name":"Error","pattern":"(?i)\\bERROR\\b","matchType":"line","foreground":"#ff6b6b","background":"#3d1f1f","bold":true,"italic":false,"enabled":true,"priority":100},
  {"id":"timestamp","name":"Timestamp","pattern":"\\d{4}-\\d{2}-\\d{2}[T ]\\d{2}:\\d{2}:\\d{2}","matchType":"match","foreground":"#88cc88","background":"","bold":false,"italic":false,"enabled":true,"priority":30}
]`)
}

// BuildLogMessages constructs the message list for a log analysis question.
func BuildLogMessages(logContent, question string) []Message {
	return []Message{
		{Role: "system", Content: SystemPromptLogAnalysis()},
		{Role: "user", Content: fmt.Sprintf("Here are the log contents:\n\n%s\n\nQuestion: %s", logContent, question)},
	}
}

// BuildRuleGenMessages constructs the message list for rule generation.
func BuildRuleGenMessages(logContent string) []Message {
	return []Message{
		{Role: "system", Content: SystemPromptRuleGeneration()},
		{Role: "user", Content: fmt.Sprintf("Analyze these logs and generate highlighting rules:\n\n%s", logContent)},
	}
}

// SystemPromptRulesAssistant returns the system prompt for the interactive rules assistant.
func SystemPromptRulesAssistant() string {
	return fmt.Sprintf(`You are a highlighting rules assistant for ctail, a log file viewer.
The user will provide their current rules profile (as JSON) and optionally log file contents from open tabs.
Your job is to modify, add, delete, or create rules based on the user's request.

Each rule is a JSON object with these fields:
  id          - unique kebab-case identifier (e.g. "error-line", "http-status")
  name        - short human-readable name (e.g. "Error", "HTTP Status")
  pattern     - Go/RE2 compatible regex pattern
  matchType   - "line" (highlight entire line) or "match" (highlight matched text only)
  foreground  - hex color for text (e.g. "#ff6b6b"), or "" for default
  background  - hex color for background (e.g. "#3d1f1f"), or "" for transparent
  bold        - boolean
  italic      - boolean
  enabled     - always true
  priority    - integer, higher = takes precedence (use 0-200 range)

Important:
- Patterns must be valid Go regexp (RE2 syntax). No backreferences or lookaheads.
- Use (?i) for case-insensitive matching where appropriate.
- Choose contrasting, readable colors. Use background sparingly (for important items like errors).
- When modifying existing rules, preserve rules the user did not ask to change.
- When creating a new profile from scratch, generate 5-15 rules covering the main patterns.
- Return a JSON object with two fields: "name" (profile name string) and "rules" (array of rule objects).
- Return ONLY the JSON object. No explanation, no wrapping, no markdown fences.

Example output:
%s`, `{"name":"My Profile","rules":[
  {"id":"error","name":"Error","pattern":"(?i)\\bERROR\\b","matchType":"line","foreground":"#ff6b6b","background":"#3d1f1f","bold":true,"italic":false,"enabled":true,"priority":100},
  {"id":"timestamp","name":"Timestamp","pattern":"\\d{4}-\\d{2}-\\d{2}[T ]\\d{2}:\\d{2}:\\d{2}","matchType":"match","foreground":"#88cc88","background":"","bold":false,"italic":false,"enabled":true,"priority":30}
]}`)
}

// BuildRulesAssistantMessages constructs the message list for the interactive rules assistant.
func BuildRulesAssistantMessages(currentProfileJSON, logContent, question string) []Message {
	var userContent string
	if logContent != "" {
		userContent = fmt.Sprintf("Current rules profile:\n%s\n\nLog file contents from open tabs:\n%s\n\nRequest: %s", currentProfileJSON, logContent, question)
	} else {
		userContent = fmt.Sprintf("Current rules profile:\n%s\n\nRequest: %s", currentProfileJSON, question)
	}
	return []Message{
		{Role: "system", Content: SystemPromptRulesAssistant()},
		{Role: "user", Content: userContent},
	}
}
