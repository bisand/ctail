package ai

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Copilot uses this public client ID for the device flow (updated 2025).
const copilotClientID = "Iv23li1BMMe2RGAuhf8j"

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
	data := url.Values{
		"client_id": {copilotClientID},
		"scope":     {"copilot"},
	}

	req, err := http.NewRequest("POST", "https://github.com/login/device/code", strings.NewReader(data.Encode()))
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
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
// Returns the OAuth access token. Blocks until success, expiry, or error.
func PollForToken(deviceCode string, interval int) (string, error) {
	if interval < 5 {
		interval = 5
	}
	ticker := time.NewTicker(time.Duration(interval) * time.Second)
	defer ticker.Stop()

	// Give the user up to 15 minutes
	deadline := time.After(15 * time.Minute)

	for {
		select {
		case <-deadline:
			return "", fmt.Errorf("device flow timed out — please try again")
		case <-ticker.C:
			token, done, err := checkDeviceAuth(deviceCode)
			if err != nil {
				return "", err
			}
			if done {
				return token, nil
			}
		}
	}
}

// checkDeviceAuth makes a single poll request.
// Returns (token, true, nil) on success, ("", false, nil) if still pending.
func checkDeviceAuth(deviceCode string) (string, bool, error) {
	data := url.Values{
		"client_id":   {copilotClientID},
		"device_code": {deviceCode},
		"grant_type":  {"urn:ietf:params:oauth:grant-type:device_code"},
	}

	req, err := http.NewRequest("POST", "https://github.com/login/oauth/access_token", strings.NewReader(data.Encode()))
	if err != nil {
		return "", false, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", false, fmt.Errorf("poll request failed: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", false, fmt.Errorf("read response: %w", err)
	}

	var result struct {
		AccessToken string `json:"access_token"`
		Error       string `json:"error"`
		ErrorDesc   string `json:"error_description"`
	}
	if err := json.Unmarshal(body, &result); err != nil {
		return "", false, fmt.Errorf("parse response: %w", err)
	}

	switch result.Error {
	case "":
		if result.AccessToken != "" {
			return result.AccessToken, true, nil
		}
		return "", false, fmt.Errorf("empty token in response")
	case "authorization_pending":
		return "", false, nil // still waiting
	case "slow_down":
		return "", false, nil // will respect interval
	case "expired_token":
		return "", false, fmt.Errorf("authorization expired — please try again")
	case "access_denied":
		return "", false, fmt.Errorf("authorization denied by user")
	default:
		return "", false, fmt.Errorf("GitHub error: %s — %s", result.Error, result.ErrorDesc)
	}
}
