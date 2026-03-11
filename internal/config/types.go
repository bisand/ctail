package config

import "time"

// Rule defines a highlighting rule
type Rule struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	Pattern    string `json:"pattern"`
	MatchType  string `json:"matchType"` // "line" or "match"
	Foreground string `json:"foreground"`
	Background string `json:"background"`
	Bold       bool   `json:"bold"`
	Italic     bool   `json:"italic"`
	Enabled    bool   `json:"enabled"`
	Priority   int    `json:"priority"`
}

// Profile is a named set of highlighting rules
type Profile struct {
	Name  string `json:"name"`
	Rules []Rule `json:"rules"`
}

// TabState stores per-tab persistent settings
type TabState struct {
	FilePath   string `json:"filePath"`
	ProfileID  string `json:"profileId"`
	AutoScroll bool   `json:"autoScroll"`
}

// AppSettings contains global application settings
type AppSettings struct {
	PollInterval  time.Duration `json:"-"`
	PollIntervalMs int          `json:"pollIntervalMs"`
	BufferSize    int           `json:"bufferSize"`
	Theme         string        `json:"theme"` // "dark" or "light"
	FontSize      int           `json:"fontSize"`
	ShowLineNumbers bool        `json:"showLineNumbers"`
	WordWrap      bool          `json:"wordWrap"`
	RestoreTabs   bool          `json:"restoreTabs"`
	WindowWidth   int           `json:"windowWidth"`
	WindowHeight  int           `json:"windowHeight"`
	Tabs          []TabState    `json:"tabs"`
}

// DefaultSettings returns sensible defaults
func DefaultSettings() AppSettings {
	return AppSettings{
		PollInterval:    500 * time.Millisecond,
		PollIntervalMs:  500,
		BufferSize:      10000,
		Theme:           "dark",
		FontSize:        14,
		ShowLineNumbers: false,
		WordWrap:        false,
		RestoreTabs:     true,
		WindowWidth:     1200,
		WindowHeight:    800,
		Tabs:            []TabState{},
	}
}

// DefaultProfile returns a built-in "Common Logs" profile
func DefaultProfile() Profile {
	return Profile{
		Name: "Common Logs",
		Rules: []Rule{
			{ID: "error", Name: "Error", Pattern: `(?i)\bERROR\b`, MatchType: "line", Foreground: "#ff6b6b", Background: "#3d1f1f", Bold: true, Enabled: true, Priority: 100},
			{ID: "fatal", Name: "Fatal", Pattern: `(?i)\bFATAL\b`, MatchType: "line", Foreground: "#ffffff", Background: "#cc0000", Bold: true, Enabled: true, Priority: 110},
			{ID: "warn", Name: "Warning", Pattern: `(?i)\bWARN(ING)?\b`, MatchType: "line", Foreground: "#ffd93d", Background: "#3d3520", Bold: false, Enabled: true, Priority: 90},
			{ID: "info", Name: "Info", Pattern: `(?i)\bINFO\b`, MatchType: "match", Foreground: "#6bcbff", Background: "", Bold: false, Enabled: true, Priority: 50},
			{ID: "debug", Name: "Debug", Pattern: `(?i)\bDEBUG\b`, MatchType: "match", Foreground: "#888888", Background: "", Bold: false, Enabled: true, Priority: 40},
			{ID: "timestamp", Name: "Timestamp", Pattern: `\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}`, MatchType: "match", Foreground: "#88cc88", Background: "", Bold: false, Enabled: true, Priority: 30},
		},
	}
}
