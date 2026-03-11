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
