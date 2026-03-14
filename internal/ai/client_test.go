package ai

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

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
}

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

	ruleMsgs := BuildRuleGenMessages("2024-01-01 ERROR test")
	if len(ruleMsgs) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(ruleMsgs))
	}
}
