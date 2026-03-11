package rules

import (
	"regexp"
	"sync"
)

// CompiledRule is a rule with a pre-compiled regex
type CompiledRule struct {
	ID         string
	Name       string
	Pattern    *regexp.Regexp
	MatchType  string
	Foreground string
	Background string
	Bold       bool
	Italic     bool
	Priority   int
}

// Match represents a highlight region in a line
type Match struct {
	Start      int    `json:"start"`
	End        int    `json:"end"`
	RuleID     string `json:"ruleId"`
	Foreground string `json:"foreground"`
	Background string `json:"background"`
	Bold       bool   `json:"bold"`
	Italic     bool   `json:"italic"`
}

// LineResult contains highlight info for a line
type LineResult struct {
	FullLine bool    `json:"fullLine"` // if true, whole line is styled
	Matches  []Match `json:"matches"`
	// full-line style (used when fullLine is true)
	Foreground string `json:"foreground,omitempty"`
	Background string `json:"background,omitempty"`
	Bold       bool   `json:"bold,omitempty"`
	Italic     bool   `json:"italic,omitempty"`
}

// Engine compiles and applies highlighting rules
type Engine struct {
	mu    sync.RWMutex
	rules []CompiledRule
}

// NewEngine creates a new rule engine
func NewEngine() *Engine {
	return &Engine{}
}

// RuleInput is the JSON-friendly rule format
type RuleInput struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	Pattern    string `json:"pattern"`
	MatchType  string `json:"matchType"`
	Foreground string `json:"foreground"`
	Background string `json:"background"`
	Bold       bool   `json:"bold"`
	Italic     bool   `json:"italic"`
	Enabled    bool   `json:"enabled"`
	Priority   int    `json:"priority"`
}

// SetRules compiles and sets rules, sorted by priority
func (e *Engine) SetRules(inputs []RuleInput) error {
	var compiled []CompiledRule
	for _, r := range inputs {
		if !r.Enabled {
			continue
		}
		re, err := regexp.Compile(r.Pattern)
		if err != nil {
			continue // skip invalid patterns
		}
		compiled = append(compiled, CompiledRule{
			ID:         r.ID,
			Name:       r.Name,
			Pattern:    re,
			MatchType:  r.MatchType,
			Foreground: r.Foreground,
			Background: r.Background,
			Bold:       r.Bold,
			Italic:     r.Italic,
			Priority:   r.Priority,
		})
	}

	// Sort by priority ascending (lower first, higher wins later)
	for i := 0; i < len(compiled); i++ {
		for j := i + 1; j < len(compiled); j++ {
			if compiled[j].Priority < compiled[i].Priority {
				compiled[i], compiled[j] = compiled[j], compiled[i]
			}
		}
	}

	e.mu.Lock()
	e.rules = compiled
	e.mu.Unlock()
	return nil
}

// Apply checks a line against all rules and returns highlight results
func (e *Engine) Apply(text string) LineResult {
	e.mu.RLock()
	rules := e.rules
	e.mu.RUnlock()

	var result LineResult

	for _, rule := range rules {
		if rule.MatchType == "line" {
			if rule.Pattern.MatchString(text) {
				result.FullLine = true
				result.Foreground = rule.Foreground
				result.Background = rule.Background
				result.Bold = rule.Bold
				result.Italic = rule.Italic
			}
		} else {
			locs := rule.Pattern.FindAllStringIndex(text, -1)
			for _, loc := range locs {
				result.Matches = append(result.Matches, Match{
					Start:      loc[0],
					End:        loc[1],
					RuleID:     rule.ID,
					Foreground: rule.Foreground,
					Background: rule.Background,
					Bold:       rule.Bold,
					Italic:     rule.Italic,
				})
			}
		}
	}

	return result
}

// ValidatePattern checks if a regex pattern is valid
func ValidatePattern(pattern string) error {
	_, err := regexp.Compile(pattern)
	return err
}
