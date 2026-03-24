package main

import (
	"context"
	"ctail/internal/ai"
	"ctail/internal/config"
	"ctail/internal/rules"
	"ctail/internal/tailer"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
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
	throttle *lineThrottle
}

// App is the main application struct bound to Wails
type App struct {
	ctx               context.Context
	config            *config.Manager
	preloadedConfig   *config.Manager
	mu                sync.RWMutex
	tabs              map[string]*TabInfo
	nextID            int
	recentMenu        *menu.Menu
	version           string
	buildNumber       string
	cachedWinState    config.WindowState
	winStateMu        sync.Mutex
	stopWinTracker    chan struct{}
	stopUpdateChecker chan struct{}
	copilotCancel     context.CancelFunc // cancels a running device-flow poll
}

// lineThrottle batches line events per tab and flushes at most once per interval.
// This prevents event flooding from overwhelming the Wails IPC bridge and WebKit
// renderer, especially after VPN reconnections when all tabs reread simultaneously.
type lineThrottle struct {
	mu       sync.Mutex
	pending  []tailer.Line
	timer    *time.Timer
	interval time.Duration
	flush    func([]tailer.Line)
}

func newLineThrottle(interval time.Duration, flush func([]tailer.Line)) *lineThrottle {
	return &lineThrottle{
		interval: interval,
		flush:    flush,
	}
}

func (lt *lineThrottle) add(lines []tailer.Line) {
	lt.mu.Lock()
	defer lt.mu.Unlock()
	lt.pending = append(lt.pending, lines...)
	if lt.timer == nil {
		lt.timer = time.AfterFunc(lt.interval, lt.doFlush)
	}
}

func (lt *lineThrottle) doFlush() {
	lt.mu.Lock()
	batch := lt.pending
	lt.pending = nil
	lt.timer = nil
	lt.mu.Unlock()
	if len(batch) > 0 {
		lt.flush(batch)
	}
}

func (lt *lineThrottle) stop() {
	lt.mu.Lock()
	defer lt.mu.Unlock()
	if lt.timer != nil {
		lt.timer.Stop()
		lt.timer = nil
	}
	lt.pending = nil
}

// UpdateCheckResult is returned by ManualCheckForUpdates for the frontend dialog
type UpdateCheckResult struct {
	UpdateAvailable bool   `json:"updateAvailable"`
	LatestVersion   string `json:"latestVersion"`
	CurrentVersion  string `json:"currentVersion"`
	URL             string `json:"url"`
	Error           string `json:"error"`
}

// NewApp creates a new App
func NewApp(ver string, buildNum string) *App {
	return &App{
		tabs:        make(map[string]*TabInfo),
		version:     ver,
		buildNumber: buildNum,
	}
}

func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	a.stopWinTracker = make(chan struct{})
	a.stopUpdateChecker = make(chan struct{})
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

	// Initialize cached window state from config
	a.winStateMu.Lock()
	a.cachedWinState = a.config.GetSettings().Window
	a.winStateMu.Unlock()

	// Start background goroutine to track window state
	go a.trackWindowState()

	// Start periodic update checker
	if !a.config.GetSettings().DisableUpdateCheck {
		go a.periodicUpdateCheck()
	}
}

func (a *App) shutdown(ctx context.Context) {
	// Stop the window state tracker
	if a.stopWinTracker != nil {
		close(a.stopWinTracker)
	}

	// Stop the periodic update checker
	if a.stopUpdateChecker != nil {
		close(a.stopUpdateChecker)
	}

	// Save cached window state
	a.saveWindowState()

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
			if tab.throttle != nil {
				tab.throttle.stop()
			}
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

func (a *App) saveWindowState() {
	if a.config == nil {
		return
	}
	a.winStateMu.Lock()
	ws := a.cachedWinState
	a.winStateMu.Unlock()

	// Only save if we have valid cached state
	if ws.Width > 0 && ws.Height > 0 {
		settings := a.config.GetSettings()
		settings.Window = ws
		a.config.SaveSettings(settings)
	}
}

// trackWindowState periodically polls window geometry and caches it.
// This ensures we have valid state even when the window is destroyed during shutdown.
func (a *App) trackWindowState() {
	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ticker.C:
			if a.ctx == nil {
				continue
			}
			isMax := wailsRuntime.WindowIsMaximised(a.ctx)
			a.winStateMu.Lock()
			if !isMax {
				w, h := wailsRuntime.WindowGetSize(a.ctx)
				x, y := wailsRuntime.WindowGetPosition(a.ctx)
				if w > 0 && h > 0 {
					a.cachedWinState.X = x
					a.cachedWinState.Y = y
					a.cachedWinState.Width = w
					a.cachedWinState.Height = h
				}
			}
			a.cachedWinState.Maximised = isMax
			a.winStateMu.Unlock()
		case <-a.stopWinTracker:
			return
		}
	}
}

// periodicUpdateCheck runs an update check at startup (after a short delay) and
// then repeats at the configured interval (UpdateCheckIntervalHours).
func (a *App) periodicUpdateCheck() {
	// Small delay to let the UI fully load
	time.Sleep(3 * time.Second)
	a.doSilentUpdateCheck()

	hours := a.config.GetSettings().UpdateCheckIntervalHours
	if hours <= 0 {
		hours = 24
	}
	ticker := time.NewTicker(time.Duration(hours) * time.Hour)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			a.doSilentUpdateCheck()
		case <-a.stopUpdateChecker:
			return
		}
	}
}

// doSilentUpdateCheck queries GitHub and emits an event only when an update is available.
func (a *App) doSilentUpdateCheck() {
	result := a.fetchLatestRelease()
	if result.UpdateAvailable {
		wailsRuntime.EventsEmit(a.ctx, "app:update-available", map[string]string{
			"version": result.LatestVersion,
			"url":     result.URL,
		})
	}
}

// ManualCheckForUpdates is called from the Help menu. Returns a structured result for the dialog.
func (a *App) ManualCheckForUpdates() UpdateCheckResult {
	return a.fetchLatestRelease()
}

// fetchLatestRelease queries the GitHub releases API and returns the result.
func (a *App) fetchLatestRelease() UpdateCheckResult {
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Get("https://api.github.com/repos/bisand/ctail/releases/latest")
	if err != nil {
		return UpdateCheckResult{CurrentVersion: a.version, Error: "Failed to check for updates: " + err.Error()}
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return UpdateCheckResult{CurrentVersion: a.version, Error: "Failed to check for updates (HTTP " + fmt.Sprintf("%d", resp.StatusCode) + ")"}
	}

	var release struct {
		TagName string `json:"tag_name"`
		HTMLURL string `json:"html_url"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&release); err != nil {
		return UpdateCheckResult{CurrentVersion: a.version, Error: "Failed to parse update info"}
	}

	latest := strings.TrimPrefix(release.TagName, "v")
	if latest != "" && compareVersions(latest, a.version) > 0 {
		return UpdateCheckResult{
			UpdateAvailable: true,
			LatestVersion:   latest,
			CurrentVersion:  a.version,
			URL:             release.HTMLURL,
		}
	}
	return UpdateCheckResult{
		UpdateAvailable: false,
		CurrentVersion:  a.version,
		LatestVersion:   latest,
	}
}

// compareVersions compares two semver strings (e.g. "0.5.3" vs "0.5.2").
// Returns positive if a > b, negative if a < b, 0 if equal.
func compareVersions(a, b string) int {
	partsA := strings.Split(a, ".")
	partsB := strings.Split(b, ".")
	for i := 0; i < len(partsA) || i < len(partsB); i++ {
		var va, vb int
		if i < len(partsA) {
			fmt.Sscanf(partsA[i], "%d", &va)
		}
		if i < len(partsB) {
			fmt.Sscanf(partsB[i], "%d", &vb)
		}
		if va != vb {
			return va - vb
		}
	}
	return 0
}

// restoreWindowState applies saved window geometry after the DOM is ready
func (a *App) restoreWindowState(ctx context.Context) {
	if a.config == nil {
		return
	}
	ws := a.config.GetSettings().Window
	if ws.Width > 0 && ws.Height > 0 {
		wailsRuntime.WindowSetSize(a.ctx, ws.Width, ws.Height)
	}
	if ws.X != 0 || ws.Y != 0 {
		wailsRuntime.WindowSetPosition(a.ctx, ws.X, ws.Y)
	}
	if ws.Maximised {
		wailsRuntime.WindowMaximise(a.ctx)
	}
}

// OpenFileDialog opens a native file dialog and returns the selected path.
// defaultDir is optional — when non-empty the dialog starts in that directory.
func (a *App) OpenFileDialog(defaultDir string) (string, error) {
	// Validate the default directory is accessible with a short timeout
	// to avoid freezing the UI when the path is on a stale network mount.
	if defaultDir != "" {
		ch := make(chan bool, 1)
		go func() {
			_, err := os.Stat(defaultDir)
			ch <- err == nil
		}()
		select {
		case ok := <-ch:
			if !ok {
				defaultDir = ""
			}
		case <-time.After(2 * time.Second):
			defaultDir = "" // stale mount, fall back to system default
		}
	}

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

	// Per-tab line event throttle: batch lines and emit at most once per 100ms
	// to prevent IPC/rendering flooding after VPN reconnections.
	throttle := newLineThrottle(100*time.Millisecond, func(lines []tailer.Line) {
		wailsRuntime.EventsEmit(a.ctx, "tailer:lines", map[string]interface{}{
			"tabId": id,
			"lines": lines,
		})
	})

	tab := &TabInfo{
		ID:       id,
		FilePath: filePath,
		FileName: filepath.Base(filePath),
		Profile:  "Common Logs",
		tailer:   t,
		throttle: throttle,
	}

	// Set up callbacks
	t.OnLines(func(lines []tailer.Line) {
		throttle.add(lines)
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

	t.OnReconnecting(func() {
		wailsRuntime.EventsEmit(a.ctx, "tailer:reconnecting", map[string]interface{}{
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
		tab.throttle.stop()
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

// GetTabFileSize returns the current file size in bytes for a tab
func (a *App) GetTabFileSize(tabID string) int64 {
	a.mu.RLock()
	tab, ok := a.tabs[tabID]
	a.mu.RUnlock()
	if !ok {
		return 0
	}
	return tab.tailer.GetFileSize()
}

// MemoryStats holds memory usage information
type MemoryStats struct {
	Alloc      uint64 `json:"alloc"`      // resident set size (or Go heap fallback)
	TotalAlloc uint64 `json:"totalAlloc"` // cumulative bytes allocated
	Sys        uint64 `json:"sys"`        // bytes obtained from OS
	NumGC      uint32 `json:"numGC"`      // number of GC cycles
}

// getProcessMemory reads private memory (VmRSS - RsShmem - RssFile) from
// /proc/self/status. This matches the "Memory" column in system monitors,
// excluding shared libraries (like WebKit). Returns 0 if unavailable.
func getProcessMemory() uint64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0
	}
	var rss, shared, file uint64
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		var val uint64
		fmt.Sscanf(fields[1], "%d", &val)
		switch fields[0] {
		case "VmRSS:":
			rss = val
		case "RssShmem:":
			shared = val
		case "RssFile:":
			file = val
		}
	}
	if rss == 0 {
		return 0
	}
	private := rss - shared - file
	return private * 1024
}

// GetMemoryUsage returns current memory usage stats.
// Alloc reports process RSS on Linux, falling back to Go heap on other platforms.
func (a *App) GetMemoryUsage() MemoryStats {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	alloc := getProcessMemory()
	if alloc == 0 {
		alloc = m.Alloc
	}
	return MemoryStats{
		Alloc:      alloc,
		TotalAlloc: m.TotalAlloc,
		Sys:        m.Sys,
		NumGC:      m.NumGC,
	}
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

// GetAppVersion returns the application version string
func (a *App) GetAppVersion() string {
	if a.buildNumber != "" && a.buildNumber != "dev" {
		return a.version + "+" + a.buildNumber
	}
	return a.version
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

// StartCopilotAuth initiates the GitHub OAuth device flow for Copilot.
// Returns the user code and verification URI for the user to complete in their browser.
func (a *App) StartCopilotAuth() (*ai.DeviceCodeResponse, error) {
	// Cancel any previous poll
	if a.copilotCancel != nil {
		a.copilotCancel()
		a.copilotCancel = nil
	}
	return ai.RequestDeviceCode()
}

// CompleteCopilotAuth polls for the OAuth token after the user authorises.
// Blocks until the user completes auth, the flow times out, or a new auth starts.
// On success, saves the token to settings and returns true.
func (a *App) CompleteCopilotAuth(deviceCode string, interval int) (bool, error) {
	ctx, cancel := context.WithCancel(context.Background())
	a.copilotCancel = cancel

	token, err := ai.PollForToken(ctx, deviceCode, interval)
	if err != nil {
		return false, err
	}

	// Verify the token works by doing a test exchange
	_, err = ai.ExchangeCopilotToken(token)
	if err != nil {
		return false, fmt.Errorf("signed in but Copilot access denied: %w\nMake sure your GitHub account has an active Copilot subscription", err)
	}

	// Save the OAuth token
	s := a.config.GetSettings()
	s.AIProvider = string(ai.ProviderCopilot)
	s.AIKey = token
	s.AIEndpoint = ""
	if err := a.config.SaveSettings(s); err != nil {
		return false, fmt.Errorf("save settings: %w", err)
	}
	return true, nil
}

// newAIClient builds an AI client from current settings.
func (a *App) newAIClient() (*ai.Client, error) {
	s := a.config.GetSettings()
	if s.AIProvider == "" {
		return nil, fmt.Errorf("AI provider not configured — set it in Settings")
	}
	if s.AIKey == "" {
		return nil, fmt.Errorf("AI API key not set — add it in Settings")
	}

	provider := ai.Provider(s.AIProvider)
	endpoint := s.AIEndpoint
	model := s.AIModel

	switch provider {
	case ai.ProviderOpenAI:
		if endpoint == "" {
			endpoint = "https://api.openai.com"
		}
		if model == "" {
			model = "gpt-4o-mini"
		}
	case ai.ProviderGitHubModels:
		if endpoint == "" {
			endpoint = "https://models.inference.ai.azure.com"
		}
		if model == "" {
			model = "gpt-4o-mini"
		}
	case ai.ProviderCopilot:
		if endpoint == "" {
			endpoint = "https://api.githubcopilot.com"
		}
		if model == "" {
			model = "gpt-4o"
		}
		// Exchange the OAuth token for a short-lived Copilot API token
		ct, err := ai.ExchangeCopilotToken(s.AIKey)
		if err != nil {
			return nil, fmt.Errorf("Copilot token exchange failed: %w", err)
		}
		return ai.NewClient(ai.Config{
			Provider: provider,
			Endpoint: endpoint,
			APIKey:   ct.Token,
			Model:    model,
			Timeout:  60 * time.Second,
		}), nil
	case ai.ProviderCustom:
		if endpoint == "" {
			return nil, fmt.Errorf("custom AI endpoint URL is required")
		}
	}

	return ai.NewClient(ai.Config{
		Provider: provider,
		Endpoint: endpoint,
		APIKey:   s.AIKey,
		Model:    model,
		Timeout:  60 * time.Second,
	}), nil
}

// getTabLogContent extracts log text from a tab for AI context.
// context: "buffer" (current buffer), "selection" (line range), "last" (last N lines).
func (a *App) getTabLogContent(tabID, context string, startLine int64, lineCount int) (string, error) {
	a.mu.RLock()
	tab, ok := a.tabs[tabID]
	a.mu.RUnlock()
	if !ok {
		return "", fmt.Errorf("tab not found: %s", tabID)
	}

	var lines []tailer.Line
	switch context {
	case "selection":
		if lineCount <= 0 {
			lineCount = 100
		}
		lines = tab.tailer.ReadRange(startLine, lineCount)
	case "last":
		if lineCount <= 0 {
			lineCount = 200
		}
		total := tab.tailer.GetTotalLines()
		start := total - int64(lineCount) + 1
		if start < 1 {
			start = 1
		}
		lines = tab.tailer.ReadRange(start, lineCount)
	default: // "buffer"
		lines = tab.tailer.GetLines()
	}

	if len(lines) == 0 {
		return "", fmt.Errorf("no log content available")
	}

	// Cap at ~4000 lines to stay within token limits
	if len(lines) > 4000 {
		lines = lines[len(lines)-4000:]
	}

	var sb strings.Builder
	for _, l := range lines {
		sb.WriteString(l.Text)
		sb.WriteByte('\n')
	}
	return sb.String(), nil
}

// AskAI sends a question about the log content to the configured AI provider.
// context: "buffer", "selection", or "last". startLine and lineCount apply to "selection" and "last".
func (a *App) AskAI(tabID, question, logContext string, startLine int64, lineCount int) (string, error) {
	client, err := a.newAIClient()
	if err != nil {
		return "", err
	}

	content, err := a.getTabLogContent(tabID, logContext, startLine, lineCount)
	if err != nil {
		return "", err
	}

	messages := ai.BuildLogMessages(content, question)
	return client.Chat(messages)
}

// GenerateRulesProfile asks AI to analyze logs and create a highlighting rules profile.
func (a *App) GenerateRulesProfile(tabID, profileName string) (config.Profile, error) {
	if profileName == "" {
		return config.Profile{}, fmt.Errorf("profile name is required")
	}

	client, err := a.newAIClient()
	if err != nil {
		return config.Profile{}, err
	}

	content, err := a.getTabLogContent(tabID, "buffer", 0, 0)
	if err != nil {
		return config.Profile{}, err
	}

	messages := ai.BuildRuleGenMessages(content)
	response, err := client.Chat(messages)
	if err != nil {
		return config.Profile{}, fmt.Errorf("AI request failed: %w", err)
	}

	// Strip any markdown fences the model might add despite instructions
	response = strings.TrimSpace(response)
	response = strings.TrimPrefix(response, "```json")
	response = strings.TrimPrefix(response, "```")
	response = strings.TrimSuffix(response, "```")
	response = strings.TrimSpace(response)

	var rules []config.Rule
	if err := json.Unmarshal([]byte(response), &rules); err != nil {
		return config.Profile{}, fmt.Errorf("failed to parse AI response as rules: %w\n\nRaw response:\n%s", err, truncateStr(response, 500))
	}

	profile := config.Profile{
		Name:  profileName,
		Rules: rules,
	}

	// Persist the new profile
	if err := a.config.SaveProfile(profile); err != nil {
		return config.Profile{}, fmt.Errorf("failed to save profile: %w", err)
	}

	return profile, nil
}

// AskAIRules sends a natural-language request about highlight rules to the AI.
// It includes the current active profile rules and log content from all open tabs as context.
// The AI returns an updated profile which is saved and returned.
func (a *App) AskAIRules(question string) (config.Profile, error) {
	if question == "" {
		return config.Profile{}, fmt.Errorf("question is required")
	}

	client, err := a.newAIClient()
	if err != nil {
		return config.Profile{}, err
	}

	// Get the current active profile as context
	s := a.config.GetSettings()
	activeProfileName := s.ActiveProfile
	currentProfile, ok := a.config.GetProfile(activeProfileName)
	if !ok {
		// Use an empty profile if none is active
		currentProfile = config.Profile{Name: activeProfileName, Rules: []config.Rule{}}
	}

	profileJSON, err := json.MarshalIndent(currentProfile, "", "  ")
	if err != nil {
		return config.Profile{}, fmt.Errorf("failed to serialize current profile: %w", err)
	}

	// Gather log content from all open tabs for context
	var logContent string
	a.mu.RLock()
	tabsCopy := make([]*TabInfo, 0, len(a.tabs))
	for _, t := range a.tabs {
		tabsCopy = append(tabsCopy, t)
	}
	a.mu.RUnlock()

	if len(tabsCopy) > 0 {
		var sb strings.Builder
		for _, tab := range tabsCopy {
			lines := tab.tailer.GetLines()
			if len(lines) == 0 {
				continue
			}
			// Cap per-tab to keep within token limits
			if len(lines) > 1000 {
				lines = lines[len(lines)-1000:]
			}
			sb.WriteString(fmt.Sprintf("=== File: %s ===\n", tab.FilePath))
			for _, l := range lines {
				sb.WriteString(l.Text)
				sb.WriteByte('\n')
			}
			sb.WriteByte('\n')
		}
		logContent = sb.String()
		// Overall cap
		if len(logContent) > 200000 {
			logContent = logContent[len(logContent)-200000:]
		}
	}

	messages := ai.BuildRulesAssistantMessages(string(profileJSON), logContent, question)
	response, err := client.Chat(messages)
	if err != nil {
		return config.Profile{}, fmt.Errorf("AI request failed: %w", err)
	}

	// Strip any markdown fences the model might add
	response = strings.TrimSpace(response)
	response = strings.TrimPrefix(response, "```json")
	response = strings.TrimPrefix(response, "```")
	response = strings.TrimSuffix(response, "```")
	response = strings.TrimSpace(response)

	var profile config.Profile
	if err := json.Unmarshal([]byte(response), &profile); err != nil {
		return config.Profile{}, fmt.Errorf("failed to parse AI response as profile: %w\n\nRaw response:\n%s", err, truncateStr(response, 500))
	}

	// If the AI didn't return a name, keep the current active profile name
	if profile.Name == "" {
		profile.Name = activeProfileName
	}

	// Persist the profile
	if err := a.config.SaveProfile(profile); err != nil {
		return config.Profile{}, fmt.Errorf("failed to save profile: %w", err)
	}

	return profile, nil
}

func truncateStr(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen] + "..."
}



