package main

import (
	"context"
	"ctail/internal/config"
	"ctail/internal/rules"
	"ctail/internal/tailer"
	"fmt"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"
	"time"

	"github.com/wailsapp/wails/v2/pkg/menu"
	wailsRuntime "github.com/wailsapp/wails/v2/pkg/runtime"
)

// TabInfo holds a tailer and its metadata
type TabInfo struct {
	ID       string `json:"id"`
	FilePath string `json:"filePath"`
	FileName string `json:"fileName"`
	Profile  string `json:"profile"`
	tailer   *tailer.Tailer
}

// App is the main application struct bound to Wails
type App struct {
	ctx             context.Context
	config          *config.Manager
	preloadedConfig *config.Manager
	mu              sync.RWMutex
	tabs            map[string]*TabInfo
	nextID          int
	recentMenu      *menu.Menu
	buildNumber     string
}

// NewApp creates a new App
func NewApp(buildNum string) *App {
	return &App{
		tabs:        make(map[string]*TabInfo),
		buildNumber: buildNum,
	}
}

func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	if a.preloadedConfig != nil {
		a.config = a.preloadedConfig
		a.preloadedConfig = nil
	} else {
		cfg, err := config.NewManager()
		if err != nil {
			fmt.Println("Warning: could not initialize config:", err)
			return
		}
		a.config = cfg
	}
	a.RefreshRecentMenu()
}

func (a *App) shutdown(ctx context.Context) {
	// Tab order is persisted by the frontend via SaveTabOrder on every change.

	// Stop tailers with a timeout to avoid hanging on stale remote mounts
	a.mu.RLock()
	tabsCopy := make([]*TabInfo, 0, len(a.tabs))
	for _, tab := range a.tabs {
		tabsCopy = append(tabsCopy, tab)
	}
	a.mu.RUnlock()

	done := make(chan struct{})
	go func() {
		for _, tab := range tabsCopy {
			if tab.tailer != nil {
				tab.tailer.Stop()
			}
		}
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(3 * time.Second):
		fmt.Println("Warning: shutdown timed out waiting for tailers to stop")
	}
}

// OpenFileDialog opens a native file dialog and returns the selected path.
// defaultDir is optional — when non-empty the dialog starts in that directory.
func (a *App) OpenFileDialog(defaultDir string) (string, error) {
	return wailsRuntime.OpenFileDialog(a.ctx, wailsRuntime.OpenDialogOptions{
		Title:            "Open Log File",
		DefaultDirectory: defaultDir,
		Filters: []wailsRuntime.FileFilter{
			{DisplayName: "Log Files", Pattern: "*.log;*.txt;*.out"},
			{DisplayName: "All Files", Pattern: "*"},
		},
	})
}

// RevealInFileManager opens the system file manager showing the directory
// that contains the given file path.
func (a *App) RevealInFileManager(filePath string) error {
	dir := filepath.Dir(filePath)
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "linux":
		cmd = exec.Command("xdg-open", dir)
	case "darwin":
		cmd = exec.Command("open", "-R", filePath)
	case "windows":
		cmd = exec.Command("explorer", "/select,", filePath)
	default:
		return fmt.Errorf("unsupported platform: %s", runtime.GOOS)
	}
	return cmd.Start()
}

// OpenTab opens a new tab tailing the given file.
// Returns immediately — file I/O runs in the background.
// The frontend receives tailer:ready or tailer:error events.
func (a *App) OpenTab(filePath string) (string, error) {
	if filePath == "" {
		return "", fmt.Errorf("no file path provided")
	}

	// If the file is already open, focus that tab instead
	a.mu.RLock()
	for _, tab := range a.tabs {
		if tab.FilePath == filePath {
			a.mu.RUnlock()
			wailsRuntime.EventsEmit(a.ctx, "tab:focus", tab.ID)
			return tab.ID, nil
		}
	}
	a.mu.RUnlock()

	a.mu.Lock()
	a.nextID++
	id := fmt.Sprintf("tab-%d", a.nextID)
	a.mu.Unlock()

	settings := a.config.GetSettings()
	pollInterval := time.Duration(settings.PollIntervalMs) * time.Millisecond
	if pollInterval < 100*time.Millisecond {
		pollInterval = 500 * time.Millisecond
	}

	t := tailer.New(filePath, pollInterval, settings.BufferSize)

	tab := &TabInfo{
		ID:       id,
		FilePath: filePath,
		FileName: filepath.Base(filePath),
		Profile:  "Common Logs",
		tailer:   t,
	}

	// Set up callbacks
	t.OnLines(func(lines []tailer.Line) {
		wailsRuntime.EventsEmit(a.ctx, "tailer:lines", map[string]interface{}{
			"tabId": id,
			"lines": lines,
		})
	})

	t.OnTruncated(func() {
		wailsRuntime.EventsEmit(a.ctx, "tailer:truncated", map[string]interface{}{
			"tabId": id,
		})
	})

	t.OnError(func(err error) {
		wailsRuntime.EventsEmit(a.ctx, "tailer:error", map[string]interface{}{
			"tabId":   id,
			"message": err.Error(),
		})
	})

	t.OnReady(func() {
		wailsRuntime.EventsEmit(a.ctx, "tailer:ready", map[string]interface{}{
			"tabId": id,
		})
	})

	// Register tab immediately so it appears in the UI
	a.mu.Lock()
	a.tabs[id] = tab
	a.mu.Unlock()

	// Start tailing in the background — never blocks
	if err := t.Start(); err != nil {
		return id, nil // still return tab id — error will come via event
	}

	// Track in recent files
	a.AddRecentFile(filePath)

	return id, nil
}

// CloseTab stops tailing and removes the tab (non-blocking)
func (a *App) CloseTab(tabID string) {
	a.mu.Lock()
	tab, ok := a.tabs[tabID]
	if ok {
		delete(a.tabs, tabID)
	}
	a.mu.Unlock()

	if ok && tab.tailer != nil {
		go tab.tailer.Stop()
	}
}

// GetTabLines returns the current buffered lines for a tab
func (a *App) GetTabLines(tabID string) []tailer.Line {
	a.mu.RLock()
	tab, ok := a.tabs[tabID]
	a.mu.RUnlock()
	if !ok {
		return nil
	}
	return tab.tailer.GetLines()
}

// GetTabLineRange reads lines from a file starting at startLine (1-based), returning up to count lines
func (a *App) GetTabLineRange(tabID string, startLine int64, count int) []tailer.Line {
	a.mu.RLock()
	tab, ok := a.tabs[tabID]
	a.mu.RUnlock()
	if !ok {
		return nil
	}
	return tab.tailer.ReadRange(startLine, count)
}

// GetTabTotalLines returns the total number of lines known in the file for a tab
func (a *App) GetTabTotalLines(tabID string) int64 {
	a.mu.RLock()
	tab, ok := a.tabs[tabID]
	a.mu.RUnlock()
	if !ok {
		return 0
	}
	return tab.tailer.GetTotalLines()
}

// GetTabs returns info about all open tabs
func (a *App) GetTabs() []TabInfo {
	a.mu.RLock()
	defer a.mu.RUnlock()
	result := make([]TabInfo, 0, len(a.tabs))
	for _, tab := range a.tabs {
		result = append(result, TabInfo{
			ID:       tab.ID,
			FilePath: tab.FilePath,
			FileName: tab.FileName,
			Profile:  tab.Profile,
		})
	}
	return result
}

// SetTabProfile changes the highlighting profile for a tab
func (a *App) SetTabProfile(tabID, profileName string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if tab, ok := a.tabs[tabID]; ok {
		tab.Profile = profileName
	}
}

// --- Config API ---

// GetSavedTabs returns previously open tabs for restoration
func (a *App) GetSavedTabs() []config.TabState {
	if a.config == nil {
		return nil
	}
	settings := a.config.GetSettings()
	if !settings.RestoreTabs {
		return nil
	}
	return settings.Tabs
}

// SaveTabOrder persists the current tab list in display order.
// Called from the frontend whenever tabs are opened, closed, or reordered.
func (a *App) SaveTabOrder(tabStates []config.TabState) {
	if a.config == nil {
		return
	}
	settings := a.config.GetSettings()
	settings.Tabs = tabStates
	_ = a.config.SaveSettings(settings)
}

// GetSettings returns app settings
func (a *App) GetSettings() config.AppSettings {
	if a.config == nil {
		return config.DefaultSettings()
	}
	return a.config.GetSettings()
}

// SaveSettings saves app settings
func (a *App) SaveSettings(s config.AppSettings) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.SaveSettings(s)
}

// ListProfiles returns available profile names
func (a *App) ListProfiles() []string {
	if a.config == nil {
		return []string{"Common Logs"}
	}
	return a.config.ListProfiles()
}

// GetProfile returns a profile by name
func (a *App) GetProfile(name string) *config.Profile {
	if a.config == nil {
		def := config.DefaultProfile()
		return &def
	}
	p, ok := a.config.GetProfile(name)
	if !ok {
		return nil
	}
	return &p
}

// SaveProfile saves a profile
func (a *App) SaveProfile(p config.Profile) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.SaveProfile(p)
}

// DeleteProfile removes a profile
func (a *App) DeleteProfile(name string) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.DeleteProfile(name)
}

// RenameProfile renames a profile
func (a *App) RenameProfile(oldName, newName string) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.RenameProfile(oldName, newName)
}

// ValidateRegex checks if a regex pattern is valid, returns error message or empty string
func (a *App) ValidateRegex(pattern string) string {
	err := rules.ValidatePattern(pattern)
	if err != nil {
		return err.Error()
	}
	return ""
}

// --- Recent Files ---

const maxRecentFiles = 10

// GetRecentFiles returns the recent files list
func (a *App) GetRecentFiles() []string {
	if a.config == nil {
		return nil
	}
	return a.config.GetSettings().RecentFiles
}

// AddRecentFile adds a file path to the recent files list (most recent first, capped)
func (a *App) AddRecentFile(filePath string) {
	if a.config == nil || filePath == "" {
		return
	}
	settings := a.config.GetSettings()
	// Remove duplicates
	filtered := make([]string, 0, len(settings.RecentFiles))
	for _, f := range settings.RecentFiles {
		if f != filePath {
			filtered = append(filtered, f)
		}
	}
	// Prepend
	settings.RecentFiles = append([]string{filePath}, filtered...)
	if len(settings.RecentFiles) > maxRecentFiles {
		settings.RecentFiles = settings.RecentFiles[:maxRecentFiles]
	}
	_ = a.config.SaveSettings(settings)
	a.RefreshRecentMenu()
}

// ClearRecentFiles empties the recent files list
func (a *App) ClearRecentFiles() {
	if a.config == nil {
		return
	}
	settings := a.config.GetSettings()
	settings.RecentFiles = []string{}
	_ = a.config.SaveSettings(settings)
	a.RefreshRecentMenu()
}

const appVersion = "0.5.0"

// GetAppVersion returns the application version string
func (a *App) GetAppVersion() string {
	if a.buildNumber != "" && a.buildNumber != "dev" {
		return appVersion + "+" + a.buildNumber
	}
	return appVersion
}

// ListThemes returns all available themes (built-in + custom)
func (a *App) ListThemes() []config.Theme {
	if a.config == nil {
		return config.BuiltInThemes()
	}
	return a.config.ListThemes()
}

// GetTheme returns a specific theme by name
func (a *App) GetTheme(name string) (config.Theme, error) {
	if a.config == nil {
		return config.Theme{}, fmt.Errorf("config not initialized")
	}
	t, ok := a.config.GetTheme(name)
	if !ok {
		return config.Theme{}, fmt.Errorf("theme %q not found", name)
	}
	return t, nil
}

// SaveCustomTheme saves a user-defined theme
func (a *App) SaveCustomTheme(t config.Theme) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.SaveTheme(t)
}

// DeleteCustomTheme removes a user-defined theme
func (a *App) DeleteCustomTheme(name string) error {
	if a.config == nil {
		return fmt.Errorf("config not initialized")
	}
	return a.config.DeleteTheme(name)
}

// RefreshRecentMenu rebuilds the "Open Recent" submenu with current recent files
func (a *App) RefreshRecentMenu() {
	if a.recentMenu == nil {
		return
	}
	a.recentMenu.Items = nil

	recentFiles := a.GetRecentFiles()
	if len(recentFiles) == 0 {
		a.recentMenu.AddText("(empty)", nil, nil)
	} else {
		for _, fp := range recentFiles {
			filePath := fp // capture for closure
			label := filepath.Base(filePath)
			a.recentMenu.AddText(label, nil, func(_ *menu.CallbackData) {
				wailsRuntime.EventsEmit(a.ctx, "menu:open-recent", filePath)
			})
		}
		a.recentMenu.AddSeparator()
		a.recentMenu.AddText("Clear Recent Files", nil, func(_ *menu.CallbackData) {
			a.ClearRecentFiles()
		})
	}

	if a.ctx != nil {
		wailsRuntime.MenuUpdateApplicationMenu(a.ctx)
	}
}

