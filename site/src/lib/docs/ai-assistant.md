<script>
	import Shot from '$lib/components/Shot.svelte';
</script>

The AI assistant answers questions about the log in front of you and can write a complete [highlighting profile](../profiles/) for an unfamiliar format. It is part of [ctail Pro](../pro/) and needs a provider configured in Settings; see [AI providers](../ai-providers/).

<Shot src="ai.webp" alt="The AI assistant explaining a web server log" />

Nothing is sent anywhere until you press **Ask** or **Generate Rules Profile**. ctail never contacts an AI provider in the background.

## Opening the assistant

- **Tools ▸ AI Assistant…** or <kbd>⌘⇧A</kbd>
- Right-click in the log and choose **Ask AI about logs**

## Asking a question

Type a question in the field at the top and press **Ask**. The assistant receives the current tab's log context along with your question and replies in the window. Good questions are specific:

- "What caused the database reconnect at 08:01:15?"
- "Summarise the errors in this log and rank them by severity."
- "Explain this stack trace."

Responses can be selected and copied.

## Generating a rules profile

Open a file that represents the format you want to highlight, then click **Generate Rules Profile**. The assistant reads the log, works out which patterns matter, such as levels, timestamps, IP addresses, request paths and identifiers, and produces a full profile with patterns, match types, colours and a sensible priority order.

The new profile is saved, activated, and named after the number of rules it contains. Edit it in **Profiles & Rules** like any other profile.

## Privacy

- Log text is sent only to the provider you configured, and only when you trigger an action.
- With a local server such as Ollama or LM Studio, nothing leaves your Mac.
- API keys and tokens are stored in the local settings file and sent only to the configured endpoint.
- GitHub and OpenAI process data under their own privacy terms.
