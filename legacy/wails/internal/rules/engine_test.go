package rules

import (
	"testing"
)

func TestEngineLineMatch(t *testing.T) {
	e := NewEngine()
	err := e.SetRules([]RuleInput{
		{ID: "error", Name: "Error", Pattern: `(?i)\bERROR\b`, MatchType: "line",
			Foreground: "#ff0000", Background: "#330000", Bold: true, Enabled: true, Priority: 100},
	})
	if err != nil {
		t.Fatal(err)
	}

	result := e.Apply("2024-01-01 ERROR something went wrong")
	if !result.FullLine {
		t.Error("expected full line match")
	}
	if result.Foreground != "#ff0000" {
		t.Errorf("expected #ff0000, got %s", result.Foreground)
	}
}

func TestEngineWordMatch(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "ts", Name: "Timestamp", Pattern: `\d{4}-\d{2}-\d{2}`, MatchType: "match",
			Foreground: "#00ff00", Enabled: true, Priority: 50},
	})

	result := e.Apply("2024-01-15 INFO hello world")
	if result.FullLine {
		t.Error("should not be full line match")
	}
	if len(result.Matches) != 1 {
		t.Fatalf("expected 1 match, got %d", len(result.Matches))
	}
	if result.Matches[0].Start != 0 || result.Matches[0].End != 10 {
		t.Errorf("match at wrong position: %d-%d", result.Matches[0].Start, result.Matches[0].End)
	}
}

func TestEngineDisabledRule(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "disabled", Pattern: `test`, MatchType: "match", Enabled: false, Priority: 50},
	})

	result := e.Apply("test line")
	if len(result.Matches) > 0 || result.FullLine {
		t.Error("disabled rule should not match")
	}
}

func TestEngineMultipleMatches(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "num", Pattern: `\d+`, MatchType: "match", Foreground: "#0000ff", Enabled: true, Priority: 50},
	})

	result := e.Apply("abc 123 def 456")
	if len(result.Matches) != 2 {
		t.Fatalf("expected 2 matches, got %d", len(result.Matches))
	}
}

func TestEnginePriority(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "low", Pattern: `ERROR`, MatchType: "line", Foreground: "#aaa", Enabled: true, Priority: 10},
		{ID: "high", Pattern: `ERROR`, MatchType: "line", Foreground: "#fff", Enabled: true, Priority: 100},
	})

	result := e.Apply("ERROR test")
	if !result.FullLine {
		t.Error("expected full line match")
	}
	// Higher priority should win (applied last)
	if result.Foreground != "#fff" {
		t.Errorf("expected high priority color #fff, got %s", result.Foreground)
	}
}

func TestValidatePattern(t *testing.T) {
	if err := ValidatePattern(`\btest\b`); err != nil {
		t.Errorf("valid pattern failed: %v", err)
	}
	if err := ValidatePattern(`[invalid`); err == nil {
		t.Error("invalid pattern should fail")
	}
}

func TestEngineNoRules(t *testing.T) {
	e := NewEngine()
	result := e.Apply("some text")
	if result.FullLine || len(result.Matches) > 0 {
		t.Error("no rules should produce no matches")
	}
}

func TestEngineInvalidPattern(t *testing.T) {
	e := NewEngine()
	err := e.SetRules([]RuleInput{
		{ID: "bad", Pattern: `[invalid`, MatchType: "match", Enabled: true, Priority: 50},
	})
	if err != nil {
		t.Error("SetRules should not return error for invalid patterns (skips them)")
	}
	result := e.Apply("test")
	if len(result.Matches) > 0 {
		t.Error("invalid pattern should be skipped")
	}
}

func TestEngineEmptyPattern(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "empty", Pattern: ``, MatchType: "match", Enabled: true, Priority: 50, Foreground: "#fff"},
	})
	// Empty pattern matches everything — just ensure no panic
	result := e.Apply("test")
	_ = result
}

func TestEngineLineAndMatchCombination(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "line", Pattern: `ERROR`, MatchType: "line", Foreground: "#ff0000", Enabled: true, Priority: 100},
		{ID: "word", Pattern: `\d+`, MatchType: "match", Foreground: "#00ff00", Enabled: true, Priority: 50},
	})

	result := e.Apply("ERROR code 42")
	if !result.FullLine {
		t.Error("expected full line match from ERROR rule")
	}
	if result.Foreground != "#ff0000" {
		t.Errorf("expected line color #ff0000, got %s", result.Foreground)
	}
	// match rules should still produce matches even with a line match
	if len(result.Matches) != 1 {
		t.Errorf("expected 1 word match (42), got %d", len(result.Matches))
	}
}

func TestEngineItalicBold(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "styled", Pattern: `WARN`, MatchType: "line", Foreground: "#ffaa00",
			Bold: true, Italic: true, Enabled: true, Priority: 50},
	})

	result := e.Apply("WARN: something")
	if !result.Bold {
		t.Error("expected bold")
	}
	if !result.Italic {
		t.Error("expected italic")
	}
}

func TestEngineMatchStyles(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "ip", Pattern: `\d+\.\d+\.\d+\.\d+`, MatchType: "match",
			Foreground: "#00ffff", Background: "#003333", Bold: true, Italic: true,
			Enabled: true, Priority: 50},
	})

	result := e.Apply("Connected from 192.168.1.1")
	if len(result.Matches) != 1 {
		t.Fatalf("expected 1 match, got %d", len(result.Matches))
	}
	m := result.Matches[0]
	if m.Foreground != "#00ffff" {
		t.Errorf("foreground = %q", m.Foreground)
	}
	if m.Background != "#003333" {
		t.Errorf("background = %q", m.Background)
	}
	if !m.Bold {
		t.Error("expected bold")
	}
	if !m.Italic {
		t.Error("expected italic")
	}
	if m.RuleID != "ip" {
		t.Errorf("ruleID = %q, want ip", m.RuleID)
	}
}

func TestEngineEmptyText(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "any", Pattern: `.+`, MatchType: "match", Enabled: true, Priority: 50},
	})

	result := e.Apply("")
	if result.FullLine || len(result.Matches) > 0 {
		t.Error("empty text should produce no matches")
	}
}

func TestEngineConcurrentApply(t *testing.T) {
	e := NewEngine()
	e.SetRules([]RuleInput{
		{ID: "num", Pattern: `\d+`, MatchType: "match", Foreground: "#fff", Enabled: true, Priority: 50},
	})

	done := make(chan bool, 10)
	for i := 0; i < 10; i++ {
		go func() {
			for j := 0; j < 100; j++ {
				result := e.Apply("test 123 line")
				if len(result.Matches) != 1 {
					t.Errorf("expected 1 match, got %d", len(result.Matches))
				}
			}
			done <- true
		}()
	}
	for i := 0; i < 10; i++ {
		<-done
	}
}

func TestValidatePatternEdgeCases(t *testing.T) {
	// Valid patterns
	for _, p := range []string{`.*`, `\d+`, `(?i)error`, `^$`, `a{3,5}`, `(a|b)`} {
		if err := ValidatePattern(p); err != nil {
			t.Errorf("ValidatePattern(%q) should pass, got %v", p, err)
		}
	}
	// Invalid patterns
	for _, p := range []string{`[unclosed`, `(?P<wrong`, `*repeat`} {
		if err := ValidatePattern(p); err == nil {
			t.Errorf("ValidatePattern(%q) should fail", p)
		}
	}
}
