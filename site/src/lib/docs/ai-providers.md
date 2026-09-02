Configure the assistant under **ctail ▸ Settings… ▸ AI**. You need exactly one provider.

| Provider | Authentication | Notes |
|---|---|---|
| **OpenAI** | API key | Pay per use. Default model `gpt-4o-mini`; any OpenAI model name works |
| **Anthropic** | API key | Claude models. Default `claude-sonnet-4-6` |
| **GitHub Models** | Personal access token | Free tier with any GitHub account |
| **GitHub Copilot** | Sign in with GitHub | Requires an active Copilot subscription |
| **Custom** | Optional API key | Any OpenAI-compatible server: Ollama, LM Studio, vLLM, LocalAI |

The **Fetch** button next to the model field lists the models your provider offers so you can pick one instead of typing it.

## OpenAI

1. Create a key at platform.openai.com.
2. Select **OpenAI**, paste the key and leave the endpoint at its default.

## Anthropic

1. Create a key in the Anthropic console.
2. Select **Anthropic** and paste the key. The default endpoint is `https://api.anthropic.com`.

## GitHub Models

1. Create a fine-grained personal access token with the `models:read` permission.
2. Select **GitHub Models** and paste the token. The endpoint is preset.

## GitHub Copilot

1. Select **GitHub Copilot** and click **Sign in with GitHub**.
2. A browser opens at github.com/login/device. Enter the code ctail shows.
3. Authorise the app. ctail detects the approval and shows *Connected*.

Copilot uses the standard device flow. The token is stored locally and exchanged for a short-lived API token on each request. **Disconnect** removes it.

## Custom and local models

Any server that speaks the OpenAI chat completions API works. ctail appends `/v1/chat/completions` to the endpoint unless you already included it.

- **Ollama:** endpoint `http://localhost:11434`, model such as `llama3.2`. Pull the model first with `ollama pull llama3.2`.
- **LM Studio:** endpoint `http://localhost:1234`, start the server in the app.
- Leave the API key empty for local servers that do not need one.

## Troubleshooting

- **"AI provider not configured"** – choose a provider and finish its setup.
- **Copilot sign-in hangs** – complete the approval in the browser; if a previous attempt timed out, click Sign in again.
- **401 or 403 from GitHub Models** – the token expired or lacks `models:read`.
- **Connection refused for a custom server** – the server is not running, or the endpoint is wrong. For Ollama run `ollama serve`.
- **Empty or generic answers** – send more context or ask a more specific question. Model quality varies a lot.
