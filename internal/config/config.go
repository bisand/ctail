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
}

// NewManager creates a config manager using the platform config directory
func NewManager() (*Manager, error) {
	dir, err := configDir()
	if err != nil {
		return nil, fmt.Errorf("config dir: %w", err)
	}

	m := &Manager{
		configDir: dir,
		settings:  DefaultSettings(),
		profiles:  make(map[string]Profile),
	}

	if err := os.MkdirAll(filepath.Join(dir, "profiles"), 0755); err != nil {
		return nil, fmt.Errorf("create config dirs: %w", err)
	}

	m.loadSettings()
	m.loadProfiles()

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

// GetSettings returns a copy of current settings
func (m *Manager) GetSettings() AppSettings {
	m.mu.RLock()
	defer m.mu.RUnlock()
	s := m.settings
	s.PollIntervalMs = int(s.PollInterval / time.Millisecond)
	return s
}

// SaveSettings persists settings to disk
func (m *Manager) SaveSettings(s AppSettings) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	s.PollInterval = time.Duration(s.PollIntervalMs) * time.Millisecond
	if s.PollInterval < 100*time.Millisecond {
		s.PollInterval = 100 * time.Millisecond
		s.PollIntervalMs = 100
	}
	if s.BufferSize < 1000 {
		s.BufferSize = 1000
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
	if err != nil {
		return
	}
	var s AppSettings
	if json.Unmarshal(data, &s) == nil {
		s.PollInterval = time.Duration(s.PollIntervalMs) * time.Millisecond
		m.settings = s
	}
}

func (m *Manager) loadProfiles() {
	profileDir := filepath.Join(m.configDir, "profiles")
	entries, err := os.ReadDir(profileDir)
	if err != nil {
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
		data, err := os.ReadFile(filepath.Join(profileDir, e.Name()))
		if err != nil {
			continue
		}
		var p Profile
		if json.Unmarshal(data, &p) == nil && p.Name != "" {
			m.profiles[p.Name] = p
		}
	}

	if len(m.profiles) == 0 {
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
	return os.WriteFile(path, data, 0644)
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
