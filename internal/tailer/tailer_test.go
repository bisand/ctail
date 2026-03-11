package tailer

import (
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func TestTailerBasicRead(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	// Create file with some lines
	content := "line 1\nline 2\nline 3\n"
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var received []Line

	tail := New(path, 100*time.Millisecond, 1000)
	tail.OnLines(func(lines []Line) {
		mu.Lock()
		received = append(received, lines...)
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read
	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if len(received) < 3 {
		t.Errorf("expected at least 3 lines, got %d", len(received))
	}
	mu.Unlock()

	lines := tail.GetLines()
	if len(lines) != 3 {
		t.Errorf("expected 3 buffered lines, got %d", len(lines))
	}
	if lines[0].Text != "line 1" {
		t.Errorf("expected 'line 1', got %q", lines[0].Text)
	}
}

func TestTailerNewLines(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("initial\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var received []Line

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnLines(func(lines []Line) {
		mu.Lock()
		received = append(received, lines...)
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(100 * time.Millisecond)

	// Append new lines
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString("new line 1\nnew line 2\n")
	f.Close()

	time.Sleep(200 * time.Millisecond)

	lines := tail.GetLines()
	if len(lines) != 3 {
		t.Errorf("expected 3 lines, got %d", len(lines))
	}
}

func TestTailerTruncation(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\nline 2\nline 3\n"), 0644); err != nil {
		t.Fatal(err)
	}

	truncated := false
	var mu sync.Mutex

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnTruncated(func() {
		mu.Lock()
		truncated = true
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(100 * time.Millisecond)

	// Truncate the file
	if err := os.WriteFile(path, []byte("fresh\n"), 0644); err != nil {
		t.Fatal(err)
	}

	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if !truncated {
		t.Error("expected truncation callback")
	}
	mu.Unlock()

	lines := tail.GetLines()
	if len(lines) != 1 {
		t.Errorf("expected 1 line after truncation, got %d", len(lines))
	}
}

func TestTailerSlidingWindow(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	// Write more lines than buffer size
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 200; i++ {
		f.WriteString("line content\n")
	}
	f.Close()

	tail := New(path, 100*time.Millisecond, 100) // buffer size 100
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	lines := tail.GetLines()
	if len(lines) > 100 {
		t.Errorf("expected max 100 lines in buffer, got %d", len(lines))
	}
}

func TestTailerReadRange(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	for i := 1; i <= 50; i++ {
		f.WriteString("line " + itoa(i) + "\n")
	}
	f.Close()

	tail := New(path, 100*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Total lines should be 50
	if total := tail.GetTotalLines(); total != 50 {
		t.Errorf("expected 50 total lines, got %d", total)
	}

	// Read from the beginning
	lines := tail.ReadRange(1, 5)
	if len(lines) != 5 {
		t.Fatalf("expected 5 lines, got %d", len(lines))
	}
	if lines[0].Text != "line 1" {
		t.Errorf("expected 'line 1', got %q", lines[0].Text)
	}
	if lines[4].Text != "line 5" {
		t.Errorf("expected 'line 5', got %q", lines[4].Text)
	}

	// Read from the middle
	lines = tail.ReadRange(25, 3)
	if len(lines) != 3 {
		t.Fatalf("expected 3 lines, got %d", len(lines))
	}
	if lines[0].Text != "line 25" {
		t.Errorf("expected 'line 25', got %q", lines[0].Text)
	}
	if lines[0].Number != 25 {
		t.Errorf("expected line number 25, got %d", lines[0].Number)
	}

	// Read past end — should return only available lines
	lines = tail.ReadRange(48, 10)
	if len(lines) != 3 {
		t.Errorf("expected 3 lines (48-50), got %d", len(lines))
	}

	// Invalid ranges
	if lines := tail.ReadRange(0, 5); lines != nil {
		t.Errorf("expected nil for startLine=0, got %d lines", len(lines))
	}
	if lines := tail.ReadRange(100, 5); lines != nil {
		t.Errorf("expected nil for startLine beyond file, got %d lines", len(lines))
	}
}

func TestTailerReadRangeAfterAppend(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\nline 2\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(100 * time.Millisecond)

	if total := tail.GetTotalLines(); total != 2 {
		t.Errorf("expected 2 total lines, got %d", total)
	}

	// Append new lines
	af, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	af.WriteString("line 3\nline 4\n")
	af.Close()

	time.Sleep(200 * time.Millisecond)

	if total := tail.GetTotalLines(); total != 4 {
		t.Errorf("expected 4 total lines after append, got %d", total)
	}

	// Read the new lines
	lines := tail.ReadRange(3, 2)
	if len(lines) != 2 {
		t.Fatalf("expected 2 lines, got %d", len(lines))
	}
	if lines[0].Text != "line 3" {
		t.Errorf("expected 'line 3', got %q", lines[0].Text)
	}

	// Read original lines still works
	lines = tail.ReadRange(1, 2)
	if len(lines) != 2 {
		t.Fatalf("expected 2 lines, got %d", len(lines))
	}
	if lines[0].Text != "line 1" {
		t.Errorf("expected 'line 1', got %q", lines[0].Text)
	}
}

// Simple int to string for test use
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	s := ""
	for n > 0 {
		s = string(rune('0'+n%10)) + s
		n /= 10
	}
	return s
}
