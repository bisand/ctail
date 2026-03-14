package ai

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// --- completionsURL tests ---

func TestCompletionsURL(t *testing.T) {
	tests := []struct {
		name     string
		provider Provider
		endpoint string
		want     string
	}{
		{"OpenAI", ProviderOpenAI, "https://api.openai.com", "https://api.openai.com/v1/chat/completions"},
		{"OpenAI trailing slash", ProviderOpenAI, "https://api.openai.com/", "https://api.openai.com/v1/chat/completions"},
		{"GitHub Models", ProviderGitHubModels, "https://models.inference.ai.azure.com", "https://models.inference.ai.azure.com/chat/completions"},
		{"Copilot", ProviderCopilot, "https://api.githubcopilot.com", "https://api.githubcopilot.com/chat/completions"},
		{"Custom", ProviderCustom, "http://localhost:11434", "http://localhost:11434/v1/chat/completions"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := NewClient(Config{Provider: tt.provider, Endpoint: tt.endpoint})
			got := c.completionsURL()
			if got != tt.want {
				t.Errorf("completionsURL() = %q, want %q", got, tt.want)
			}
		})
	}
}

// --- NewClient defaults ---

func TestNewClientDefaults(t *testing.T) {
	c := NewClient(Config{Provider: ProviderOpenAI, Endpoint: "http://test"})
	if c.cfg.Model != "gpt-4o-mini" {
		t.Errorf("default model = %q, want gpt-4o-mini", c.cfg.Model)
	}
	if c.cfg.Timeout != 30*1e9 { // 30s in nanoseconds
		t.Errorf("default timeout = %v, want 30s", c.cfg.Timeout)
	}
}

func TestNewClientCustomModel(t *testing.T) {
	c := NewClient(Config{Provider: ProviderCustom, Endpoint: "http://test", Model: "llama3.2"})
	if c.cfg.Model != "llama3.2" {
		t.Errorf("model = %q, want llama3.2", c.cfg.Model)
	}
}

// --- Chat provider-specific behavior ---

func TestClientChat(t *testing.T) {
	// Mock OpenAI-compatible server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("missing or wrong auth header")
		}

		resp := chatResponse{
			Choices: []struct {
				Message struct {
					Content string `json:"content"`
				} `json:"message"`
			}{
				{Message: struct {
					Content string `json:"content"`
				}{Content: "The logs show an error on line 42."}},
			},
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	client := NewClient(Config{
		Provider: ProviderCustom,
		Endpoint: server.URL,
		APIKey:   "test-key",
		Model:    "test-model",
	})

	reply, err := client.Chat([]Message{
		{Role: "user", Content: "What's wrong?"},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if reply != "The logs show an error on line 42." {
		t.Errorf("unexpected reply: %s", reply)
	}
}

func TestClientChatGitHubModelsPath(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Errorf("GitHub Models should use /chat/completions, got %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer ghp_test" {
			t.Errorf("auth = %q, want Bearer ghp_test", got)
		}
		json.NewEncoder(w).Encode(chatResponse{
			Choices: []struct {
				Message struct {
					Content string `json:"content"`
				} `json:"message"`
			}{{Message: struct {
				Content string `json:"content"`
			}{Content: "ok"}}},
		})
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderGitHubModels, Endpoint: server.URL, APIKey: "ghp_test"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestClientChatCopilotHeaders(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Errorf("Copilot should use /chat/completions, got %s", r.URL.Path)
		}
		// Verify Copilot-specific headers
		if r.Header.Get("Editor-Version") == "" {
			t.Error("missing Editor-Version header")
		}
		if r.Header.Get("Editor-Plugin-Version") == "" {
			t.Error("missing Editor-Plugin-Version header")
		}
		if r.Header.Get("Copilot-Integration-Id") == "" {
			t.Error("missing Copilot-Integration-Id header")
		}
		if !strings.Contains(r.Header.Get("User-Agent"), "GithubCopilot") {
			t.Error("User-Agent should contain GithubCopilot")
		}
		json.NewEncoder(w).Encode(chatResponse{
			Choices: []struct {
				Message struct {
					Content string `json:"content"`
				} `json:"message"`
			}{{Message: struct {
				Content string `json:"content"`
			}{Content: "ok"}}},
		})
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCopilot, Endpoint: server.URL, APIKey: "copilot-token"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

// --- Error handling ---

func TestClientChatError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		w.Write([]byte(`{"error":{"message":"invalid api key"}}`))
	}))
	defer server.Close()

	client := NewClient(Config{
		Provider: ProviderCustom,
		Endpoint: server.URL,
		APIKey:   "bad-key",
	})

	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for 401 response")
	}
	if !strings.Contains(err.Error(), "401") {
		t.Errorf("error should contain status code, got: %v", err)
	}
}

func TestClientChatEmptyChoices(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(chatResponse{Choices: nil})
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCustom, Endpoint: server.URL, APIKey: "key"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for empty choices")
	}
	if !strings.Contains(err.Error(), "empty response") {
		t.Errorf("error should mention empty response, got: %v", err)
	}
}

func TestClientChatAPIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// API returns 200 but with error in body
		json.NewEncoder(w).Encode(map[string]interface{}{
			"error":   map[string]string{"message": "rate limited"},
			"choices": []interface{}{},
		})
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCustom, Endpoint: server.URL, APIKey: "key"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for API-level error")
	}
	if !strings.Contains(err.Error(), "rate limited") {
		t.Errorf("error should contain API message, got: %v", err)
	}
}

func TestClientChatMalformedJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{not valid json`))
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCustom, Endpoint: server.URL, APIKey: "key"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for malformed JSON")
	}
	if !strings.Contains(err.Error(), "parse response") {
		t.Errorf("error should mention parse, got: %v", err)
	}
}

func TestClientChatServerError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		w.Write([]byte(`{"error":"internal server error"}`))
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCustom, Endpoint: server.URL, APIKey: "key"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for 500 response")
	}
	if !strings.Contains(err.Error(), "500") {
		t.Errorf("error should contain 500 status, got: %v", err)
	}
}

func TestClientChatForbidden(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		w.Write([]byte(`{"message":"Please only use approved clients"}`))
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderCopilot, Endpoint: server.URL, APIKey: "tok"})
	_, err := client.Chat([]Message{{Role: "user", Content: "test"}})
	if err == nil {
		t.Fatal("expected error for 403 response")
	}
	if !strings.Contains(err.Error(), "403") {
		t.Errorf("error should contain 403, got: %v", err)
	}
}

// --- Request body validation ---

func TestClientChatSendsCorrectBody(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req chatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Fatalf("failed to decode request: %v", err)
		}
		if req.Model != "gpt-4o" {
			t.Errorf("model = %q, want gpt-4o", req.Model)
		}
		if len(req.Messages) != 2 {
			t.Errorf("messages count = %d, want 2", len(req.Messages))
		}
		if req.Messages[0].Role != "system" {
			t.Errorf("first message role = %q, want system", req.Messages[0].Role)
		}
		json.NewEncoder(w).Encode(chatResponse{
			Choices: []struct {
				Message struct {
					Content string `json:"content"`
				} `json:"message"`
			}{{Message: struct {
				Content string `json:"content"`
			}{Content: "ok"}}},
		})
	}))
	defer server.Close()

	client := NewClient(Config{Provider: ProviderOpenAI, Endpoint: server.URL, APIKey: "key", Model: "gpt-4o"})
	_, err := client.Chat([]Message{
		{Role: "system", Content: "You are helpful"},
		{Role: "user", Content: "test"},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

// --- Truncate helper ---

func TestTruncate(t *testing.T) {
	tests := []struct {
		input  string
		maxLen int
		want   string
	}{
		{"short", 10, "short"},
		{"exactly10!", 10, "exactly10!"},
		{"this is longer than ten", 10, "this is lo..."},
		{"", 5, ""},
	}
	for _, tt := range tests {
		got := truncate(tt.input, tt.maxLen)
		if got != tt.want {
			t.Errorf("truncate(%q, %d) = %q, want %q", tt.input, tt.maxLen, got, tt.want)
		}
	}
}

// --- Prompt builders ---

func TestPromptBuilders(t *testing.T) {
	msgs := BuildLogMessages("ERROR: disk full", "What happened?")
	if len(msgs) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(msgs))
	}
	if msgs[0].Role != "system" {
		t.Error("first message should be system")
	}
	if msgs[1].Role != "user" {
		t.Error("second message should be user")
	}
	if !strings.Contains(msgs[1].Content, "ERROR: disk full") {
		t.Error("user message should contain log content")
	}
	if !strings.Contains(msgs[1].Content, "What happened?") {
		t.Error("user message should contain question")
	}

	ruleMsgs := BuildRuleGenMessages("2024-01-01 ERROR test")
	if len(ruleMsgs) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(ruleMsgs))
	}
	if !strings.Contains(ruleMsgs[0].Content, "matchType") {
		t.Error("system prompt should describe rule format")
	}
	if !strings.Contains(ruleMsgs[1].Content, "2024-01-01 ERROR test") {
		t.Error("user message should contain log content")
	}
}

func TestBuildLogMessagesEmptyInputs(t *testing.T) {
	msgs := BuildLogMessages("", "")
	if len(msgs) != 2 {
		t.Fatalf("expected 2 messages even with empty inputs, got %d", len(msgs))
	}
}
