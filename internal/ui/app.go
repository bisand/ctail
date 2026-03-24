package ui

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"ctail/internal/config"
	"ctail/internal/rules"
	"ctail/internal/tailer"
)

// Tab represents an open log file tab.
type Tab struct {
	ID       string
	Name     string
	FilePath string
	Lines    []tailer.Line
	Tailer   *tailer.Tailer

	WindowStart int64 // first line number in current buffer window
	TotalLines  int64 // total lines known in file
	AutoScroll  bool
	HasUpdate   bool
	Loading     bool // true while a fetch is in progress
}

// App holds all application state for the Gio UI.
type App struct {
	mu     sync.Mutex
	Tabs   []*Tab
	Active int

	Config   *config.Manager
	Settings config.AppSettings
	Rules    *rules.Engine
	Colors   Colors

	// Invalidate triggers a UI redraw from any goroutine.
	Invalidate func()

	nextTabID int
}

// NewApp creates a new App, loading config from disk.
func NewApp() (*App, error) {
	cfg, err := config.NewManager()
	if err != nil {
		return nil, fmt.Errorf("config: %w", err)
	}

	settings := cfg.GetSettings()
	colors := loadColors(cfg, settings)
	engine := loadRules(cfg, settings)

	return &App{
		Config:   cfg,
		Settings: settings,
		Rules:    engine,
		Colors:   colors,
		Active:   -1,
	}, nil
}

func loadColors(cfg *config.Manager, s config.AppSettings) Colors {
	if th, ok := cfg.GetTheme(s.Theme); ok {
		if s.ThemeMode == "light" {
			return ColorsFromTheme(th.Light)
		}
		return ColorsFromTheme(th.Dark)
	}
	return DefaultColors()
}

func loadRules(cfg *config.Manager, s config.AppSettings) *rules.Engine {
	engine := rules.NewEngine()
	if p, ok := cfg.GetProfile(s.ActiveProfile); ok {
		inputs := make([]rules.RuleInput, len(p.Rules))
		for i, r := range p.Rules {
			inputs[i] = rules.RuleInput{
				ID: r.ID, Name: r.Name, Pattern: r.Pattern,
				MatchType: r.MatchType, Foreground: r.Foreground,
				Background: r.Background, Bold: r.Bold, Italic: r.Italic,
				Enabled: r.Enabled, Priority: r.Priority,
			}
		}
		_ = engine.SetRules(inputs)
	}
	return engine
}

// OpenFile opens a file in a new tab and starts tailing it.
func (a *App) OpenFile(filePath string) {
	a.mu.Lock()
	defer a.mu.Unlock()

	// Activate existing tab if file already open
	for i, tab := range a.Tabs {
		if tab.FilePath == filePath {
			a.Active = i
			return
		}
	}

	id := fmt.Sprintf("tab-%d", a.nextTabID)
	a.nextTabID++

	bufSize := a.Settings.BufferSize
	if bufSize < 1000 {
		bufSize = 10000
	}
	poll := a.Settings.PollInterval
	if poll < 100*time.Millisecond {
		poll = 200 * time.Millisecond
	}

	t := tailer.New(filePath, poll, bufSize)
	tab := &Tab{
		ID:         id,
		Name:       filepath.Base(filePath),
		FilePath:   filePath,
		AutoScroll: true,
		Tailer:     t,
	}

	invalidate := func() {
		if a.Invalidate != nil {
			a.Invalidate()
		}
	}

	t.OnLines(func(_ []tailer.Line) {
		a.mu.Lock()
		tab.Lines = t.GetLines()
		tab.TotalLines = t.GetTotalLines()
		if len(tab.Lines) > 0 {
			tab.WindowStart = tab.Lines[0].Number
		}
		if a.Active >= 0 && a.Active < len(a.Tabs) && a.Tabs[a.Active] != tab {
			tab.HasUpdate = true
		}
		a.mu.Unlock()
		invalidate()
	})
	t.OnTruncated(func() {
		a.mu.Lock()
		tab.Lines = t.GetLines()
		tab.TotalLines = t.GetTotalLines()
		if len(tab.Lines) > 0 {
			tab.WindowStart = tab.Lines[0].Number
		}
		a.mu.Unlock()
		invalidate()
	})
	t.OnReady(func() {
		a.mu.Lock()
		tab.Lines = t.GetLines()
		tab.TotalLines = t.GetTotalLines()
		if len(tab.Lines) > 0 {
			tab.WindowStart = tab.Lines[0].Number
		}
		a.mu.Unlock()
		invalidate()
	})

	a.Tabs = append(a.Tabs, tab)
	a.Active = len(a.Tabs) - 1

	go func() {
		if err := t.Start(); err != nil {
			fmt.Fprintf(os.Stderr, "tailer error for %s: %v\n", filePath, err)
		}
	}()
}

// CloseTab closes the tab at the given index.
func (a *App) CloseTab(idx int) {
	a.mu.Lock()
	defer a.mu.Unlock()

	if idx < 0 || idx >= len(a.Tabs) {
		return
	}
	a.Tabs[idx].Tailer.Stop()
	a.Tabs = append(a.Tabs[:idx], a.Tabs[idx+1:]...)
	if a.Active >= len(a.Tabs) {
		a.Active = len(a.Tabs) - 1
	}
}

// ActiveTab returns the currently active tab, or nil.
func (a *App) ActiveTab() *Tab {
	if a.Active < 0 || a.Active >= len(a.Tabs) {
		return nil
	}
	return a.Tabs[a.Active]
}

// SetActive sets the active tab and clears its update badge.
func (a *App) SetActive(idx int) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if idx >= 0 && idx < len(a.Tabs) {
		a.Active = idx
		a.Tabs[idx].HasUpdate = false
	}
}

// Lock/Unlock for callers that need to read state safely.
func (a *App) Lock()   { a.mu.Lock() }
func (a *App) Unlock() { a.mu.Unlock() }

// Shutdown stops all tailers.
func (a *App) Shutdown() {
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, tab := range a.Tabs {
		tab.Tailer.Stop()
	}
}

// ToggleAutoScroll toggles the auto-scroll state for the active tab.
func (a *App) ToggleAutoScroll() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if tab := a.ActiveTab(); tab != nil {
		tab.AutoScroll = !tab.AutoScroll
	}
}

// SetAutoScroll sets the auto-scroll state for the active tab.
func (a *App) SetAutoScroll(on bool) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if tab := a.ActiveTab(); tab != nil {
		tab.AutoScroll = on
	}
}

const (
	FetchBatch    = 200
	MaxCachedPages = 2
)

// FetchEarlierLines loads older lines into the buffer for the active tab.
// Returns the number of lines prepended (for scroll adjustment).
func (a *App) FetchEarlierLines(maxWindow int) int {
	a.mu.Lock()
	tab := a.ActiveTab()
	if tab == nil || tab.Loading || len(tab.Lines) == 0 {
		a.mu.Unlock()
		return 0
	}
	ws := tab.Lines[0].Number
	if ws <= 1 {
		a.mu.Unlock()
		return 0
	}
	tab.Loading = true
	tabID := tab.ID
	tailerRef := tab.Tailer
	a.mu.Unlock()

	fetchStart := ws - int64(FetchBatch)
	if fetchStart < 1 {
		fetchStart = 1
	}
	fetchCount := int(ws - fetchStart)
	if fetchCount <= 0 {
		a.mu.Lock()
		tab.Loading = false
		a.mu.Unlock()
		return 0
	}

	olderLines := tailerRef.ReadRange(fetchStart, fetchCount)

	a.mu.Lock()
	defer a.mu.Unlock()

	// Verify tab is still valid
	tab = a.findTab(tabID)
	if tab == nil {
		return 0
	}
	tab.Loading = false

	if len(olderLines) == 0 {
		return 0
	}

	// Prepend and trim from end
	merged := make([]tailer.Line, 0, len(olderLines)+len(tab.Lines))
	merged = append(merged, olderLines...)
	merged = append(merged, tab.Lines...)
	if len(merged) > maxWindow {
		merged = merged[:maxWindow]
	}
	tab.Lines = merged
	if len(tab.Lines) > 0 {
		tab.WindowStart = tab.Lines[0].Number
	}

	return len(olderLines)
}

// FetchLaterLines loads newer lines into the buffer for the active tab.
// Returns the number of lines trimmed from front (for scroll adjustment).
func (a *App) FetchLaterLines(maxWindow int) int {
	a.mu.Lock()
	tab := a.ActiveTab()
	if tab == nil || tab.Loading || len(tab.Lines) == 0 {
		a.mu.Unlock()
		return 0
	}
	we := tab.Lines[len(tab.Lines)-1].Number
	total := tab.TotalLines
	if we >= total {
		a.mu.Unlock()
		return 0
	}
	tab.Loading = true
	tabID := tab.ID
	tailerRef := tab.Tailer
	a.mu.Unlock()

	fetchStart := we + 1
	newerLines := tailerRef.ReadRange(fetchStart, FetchBatch)

	a.mu.Lock()
	defer a.mu.Unlock()

	tab = a.findTab(tabID)
	if tab == nil {
		return 0
	}
	tab.Loading = false

	if len(newerLines) == 0 {
		return 0
	}

	// Append and trim from front
	prevLen := len(tab.Lines)
	merged := make([]tailer.Line, 0, prevLen+len(newerLines))
	merged = append(merged, tab.Lines...)
	merged = append(merged, newerLines...)
	trimmed := 0
	if len(merged) > maxWindow {
		trimmed = len(merged) - maxWindow
		merged = merged[trimmed:]
	}
	tab.Lines = merged
	if len(tab.Lines) > 0 {
		tab.WindowStart = tab.Lines[0].Number
	}

	return trimmed
}

// CanScrollBack returns true if there are earlier lines in the file.
func (a *App) CanScrollBack() bool {
	tab := a.ActiveTab()
	if tab == nil || len(tab.Lines) == 0 {
		return false
	}
	return tab.Lines[0].Number > 1
}

// CanScrollForward returns true if there are later lines in the file.
func (a *App) CanScrollForward() bool {
	tab := a.ActiveTab()
	if tab == nil || len(tab.Lines) == 0 {
		return false
	}
	return tab.Lines[len(tab.Lines)-1].Number < tab.TotalLines
}

func (a *App) findTab(id string) *Tab {
	for _, t := range a.Tabs {
		if t.ID == id {
			return t
		}
	}
	return nil
}
