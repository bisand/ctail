package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

// Manager handles loading and saving of settings and profiles
type Manager struct {
	mu         sync.RWMutex
	configDir  string
	settings   AppSettings
	profiles   map[string]Profile
	themes     map[string]Theme
}

// NewManager creates a config manager using the platform config directory
func NewManager() (*Manager, error) {
	dir, err := configDir()
	if err != nil {
		return nil, fmt.Errorf("config dir: %w", err)
	}

	fmt.Fprintf(os.Stderr, "ctail: config directory: %s\n", dir)

	m := &Manager{
		configDir: dir,
		settings:  DefaultSettings(),
		profiles:  make(map[string]Profile),
		themes:    make(map[string]Theme),
	}

	if err := os.MkdirAll(filepath.Join(dir, "profiles"), 0755); err != nil {
		return nil, fmt.Errorf("create config dirs: %w", err)
	}
	if err := os.MkdirAll(filepath.Join(dir, "themes"), 0755); err != nil {
		return nil, fmt.Errorf("create themes dir: %w", err)
	}

	m.loadSettings()
	m.loadProfiles()
	m.loadThemes()

	return m, nil
}

func configDir() (string, error) {
	if runtime.GOOS == "windows" {
		appdata := os.Getenv("APPDATA")
		if appdata == "" {
			home, err := os.UserHomeDir()
			if err != nil {
				return "", err
			}
			appdata = filepath.Join(home, "AppData", "Roaming")
		}
		return filepath.Join(appdata, "ctail"), nil
	}
	// Linux / other
	xdg := os.Getenv("XDG_CONFIG_HOME")
	if xdg == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		xdg = filepath.Join(home, ".config")
	}
	return filepath.Join(xdg, "ctail"), nil
}

// GetConfigDir returns the config directory path for diagnostics
func (m *Manager) GetConfigDir() string {
	return m.configDir
}

// GetSettings returns a copy of current settings
func (m *Manager) GetSettings() AppSettings {
	m.mu.RLock()
	defer m.mu.RUnlock()
	s := m.settings
	s.PollIntervalMs = int(s.PollInterval / time.Millisecond)
	// Deep-copy slices so callers cannot mutate shared state
	if s.Tabs != nil {
		s.Tabs = append([]TabState(nil), s.Tabs...)
	}
	if s.RecentFiles != nil {
		s.RecentFiles = append([]string(nil), s.RecentFiles...)
	}
	return s
}

// SaveSettings persists settings to disk.
// Tabs are always preserved from the current in-memory state to prevent
// concurrent callers from accidentally overwriting tab metadata.
// Use SaveSettingsWithTabs to explicitly update tabs.
func (m *Manager) SaveSettings(s AppSettings) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	// Preserve current tabs — only SaveSettingsWithTabs may change them
	s.Tabs = m.settings.Tabs
	return m.saveSettingsLocked(s)
}

// SaveSettingsWithTabs persists settings including an updated Tabs slice.
func (m *Manager) SaveSettingsWithTabs(s AppSettings) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.saveSettingsLocked(s)
}

func (m *Manager) saveSettingsLocked(s AppSettings) error {
	s.PollInterval = time.Duration(s.PollIntervalMs) * time.Millisecond
	if s.PollInterval < 100*time.Millisecond {
		s.PollInterval = 100 * time.Millisecond
		s.PollIntervalMs = 100
	}
	if s.BufferSize < 1000 {
		s.BufferSize = 1000
	}
	if s.ReadTimeoutSec < 5 {
		s.ReadTimeoutSec = 5
	} else if s.ReadTimeoutSec > 120 {
		s.ReadTimeoutSec = 120
	}
	m.settings = s
	return m.writeJSON(filepath.Join(m.configDir, "settings.json"), s)
}

// ListProfiles returns all profile names
func (m *Manager) ListProfiles() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	names := make([]string, 0, len(m.profiles))
	for name := range m.profiles {
		names = append(names, name)
	}
	return names
}

// GetProfile returns a profile by name
func (m *Manager) GetProfile(name string) (Profile, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	p, ok := m.profiles[name]
	return p, ok
}

// SaveProfile creates or updates a profile
func (m *Manager) SaveProfile(p Profile) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.profiles[p.Name] = p
	filename := sanitizeFilename(p.Name) + ".json"
	return m.writeJSON(filepath.Join(m.configDir, "profiles", filename), p)
}

// DeleteProfile removes a profile
func (m *Manager) DeleteProfile(name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.profiles, name)
	filename := sanitizeFilename(name) + ".json"
	path := filepath.Join(m.configDir, "profiles", filename)
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

// RenameProfile renames an existing profile
func (m *Manager) RenameProfile(oldName, newName string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	p, ok := m.profiles[oldName]
	if !ok {
		return fmt.Errorf("profile %q not found", oldName)
	}
	// Remove old
	delete(m.profiles, oldName)
	oldFile := filepath.Join(m.configDir, "profiles", sanitizeFilename(oldName)+".json")
	_ = os.Remove(oldFile)

	// Save new
	p.Name = newName
	m.profiles[newName] = p
	newFile := filepath.Join(m.configDir, "profiles", sanitizeFilename(newName)+".json")
	return m.writeJSON(newFile, p)
}

func (m *Manager) loadSettings() {
	path := filepath.Join(m.configDir, "settings.json")
	data, err := os.ReadFile(path)
	if err != nil || len(data) == 0 {
		// Main file missing or empty — try the temp file left by an
		// interrupted atomic write (process killed before rename).
		tmp := path + ".tmp"
		data, err = os.ReadFile(tmp)
		if err != nil || len(data) == 0 {
			fmt.Fprintf(os.Stderr, "ctail: no settings file found at %s\n", path)
			return
		}
		fmt.Fprintf(os.Stderr, "ctail: recovered settings from %s\n", tmp)
		// Recover: promote tmp to main file
		_ = os.Rename(tmp, path)
	}
	// Strip UTF-8 BOM if present (Windows Notepad adds this)
	data = stripBOM(data)
	var s AppSettings
	if err := json.Unmarshal(data, &s); err != nil {
		fmt.Fprintf(os.Stderr, "ctail: failed to parse %s: %v\n", path, err)
		fmt.Fprintf(os.Stderr, "ctail: settings file size: %d bytes, first 200 chars: %s\n", len(data), truncateBytes(data, 200))
		return
	}
	fmt.Fprintf(os.Stderr, "ctail: loaded settings from %s (%d tabs, profile=%q)\n", path, len(s.Tabs), s.ActiveProfile)
	s.PollInterval = time.Duration(s.PollIntervalMs) * time.Millisecond
	// Migrate old theme setting ("dark"/"light") to new format
	if s.Theme == "dark" || s.Theme == "light" {
		s.ThemeMode = s.Theme
		s.Theme = "catppuccin"
	}
	if s.ThemeMode == "" {
		s.ThemeMode = "dark"
	}
	if s.Theme == "" {
		s.Theme = "catppuccin"
	}
	if s.ScrollSpeed == 0 {
		s.ScrollSpeed = 1
	}
	if s.ReadTimeoutSec == 0 {
		s.ReadTimeoutSec = 30
	}
	m.settings = s
}

func (m *Manager) loadProfiles() {
	profileDir := filepath.Join(m.configDir, "profiles")
	entries, err := os.ReadDir(profileDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ctail: cannot read profiles dir %s: %v\n", profileDir, err)
		// Create default profile
		def := DefaultProfile()
		m.profiles[def.Name] = def
		_ = m.writeJSON(filepath.Join(profileDir, "common-logs.json"), def)
		return
	}

	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		filePath := filepath.Join(profileDir, e.Name())
		data, err := os.ReadFile(filePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "ctail: cannot read profile %s: %v\n", filePath, err)
			continue
		}
		data = stripBOM(data)
		var p Profile
		if err := json.Unmarshal(data, &p); err != nil {
			fmt.Fprintf(os.Stderr, "ctail: failed to parse profile %s: %v\n", filePath, err)
			continue
		}
		if p.Name == "" {
			fmt.Fprintf(os.Stderr, "ctail: skipping profile %s: empty name\n", filePath)
			continue
		}
		m.profiles[p.Name] = p
		fmt.Fprintf(os.Stderr, "ctail: loaded profile %q from %s (%d rules)\n", p.Name, e.Name(), len(p.Rules))
	}

	if len(m.profiles) == 0 {
		fmt.Fprintf(os.Stderr, "ctail: no profiles found, creating default\n")
		def := DefaultProfile()
		m.profiles[def.Name] = def
		_ = m.writeJSON(filepath.Join(profileDir, "common-logs.json"), def)
	}
}

func (m *Manager) writeJSON(path string, v interface{}) error {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	// Atomic write: write to a temp file in the same directory, then rename.
	// This prevents data loss if the process is killed mid-write (common on
	// Windows shutdown) which would otherwise truncate/empty the target file.
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

func truncateBytes(data []byte, n int) string {
	if len(data) <= n {
		return string(data)
	}
	return string(data[:n]) + "..."
}

// stripBOM removes a UTF-8 BOM prefix if present.
// Windows Notepad and some editors add this, which breaks JSON parsing.
func stripBOM(data []byte) []byte {
	if len(data) >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
		return data[3:]
	}
	return data
}

func sanitizeFilename(name string) string {
	name = strings.ToLower(name)
	name = strings.Map(func(r rune) rune {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') || r == '-' || r == '_' {
			return r
		}
		if r == ' ' {
			return '-'
		}
		return -1
	}, name)
	if name == "" {
		name = "unnamed"
	}
	return name
}

func (m *Manager) loadThemes() {
	// Load built-in themes first
	for _, t := range BuiltInThemes() {
		m.themes[t.Name] = t
	}

	// Load custom themes from config dir (override built-ins with same name)
	themeDir := filepath.Join(m.configDir, "themes")
	entries, err := os.ReadDir(themeDir)
	if err != nil {
		return
	}
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(themeDir, e.Name()))
		if err != nil {
			continue
		}
		data = stripBOM(data)
		var t Theme
		if json.Unmarshal(data, &t) == nil && t.Name != "" {
			t.BuiltIn = false
			m.themes[t.Name] = t
		}
	}
}

// ListThemes returns all available themes
func (m *Manager) ListThemes() []Theme {
	m.mu.RLock()
	defer m.mu.RUnlock()
	themes := make([]Theme, 0, len(m.themes))
	for _, t := range m.themes {
		themes = append(themes, t)
	}
	return themes
}

// GetTheme returns a specific theme by name
func (m *Manager) GetTheme(name string) (Theme, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	t, ok := m.themes[name]
	return t, ok
}

// SaveTheme saves a custom theme to disk
func (m *Manager) SaveTheme(t Theme) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	t.BuiltIn = false
	m.themes[t.Name] = t
	filename := sanitizeFilename(t.Name) + ".json"
	return m.writeJSON(filepath.Join(m.configDir, "themes", filename), t)
}

// DeleteTheme removes a custom theme
func (m *Manager) DeleteTheme(name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	t, ok := m.themes[name]
	if !ok {
		return fmt.Errorf("theme %q not found", name)
	}
	if t.BuiltIn {
		return fmt.Errorf("cannot delete built-in theme %q", name)
	}
	delete(m.themes, name)
	filename := sanitizeFilename(name) + ".json"
	path := filepath.Join(m.configDir, "themes", filename)
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}
