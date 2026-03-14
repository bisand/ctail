package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// Copilot editor integrations use this public client ID for the device flow.
const copilotClientID = "Iv1.b507a08c87ecfe98"

// DeviceCodeResponse is returned when initiating the device flow.
type DeviceCodeResponse struct {
	DeviceCode      string `json:"device_code"`
	UserCode        string `json:"user_code"`
	VerificationURI string `json:"verification_uri"`
	ExpiresIn       int    `json:"expires_in"`
	Interval        int    `json:"interval"`
}

// RequestDeviceCode initiates the GitHub OAuth device flow.
// Returns the device code response containing the user code to display.
func RequestDeviceCode() (*DeviceCodeResponse, error) {
	payload, _ := json.Marshal(map[string]string{
		"client_id": copilotClientID,
		"scope":     "read:user",
	})

	req, err := http.NewRequest("POST", "https://github.com/login/device/code", bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("request failed: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("read response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GitHub returned %d: %s", resp.StatusCode, string(body))
	}

	var dcr DeviceCodeResponse
	if err := json.Unmarshal(body, &dcr); err != nil {
		return nil, fmt.Errorf("parse response: %w", err)
	}

	return &dcr, nil
}

// PollForToken polls GitHub until the user completes the device flow authorization.
// Returns the OAuth access token. Blocks until success, expiry, cancel, or error.
func PollForToken(ctx context.Context, deviceCode string, interval int) (string, error) {
	if interval < 5 {
		interval = 5
	}
	ticker := time.NewTicker(time.Duration(interval) * time.Second)
	defer ticker.Stop()

	// Give the user up to 15 minutes
	deadline := time.After(15 * time.Minute)

	for {
		select {
		case <-ctx.Done():
			return "", fmt.Errorf("authorization cancelled")
		case <-deadline:
			return "", fmt.Errorf("device flow timed out — please try again")
		case <-ticker.C:
			token, done, slowDown, err := checkDeviceAuth(deviceCode)
			if err != nil {
				return "", err
			}
			if done {
				return token, nil
			}
			if slowDown {
				// GitHub requires us to increase interval by 5 seconds
				interval += 5
				ticker.Stop()
				ticker = time.NewTicker(time.Duration(interval) * time.Second)
			}
		}
	}
}

// checkDeviceAuth makes a single poll request.
// Returns (token, done, slowDown, err).
func checkDeviceAuth(deviceCode string) (string, bool, bool, error) {
	payload, _ := json.Marshal(map[string]string{
		"client_id":   copilotClientID,
		"device_code": deviceCode,
		"grant_type":  "urn:ietf:params:oauth:grant-type:device_code",
	})

	req, err := http.NewRequest("POST", "https://github.com/login/oauth/access_token", bytes.NewReader(payload))
	if err != nil {
		return "", false, false, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", false, false, fmt.Errorf("poll request failed: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", false, false, fmt.Errorf("read response: %w", err)
	}

	var result struct {
		AccessToken string `json:"access_token"`
		Error       string `json:"error"`
		ErrorDesc   string `json:"error_description"`
	}
	if err := json.Unmarshal(body, &result); err != nil {
		return "", false, false, fmt.Errorf("parse response: %w", err)
	}

	switch result.Error {
	case "":
		if result.AccessToken != "" {
			return result.AccessToken, true, false, nil
		}
		return "", false, false, fmt.Errorf("empty token in response")
	case "authorization_pending":
		return "", false, false, nil
	case "slow_down":
		return "", false, true, nil
	case "expired_token":
		return "", false, false, fmt.Errorf("authorization expired — please try again")
	case "access_denied":
		return "", false, false, fmt.Errorf("authorization denied by user")
	default:
		return "", false, false, fmt.Errorf("GitHub error: %s — %s", result.Error, result.ErrorDesc)
	}
}

// CopilotToken holds a short-lived API token for Copilot.
type CopilotToken struct {
	Token     string `json:"token"`
	ExpiresAt int64  `json:"expires_at"`
}

// ExchangeCopilotToken exchanges a GitHub OAuth token for a short-lived Copilot API token.
func ExchangeCopilotToken(oauthToken string) (*CopilotToken, error) {
	req, err := http.NewRequest("GET", "https://api.github.com/copilot_internal/v2/token", nil)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Authorization", "token "+oauthToken)
	req.Header.Set("Accept", "application/json")

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("token exchange failed: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("read response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("token exchange returned %d: %s", resp.StatusCode, string(body))
	}

	var ct CopilotToken
	if err := json.Unmarshal(body, &ct); err != nil {
		return nil, fmt.Errorf("parse token: %w", err)
	}
	if ct.Token == "" {
		return nil, fmt.Errorf("empty token in exchange response")
	}
	return &ct, nil
}
