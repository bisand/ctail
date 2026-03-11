package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDefaultSettings(t *testing.T) {
	s := DefaultSettings()
	if s.PollIntervalMs != 500 {
		t.Errorf("expected default poll interval 500ms, got %d", s.PollIntervalMs)
	}
	if s.BufferSize != 10000 {
		t.Errorf("expected default buffer 10000, got %d", s.BufferSize)
	}
	if s.Theme != "dark" {
		t.Errorf("expected dark theme, got %s", s.Theme)
	}
}

func TestDefaultProfile(t *testing.T) {
	p := DefaultProfile()
	if p.Name != "Common Logs" {
		t.Errorf("expected 'Common Logs', got %s", p.Name)
	}
	if len(p.Rules) == 0 {
		t.Error("expected default rules")
	}
}

func TestConfigManagerSaveLoad(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	m, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	// Check default profile exists
	profiles := m.ListProfiles()
	if len(profiles) == 0 {
		t.Fatal("expected at least one profile")
	}

	// Save and reload settings
	s := DefaultSettings()
	s.FontSize = 18
	if err := m.SaveSettings(s); err != nil {
		t.Fatal(err)
	}

	loaded := m.GetSettings()
	if loaded.FontSize != 18 {
		t.Errorf("expected font size 18, got %d", loaded.FontSize)
	}
}

func TestProfileCRUD(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	m, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	// Create
	p := Profile{Name: "Test Profile", Rules: []Rule{
		{ID: "r1", Name: "Test", Pattern: `test`, MatchType: "match", Foreground: "#fff", Enabled: true},
	}}
	if err := m.SaveProfile(p); err != nil {
		t.Fatal(err)
	}

	// Read
	got, ok := m.GetProfile("Test Profile")
	if !ok {
		t.Fatal("profile not found")
	}
	if len(got.Rules) != 1 {
		t.Errorf("expected 1 rule, got %d", len(got.Rules))
	}

	// Rename
	if err := m.RenameProfile("Test Profile", "Renamed"); err != nil {
		t.Fatal(err)
	}
	_, ok = m.GetProfile("Test Profile")
	if ok {
		t.Error("old name should not exist")
	}
	_, ok = m.GetProfile("Renamed")
	if !ok {
		t.Error("renamed profile should exist")
	}

	// Delete
	if err := m.DeleteProfile("Renamed"); err != nil {
		t.Fatal(err)
	}
	_, ok = m.GetProfile("Renamed")
	if ok {
		t.Error("deleted profile should not exist")
	}
}

func TestSanitizeFilename(t *testing.T) {
	tests := []struct {
		input, want string
	}{
		{"Common Logs", "common-logs"},
		{"My Profile!", "my-profile"},
		{"test 123", "test-123"},
		{"", "unnamed"},
	}
	for _, tt := range tests {
		got := sanitizeFilename(tt.input)
		if got != tt.want {
			t.Errorf("sanitizeFilename(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

func TestConfigDirCreated(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	_, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	profileDir := filepath.Join(dir, "ctail", "profiles")
	if _, err := os.Stat(profileDir); os.IsNotExist(err) {
		t.Error("profiles directory should be created")
	}
}
