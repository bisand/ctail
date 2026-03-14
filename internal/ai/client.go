package ai

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Provider identifies the AI backend type.
type Provider string

const (
	ProviderOpenAI       Provider = "openai"
	ProviderGitHubModels Provider = "github"
	ProviderCopilot      Provider = "copilot"
	ProviderCustom       Provider = "custom"
)

// Config holds settings for the AI client.
type Config struct {
	Provider Provider
	Endpoint string // Base URL (e.g. "https://api.openai.com")
	APIKey   string
	Model    string
	Timeout  time.Duration
}

// Message represents a chat message.
type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// Client communicates with OpenAI-compatible chat APIs.
type Client struct {
	cfg    Config
	http   *http.Client
}

// chatRequest is the request body for /v1/chat/completions.
type chatRequest struct {
	Model    string    `json:"model"`
	Messages []Message `json:"messages"`
}

// chatResponse is the relevant portion of the API response.
type chatResponse struct {
	Choices []struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	} `json:"choices"`
	Error *struct {
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

// NewClient creates an AI client from the given config.
func NewClient(cfg Config) *Client {
	if cfg.Timeout == 0 {
		cfg.Timeout = 30 * time.Second
	}
	if cfg.Model == "" {
		cfg.Model = "gpt-4o-mini"
	}
	return &Client{
		cfg: cfg,
		http: &http.Client{Timeout: cfg.Timeout},
	}
}

// completionsURL returns the full chat completions endpoint.
func (c *Client) completionsURL() string {
	base := strings.TrimRight(c.cfg.Endpoint, "/")
	switch c.cfg.Provider {
	case ProviderGitHubModels:
		return base + "/chat/completions"
	case ProviderCopilot:
		return base + "/chat/completions"
	default:
		return base + "/v1/chat/completions"
	}
}

// Chat sends a list of messages and returns the assistant's reply.
func (c *Client) Chat(messages []Message) (string, error) {
	body, err := json.Marshal(chatRequest{
		Model:    c.cfg.Model,
		Messages: messages,
	})
	if err != nil {
		return "", fmt.Errorf("ai: marshal request: %w", err)
	}

	req, err := http.NewRequest("POST", c.completionsURL(), bytes.NewReader(body))
	if err != nil {
		return "", fmt.Errorf("ai: create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")

	switch c.cfg.Provider {
	case ProviderCopilot:
		req.Header.Set("Authorization", "Bearer "+c.cfg.APIKey)
		setCopilotHeaders(req)
	default:
		req.Header.Set("Authorization", "Bearer "+c.cfg.APIKey)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return "", fmt.Errorf("ai: request failed: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("ai: read response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("ai: API error %d: %s", resp.StatusCode, truncate(string(respBody), 200))
	}

	var chatResp chatResponse
	if err := json.Unmarshal(respBody, &chatResp); err != nil {
		return "", fmt.Errorf("ai: parse response: %w", err)
	}

	if chatResp.Error != nil {
		return "", fmt.Errorf("ai: %s", chatResp.Error.Message)
	}

	if len(chatResp.Choices) == 0 {
		return "", fmt.Errorf("ai: empty response (no choices)")
	}

	return chatResp.Choices[0].Message.Content, nil
}

func truncate(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen] + "..."
}
