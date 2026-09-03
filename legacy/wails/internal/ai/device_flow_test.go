package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func TestCheckDeviceAuthSuccess(t *testing.T) {
	token, done, slowDown, err := parseDeviceAuthResponse(
		`{"access_token":"gho_test123","token_type":"bearer","scope":"read:user"}`,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !done {
		t.Error("expected done=true")
	}
	if slowDown {
		t.Error("expected slowDown=false")
	}
	if token != "gho_test123" {
		t.Errorf("token = %q, want gho_test123", token)
	}
}

func TestCheckDeviceAuthPending(t *testing.T) {
	token, done, slowDown, err := parseDeviceAuthResponse(
		`{"error":"authorization_pending","error_description":"still waiting"}`,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if done {
		t.Error("expected done=false")
	}
	if slowDown {
		t.Error("expected slowDown=false")
	}
	if token != "" {
		t.Errorf("token should be empty, got %q", token)
	}
}

func TestCheckDeviceAuthSlowDown(t *testing.T) {
	_, done, slowDown, err := parseDeviceAuthResponse(
		`{"error":"slow_down","error_description":"please slow down"}`,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if done {
		t.Error("expected done=false")
	}
	if !slowDown {
		t.Error("expected slowDown=true")
	}
}

func TestCheckDeviceAuthExpired(t *testing.T) {
	_, _, _, err := parseDeviceAuthResponse(
		`{"error":"expired_token","error_description":"token expired"}`,
	)
	if err == nil {
		t.Fatal("expected error for expired token")
	}
	if got := err.Error(); got != "authorization expired — please try again" {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestCheckDeviceAuthAccessDenied(t *testing.T) {
	_, _, _, err := parseDeviceAuthResponse(
		`{"error":"access_denied","error_description":"user denied"}`,
	)
	if err == nil {
		t.Fatal("expected error for access denied")
	}
	if got := err.Error(); got != "authorization denied by user" {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestCheckDeviceAuthUnknownError(t *testing.T) {
	_, _, _, err := parseDeviceAuthResponse(
		`{"error":"something_weird","error_description":"unexpected"}`,
	)
	if err == nil {
		t.Fatal("expected error for unknown error code")
	}
}

func TestCheckDeviceAuthEmptyToken(t *testing.T) {
	_, _, _, err := parseDeviceAuthResponse(
		`{"access_token":""}`,
	)
	if err == nil {
		t.Fatal("expected error for empty token")
	}
}

func TestCheckDeviceAuthMalformedJSON(t *testing.T) {
	_, _, _, err := parseDeviceAuthResponse(`{not json}`)
	if err == nil {
		t.Fatal("expected error for malformed JSON")
	}
}

// parseDeviceAuthResponse is a test helper that parses the same JSON
// structure as checkDeviceAuth without making an HTTP request.
func parseDeviceAuthResponse(body string) (string, bool, bool, error) {
	var result struct {
		AccessToken string `json:"access_token"`
		Error       string `json:"error"`
		ErrorDesc   string `json:"error_description"`
	}
	if err := json.Unmarshal([]byte(body), &result); err != nil {
		return "", false, false, err
	}

	switch result.Error {
	case "":
		if result.AccessToken != "" {
			return result.AccessToken, true, false, nil
		}
		return "", false, false, errEmpty
	case "authorization_pending":
		return "", false, false, nil
	case "slow_down":
		return "", false, true, nil
	case "expired_token":
		return "", false, false, errExpired
	case "access_denied":
		return "", false, false, errDenied
	default:
		return "", false, false, errUnknown(result.Error, result.ErrorDesc)
	}
}

var (
	errEmpty   = errorf("empty token in response")
	errExpired = errorf("authorization expired — please try again")
	errDenied  = errorf("authorization denied by user")
)

func errorf(msg string) error              { return &staticError{msg} }
func errUnknown(e, d string) error         { return &staticError{"GitHub error: " + e + " — " + d} }

type staticError struct{ msg string }

func (e *staticError) Error() string { return e.msg }

// --- PollForToken tests (with mock server) ---

func TestPollForTokenSuccess(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		n := calls.Add(1)
		if n < 3 {
			json.NewEncoder(w).Encode(map[string]string{"error": "authorization_pending"})
			return
		}
		json.NewEncoder(w).Encode(map[string]string{"access_token": "gho_success"})
	}))
	defer server.Close()

	origPoll := pollEndpoint
	origMin := minPollInterval
	pollEndpoint = server.URL
	minPollInterval = 1 // speed up test
	defer func() { pollEndpoint = origPoll; minPollInterval = origMin }()

	ctx := context.Background()
	token, err := PollForToken(ctx, "test-device-code", 1)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if token != "gho_success" {
		t.Errorf("token = %q, want gho_success", token)
	}
	if calls.Load() < 3 {
		t.Errorf("expected at least 3 poll calls, got %d", calls.Load())
	}
}

func TestPollForTokenCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(map[string]string{"error": "authorization_pending"})
	}))
	defer server.Close()

	origPoll := pollEndpoint
	origMin := minPollInterval
	pollEndpoint = server.URL
	minPollInterval = 1
	defer func() { pollEndpoint = origPoll; minPollInterval = origMin }()

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(200 * time.Millisecond)
		cancel()
	}()

	_, err := PollForToken(ctx, "test-device-code", 1)
	if err == nil {
		t.Fatal("expected error on cancellation")
	}
	if got := err.Error(); got != "authorization cancelled" {
		t.Errorf("error = %q, want 'authorization cancelled'", got)
	}
}

// --- ExchangeCopilotToken tests ---

func TestExchangeCopilotTokenSuccess(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" {
			t.Errorf("method = %s, want GET", r.Method)
		}
		if r.Header.Get("Authorization") != "token gho_test" {
			t.Errorf("auth = %q, want 'token gho_test'", r.Header.Get("Authorization"))
		}
		// Verify Copilot headers are set
		if r.Header.Get("Editor-Version") == "" {
			t.Error("missing Editor-Version header")
		}
		if r.Header.Get("Copilot-Integration-Id") == "" {
			t.Error("missing Copilot-Integration-Id header")
		}
		json.NewEncoder(w).Encode(CopilotToken{
			Token:     "tid_copilot_token",
			ExpiresAt: time.Now().Add(30 * time.Minute).Unix(),
		})
	}))
	defer server.Close()

	origExchange := exchangeEndpoint
	exchangeEndpoint = server.URL
	defer func() { exchangeEndpoint = origExchange }()

	ct, err := ExchangeCopilotToken("gho_test")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ct.Token != "tid_copilot_token" {
		t.Errorf("token = %q, want tid_copilot_token", ct.Token)
	}
}

func TestExchangeCopilotTokenForbidden(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		w.Write([]byte(`{"message":"Please only use approved clients"}`))
	}))
	defer server.Close()

	origExchange := exchangeEndpoint
	exchangeEndpoint = server.URL
	defer func() { exchangeEndpoint = origExchange }()

	_, err := ExchangeCopilotToken("gho_test")
	if err == nil {
		t.Fatal("expected error for 403")
	}
	if got := err.Error(); !contains(got, "403") {
		t.Errorf("error should mention 403, got: %v", err)
	}
}

func TestExchangeCopilotTokenEmptyToken(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(CopilotToken{Token: "", ExpiresAt: 0})
	}))
	defer server.Close()

	origExchange := exchangeEndpoint
	exchangeEndpoint = server.URL
	defer func() { exchangeEndpoint = origExchange }()

	_, err := ExchangeCopilotToken("gho_test")
	if err == nil {
		t.Fatal("expected error for empty token")
	}
}

// --- setCopilotHeaders ---

func TestSetCopilotHeaders(t *testing.T) {
	req, _ := http.NewRequest("GET", "http://example.com", nil)
	setCopilotHeaders(req)

	checks := map[string]string{
		"Editor-Version":        "vscode/1.100.0",
		"Editor-Plugin-Version": "copilot/1.300.0",
		"User-Agent":            "GithubCopilot/1.300.0",
		"Copilot-Integration-Id": "vscode-chat",
	}
	for header, want := range checks {
		if got := req.Header.Get(header); got != want {
			t.Errorf("%s = %q, want %q", header, got, want)
		}
	}
}

// --- truncateStr ---

func TestTruncateStr(t *testing.T) {
	if got := truncateStr("hello", 10); got != "hello" {
		t.Errorf("truncateStr short = %q", got)
	}
	if got := truncateStr("hello world", 5); got != "hello..." {
		t.Errorf("truncateStr long = %q", got)
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsHelper(s, substr))
}

func containsHelper(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
