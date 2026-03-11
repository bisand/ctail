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
