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
	if s.Theme != "catppuccin" {
		t.Errorf("expected catppuccin theme, got %s", s.Theme)
	}
	if s.ThemeMode != "dark" {
		t.Errorf("expected dark theme mode, got %s", s.ThemeMode)
	}
	if s.DisplayBackend != "auto" {
		t.Errorf("expected auto display backend, got %s", s.DisplayBackend)
	}
	if !s.CheckForUpdates {
		t.Error("expected CheckForUpdates to default to true")
	}
	if s.Window.Width != 1200 || s.Window.Height != 800 {
		t.Errorf("expected default window 1200x800, got %dx%d", s.Window.Width, s.Window.Height)
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

	themeDir := filepath.Join(dir, "ctail", "themes")
	if _, err := os.Stat(themeDir); os.IsNotExist(err) {
		t.Error("themes directory should be created")
	}
}

func TestBuiltInThemes(t *testing.T) {
	themes := BuiltInThemes()
	if len(themes) < 5 {
		t.Errorf("expected at least 5 built-in themes, got %d", len(themes))
	}
	// Verify catppuccin is present and has colors
	found := false
	for _, th := range themes {
		if th.Name == "catppuccin" {
			found = true
			if th.Dark.BgPrimary == "" || th.Light.BgPrimary == "" {
				t.Error("catppuccin theme should have dark and light colors")
			}
		}
	}
	if !found {
		t.Error("catppuccin theme not found in built-ins")
	}
}

func TestThemeListAndGet(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	m, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	themes := m.ListThemes()
	if len(themes) < 5 {
		t.Errorf("expected at least 5 themes, got %d", len(themes))
	}

	th, ok := m.GetTheme("nord")
	if !ok {
		t.Fatal("nord theme should exist")
	}
	if th.DisplayName != "Nord" {
		t.Errorf("expected display name 'Nord', got %s", th.DisplayName)
	}
}

func TestThemeMigration(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	// Write old-style settings with theme: "dark"
	configPath := filepath.Join(dir, "ctail")
	os.MkdirAll(filepath.Join(configPath, "profiles"), 0755)
	os.WriteFile(filepath.Join(configPath, "settings.json"),
		[]byte(`{"pollIntervalMs":500,"bufferSize":10000,"theme":"dark","fontSize":14}`), 0644)

	m, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	s := m.GetSettings()
	if s.Theme != "catppuccin" {
		t.Errorf("expected migrated theme 'catppuccin', got %s", s.Theme)
	}
	if s.ThemeMode != "dark" {
		t.Errorf("expected migrated mode 'dark', got %s", s.ThemeMode)
	}
}

func TestCustomThemeSaveDelete(t *testing.T) {
	dir := t.TempDir()
	os.Setenv("XDG_CONFIG_HOME", dir)
	defer os.Unsetenv("XDG_CONFIG_HOME")

	m, err := NewManager()
	if err != nil {
		t.Fatal(err)
	}

	custom := Theme{
		Name:        "my-theme",
		DisplayName: "My Theme",
		Dark:        ThemeColors{BgPrimary: "#000000", TextPrimary: "#ffffff"},
		Light:       ThemeColors{BgPrimary: "#ffffff", TextPrimary: "#000000"},
	}

	if err := m.SaveTheme(custom); err != nil {
		t.Fatal(err)
	}

	th, ok := m.GetTheme("my-theme")
	if !ok {
		t.Fatal("custom theme should exist after save")
	}
	if th.BuiltIn {
		t.Error("saved theme should not be marked as built-in")
	}

	if err := m.DeleteTheme("my-theme"); err != nil {
		t.Fatal(err)
	}
	_, ok = m.GetTheme("my-theme")
	if ok {
		t.Error("deleted theme should not exist")
	}

	// Cannot delete built-in
	if err := m.DeleteTheme("catppuccin"); err == nil {
		t.Error("should not be able to delete built-in theme")
	}
}
