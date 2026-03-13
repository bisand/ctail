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
	preloadedConfig *config.Manager // set in main() before wails.Run
	mu              sync.RWMutex
	tabs            map[string]*TabInfo
	nextID          int
}

// NewApp creates a new App
func NewApp() *App {
	return &App{
		tabs: make(map[string]*TabInfo),
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
}

// domReady restores saved window position after the webview is ready.
// Size is already set via Wails options in main().
// Note: WindowSetPosition is ignored on Wayland by design.
func (a *App) domReady(ctx context.Context) {
	if a.config == nil {
		return
	}
	s := a.config.GetSettings()
	if s.WindowX >= 0 && s.WindowY >= 0 {
		wailsRuntime.WindowSetPosition(ctx, s.WindowX, s.WindowY)
	}
}

// beforeClose saves window geometry while the window is still alive.
// On Linux/WebKit, OnShutdown fires too late — the window is already
// gone and WindowGetSize/WindowGetPosition return zeros.
func (a *App) beforeClose(ctx context.Context) (prevent bool) {
	if a.config != nil {
		s := a.config.GetSettings()
		s.WindowWidth, s.WindowHeight = wailsRuntime.WindowGetSize(ctx)
		s.WindowX, s.WindowY = wailsRuntime.WindowGetPosition(ctx)
		_ = a.config.SaveSettings(s)
	}
	return false
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

