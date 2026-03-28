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

	tail := New(path, 100*time.Millisecond, 1000)

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read
	time.Sleep(200 * time.Millisecond)

	lines := tail.GetLines()
	if len(lines) != 3 {
		t.Errorf("expected 3 buffered lines, got %d", len(lines))
	}
	if len(lines) > 0 && lines[0].Text != "line 1" {
		t.Errorf("expected 'line 1', got %q", lines[0].Text)
	}
	if tail.GetTotalLines() != 3 {
		t.Errorf("expected 3 total lines, got %d", tail.GetTotalLines())
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

	truncated := make(chan struct{}, 1)

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnTruncated(func() {
		select {
		case truncated <- struct{}{}:
		default:
		}
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(150 * time.Millisecond)

	// Truncate then write new content
	if err := os.Truncate(path, 0); err != nil {
		t.Fatal(err)
	}

	// Wait for truncation callback (up to 1s)
	select {
	case <-truncated:
	case <-time.After(1 * time.Second):
		t.Fatal("timed out waiting for truncation callback")
	}

	// Now write fresh content
	if err := os.WriteFile(path, []byte("fresh\n"), 0644); err != nil {
		t.Fatal(err)
	}

	// Wait for the tailer to pick up the new content
	time.Sleep(200 * time.Millisecond)

	lines := tail.GetLines()
	if len(lines) != 1 {
		t.Errorf("expected 1 line after truncation, got %d", len(lines))
	}
}

func TestTailerFileReplacement(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	// Create file with initial content
	if err := os.WriteFile(path, []byte("old line 1\nold line 2\nold line 3\n"), 0644); err != nil {
		t.Fatal(err)
	}

	truncated := make(chan struct{}, 1)
	var mu sync.Mutex
	var newLines []Line

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnTruncated(func() {
		select {
		case truncated <- struct{}{}:
		default:
		}
	})
	tail.OnLines(func(lines []Line) {
		mu.Lock()
		newLines = append(newLines, lines...)
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(150 * time.Millisecond)

	// Simulate log rotation: rename old file, create new one with MORE content
	// than the old file (this is the case the old code missed)
	rotatedPath := path + ".1"
	if err := os.Rename(path, rotatedPath); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("new line 1\nnew line 2\nnew line 3\nnew line 4\nnew line 5\n"), 0644); err != nil {
		t.Fatal(err)
	}

	// Wait for truncation callback (file replacement detected via inode change)
	select {
	case <-truncated:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for file replacement detection")
	}

	// Wait for new content to be read
	time.Sleep(200 * time.Millisecond)

	lines := tail.GetLines()
	if len(lines) != 5 {
		t.Errorf("expected 5 lines after file replacement, got %d", len(lines))
		for _, l := range lines {
			t.Logf("  line %d: %q", l.Number, l.Text)
		}
	}
	if len(lines) > 0 && lines[0].Text != "new line 1" {
		t.Errorf("expected first line to be 'new line 1', got %q", lines[0].Text)
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

func TestTailerErrorRecovery(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var errors []string
	readyCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnError(func(err error) {
		mu.Lock()
		errors = append(errors, err.Error())
		mu.Unlock()
	})
	tail.OnReady(func() {
		mu.Lock()
		readyCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if readyCount != 1 {
		t.Errorf("expected 1 initial ready, got %d", readyCount)
	}
	mu.Unlock()

	// Remove the file to trigger errors
	os.Remove(path)
	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if len(errors) == 0 {
		t.Error("expected at least one error after file removal")
	}
	mu.Unlock()

	// Recreate the file — should trigger recovery (onReady again)
	// Wait long enough for the readyCooldown (5s) to pass since initial onReady
	if err := os.WriteFile(path, []byte("line 2\n"), 0644); err != nil {
		t.Fatal(err)
	}
	time.Sleep(5500 * time.Millisecond)

	mu.Lock()
	if readyCount < 2 {
		t.Errorf("expected onReady to fire again after recovery, got readyCount=%d", readyCount)
	}
	mu.Unlock()
}

func TestTailerRestart(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\nline 2\nline 3\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Should have initial 3 lines in buffer
	existingLines := tail.GetLines()
	if len(existingLines) != 3 {
		t.Fatalf("expected 3 buffered lines before restart, got %d", len(existingLines))
	}

	// Restart the tailer
	tail.Restart()
	time.Sleep(200 * time.Millisecond)

	// Lines buffer should still be preserved after restart
	afterLines := tail.GetLines()
	if len(afterLines) != 3 {
		t.Fatalf("expected 3 buffered lines after restart, got %d", len(afterLines))
	}
	if afterLines[0].Text != "line 1" {
		t.Errorf("expected 'line 1' after restart, got %q", afterLines[0].Text)
	}

	// Append new data — should be picked up by the new poll loop
	af, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	af.WriteString("line 4\n")
	af.Close()

	time.Sleep(200 * time.Millisecond)

	finalLines := tail.GetLines()
	if len(finalLines) < 4 {
		t.Errorf("expected at least 4 lines after restart+append, got %d", len(finalLines))
	}
}

func TestTailerRestartClearsErrorState(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var errors []string
	readyCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnError(func(err error) {
		mu.Lock()
		errors = append(errors, err.Error())
		mu.Unlock()
	})
	tail.OnReady(func() {
		mu.Lock()
		readyCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Remove file to trigger error state
	os.Remove(path)
	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if len(errors) == 0 {
		t.Fatal("expected at least one error after file removal")
	}
	mu.Unlock()

	// Recreate the file, then restart — inError stays true so the first
	// successful poll fires onReady (the error→ready transition).
	if err := os.WriteFile(path, []byte("line 2\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail.Restart()
	time.Sleep(300 * time.Millisecond)

	// After restart + successful poll, onReady should have fired
	mu.Lock()
	if readyCount < 2 {
		t.Errorf("expected onReady to fire after restart recovery, got readyCount=%d", readyCount)
	}
	mu.Unlock()
}

func TestTailerRestartOnReconnectingCallback(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	reconnectingCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnReconnecting(func() {
		mu.Lock()
		reconnectingCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Manual restart should NOT fire onReconnecting (it's for auto-restarts only)
	tail.Restart()
	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	count := reconnectingCount
	mu.Unlock()

	if count != 0 {
		t.Errorf("expected onReconnecting not to fire on manual restart, got count=%d", count)
	}
}

func TestTailerRestartStoppedIsNoop(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}

	time.Sleep(200 * time.Millisecond)

	// Stop, then restart — should be a no-op, not panic
	tail.Stop()
	tail.Restart() // must not panic or start new goroutines
	time.Sleep(100 * time.Millisecond)

	// Verify it's still stopped — no new lines picked up
	tail.mu.RLock()
	running := tail.running
	tail.mu.RUnlock()

	if running {
		t.Error("expected tailer to remain stopped after Restart() on stopped tailer")
	}
}

func TestTailerMultipleRestarts(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
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

	time.Sleep(200 * time.Millisecond)

	// Restart multiple times in succession
	for i := 0; i < 5; i++ {
		tail.Restart()
		time.Sleep(100 * time.Millisecond)
	}

	// Buffer should still be intact
	lines := tail.GetLines()
	if len(lines) != 1 {
		t.Fatalf("expected 1 buffered line after multiple restarts, got %d", len(lines))
	}
	if lines[0].Text != "line 1" {
		t.Errorf("expected 'line 1', got %q", lines[0].Text)
	}

	// New data should still be picked up after repeated restarts
	af, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	af.WriteString("line 2\n")
	af.Close()

	time.Sleep(200 * time.Millisecond)

	lines = tail.GetLines()
	if len(lines) != 2 {
		t.Errorf("expected 2 lines after append following multiple restarts, got %d", len(lines))
	}
}

func TestTailerRestartPreservesLineNumbers(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\nline 2\nline 3\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	if total := tail.GetTotalLines(); total != 3 {
		t.Fatalf("expected 3 total lines, got %d", total)
	}

	tail.Restart()
	time.Sleep(200 * time.Millisecond)

	// Total line count should be preserved
	if total := tail.GetTotalLines(); total != 3 {
		t.Errorf("expected 3 total lines after restart, got %d", total)
	}

	// Append new lines — numbering should continue from 3
	af, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	af.WriteString("line 4\nline 5\n")
	af.Close()

	time.Sleep(200 * time.Millisecond)

	if total := tail.GetTotalLines(); total != 5 {
		t.Errorf("expected 5 total lines after append, got %d", total)
	}

	lines := tail.GetLines()
	last := lines[len(lines)-1]
	if last.Number != 5 {
		t.Errorf("expected last line number 5, got %d", last.Number)
	}
	if last.Text != "line 5" {
		t.Errorf("expected 'line 5', got %q", last.Text)
	}
}

func TestTailerRestartAfterFileRecreation(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("original\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var errors []string
	readyCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.OnError(func(err error) {
		mu.Lock()
		errors = append(errors, err.Error())
		mu.Unlock()
	})
	tail.OnReady(func() {
		mu.Lock()
		readyCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Delete the file
	os.Remove(path)
	time.Sleep(200 * time.Millisecond)

	mu.Lock()
	if len(errors) == 0 {
		t.Fatal("expected error after file removal")
	}
	mu.Unlock()

	// Recreate with new content
	if err := os.WriteFile(path, []byte("new content\n"), 0644); err != nil {
		t.Fatal(err)
	}

	// Restart to simulate recovery from stale mount
	tail.Restart()
	time.Sleep(300 * time.Millisecond)

	// Poll should now detect the file. Since the file was recreated with different
	// content, the tailer should pick up new data if the size differs from the
	// stored offset.
	mu.Lock()
	ready := readyCount
	mu.Unlock()

	// readyCount >= 1 from initial read, we mainly confirm no panic/hang
	if ready < 1 {
		t.Errorf("expected at least 1 ready callback, got %d", ready)
	}
}

func TestTailerStopAfterRestart(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}

	time.Sleep(200 * time.Millisecond)

	tail.Restart()
	time.Sleep(100 * time.Millisecond)

	// Stop must not panic after Restart (new stopCh was created)
	tail.Stop()

	// Wait for the poll loop to fully exit
	time.Sleep(200 * time.Millisecond)

	// Append data — should NOT be picked up since we stopped
	linesBefore := tail.GetLines()

	af, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	af.WriteString("line 2\n")
	af.Close()

	time.Sleep(200 * time.Millisecond)

	linesAfter := tail.GetLines()
	if len(linesAfter) != len(linesBefore) {
		t.Errorf("expected no new lines after Stop(), had %d now %d", len(linesBefore), len(linesAfter))
	}
}

func TestTailerRestartConcurrentSafety(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")

	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Hammer Restart from multiple goroutines — must not panic or deadlock
	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			tail.Restart()
		}()
	}

	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("concurrent Restart() calls deadlocked")
	}

	// Tailer should still be functional
	time.Sleep(200 * time.Millisecond)
	lines := tail.GetLines()
	if len(lines) != 1 {
		t.Errorf("expected 1 line after concurrent restarts, got %d", len(lines))
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

// --- Resilience tests ---

// TestTailerLastDataAtTracking verifies that lastDataAt is set on initial read
// and updated when new lines are appended.
func TestTailerLastDataAtTracking(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read
	time.Sleep(200 * time.Millisecond)

	tail.mu.RLock()
	initialDataAt := tail.lastDataAt
	tail.mu.RUnlock()

	if initialDataAt.IsZero() {
		t.Fatal("lastDataAt should be set after initial read")
	}

	// Append new lines
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString("line 2\n")
	f.Close()

	// Wait for poll to pick up new data
	time.Sleep(300 * time.Millisecond)

	tail.mu.RLock()
	updatedDataAt := tail.lastDataAt
	tail.mu.RUnlock()

	if !updatedDataAt.After(initialDataAt) {
		t.Errorf("lastDataAt should advance after new lines: initial=%v, updated=%v", initialDataAt, updatedDataAt)
	}
}

// TestTailerLastDataAtOnTruncation verifies that lastDataAt is updated
// when the file is truncated.
func TestTailerLastDataAtOnTruncation(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\nline 2\nline 3\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	tail.mu.RLock()
	beforeTruncate := tail.lastDataAt
	tail.mu.RUnlock()

	// Truncate and write new content
	time.Sleep(10 * time.Millisecond) // ensure clock ticks
	if err := os.WriteFile(path, []byte("new\n"), 0644); err != nil {
		t.Fatal(err)
	}

	time.Sleep(300 * time.Millisecond)

	tail.mu.RLock()
	afterTruncate := tail.lastDataAt
	tail.mu.RUnlock()

	if !afterTruncate.After(beforeTruncate) {
		t.Errorf("lastDataAt should advance after truncation: before=%v, after=%v", beforeTruncate, afterTruncate)
	}
}

// TestTailerStalenessWatchdog verifies that the watchdog detects when the
// tailer's offset is out of sync with the actual file size and triggers
// an auto-restart.
func TestTailerStalenessWatchdog(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	reconnectCount := 0
	readyCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.staleThreshold = 1 * time.Millisecond // very short for testing

	tail.OnReconnecting(func() {
		mu.Lock()
		reconnectCount++
		mu.Unlock()
	})
	tail.OnReady(func() {
		mu.Lock()
		readyCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read
	time.Sleep(300 * time.Millisecond)

	mu.Lock()
	rc := readyCount
	mu.Unlock()
	if rc < 1 {
		t.Fatal("expected initial onReady")
	}

	// Now simulate a stale state: set lastDataAt far in the past and
	// corrupt the offset so it doesn't match the file size.
	// The watchdog should detect this mismatch and trigger Restart.
	tail.mu.Lock()
	tail.lastDataAt = time.Now().Add(-1 * time.Hour)
	tail.offset = 999999 // doesn't match actual file size (7 bytes)
	tail.mu.Unlock()

	// Wait for the watchdog to trigger (needs 2 consecutive stale checks)
	time.Sleep(500 * time.Millisecond)

	mu.Lock()
	rc2 := reconnectCount
	mu.Unlock()

	if rc2 < 1 {
		t.Errorf("expected onReconnecting from staleness watchdog, got count=%d", rc2)
	}
}

// TestTailerStalenessWatchdogNoFalsePositive verifies that the watchdog
// does NOT trigger when the file size matches our offset (healthy state).
func TestTailerStalenessWatchdogNoFalsePositive(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	reconnectCount := 0

	tail := New(path, 50*time.Millisecond, 1000)
	tail.staleThreshold = 1 * time.Millisecond // very short for testing

	tail.OnReconnecting(func() {
		mu.Lock()
		reconnectCount++
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read and several poll cycles
	time.Sleep(500 * time.Millisecond)

	// The file hasn't changed and offset should match — no reconnect expected
	mu.Lock()
	rc := reconnectCount
	mu.Unlock()

	if rc != 0 {
		t.Errorf("watchdog should NOT trigger when offset matches file size, got reconnectCount=%d", rc)
	}
}

// TestTailerStalenessRecovery verifies that after the watchdog triggers a
// restart, the tailer resumes reading new lines normally.
func TestTailerStalenessRecovery(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var allLines []Line

	tail := New(path, 50*time.Millisecond, 1000)
	tail.staleThreshold = 1 * time.Millisecond

	tail.OnLines(func(lines []Line) {
		mu.Lock()
		allLines = append(allLines, lines...)
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	// Force a stale state
	tail.mu.Lock()
	tail.lastDataAt = time.Now().Add(-1 * time.Hour)
	tail.offset = 999999
	tail.mu.Unlock()

	// Wait for watchdog to restart
	time.Sleep(500 * time.Millisecond)

	// Clear collected lines from restart re-read
	mu.Lock()
	allLines = nil
	mu.Unlock()

	// Write new data — after restart the tailer should pick it up
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString("line 2\n")
	f.Close()

	time.Sleep(500 * time.Millisecond)

	mu.Lock()
	count := len(allLines)
	mu.Unlock()

	if count == 0 {
		t.Error("expected new lines to be delivered after staleness recovery")
	}
}

// TestTailerPollPanicRecovery verifies that if a callback panics, the poll
// loop survives and continues polling.
func TestTailerPollPanicRecovery(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	var mu sync.Mutex
	var errors []string
	callCount := 0

	tail := New(path, 50*time.Millisecond, 1000)

	// OnLines will panic on first call, then work normally
	tail.OnLines(func(lines []Line) {
		mu.Lock()
		callCount++
		c := callCount
		mu.Unlock()
		if c == 1 {
			panic("test panic in OnLines callback")
		}
	})

	tail.OnError(func(err error) {
		mu.Lock()
		errors = append(errors, err.Error())
		mu.Unlock()
	})

	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	// Wait for initial read
	time.Sleep(200 * time.Millisecond)

	// Write new data to trigger OnLines (which will panic on first call)
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString("line 2\n")
	f.Close()

	// Wait for the panic to happen and be recovered
	time.Sleep(300 * time.Millisecond)

	mu.Lock()
	panicErrors := 0
	for _, e := range errors {
		if len(e) > 5 && e[:5] == "poll " {
			panicErrors++
		}
	}
	mu.Unlock()

	if panicErrors == 0 {
		t.Error("expected a panic recovery error to be reported")
	}

	// Write more data — the poll loop should still be alive
	f2, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		t.Fatal(err)
	}
	f2.WriteString("line 3\n")
	f2.Close()

	time.Sleep(300 * time.Millisecond)

	// The tailer should still be running and have picked up new data
	lines := tail.GetLines()
	if len(lines) == 0 {
		t.Error("tailer should still be running after panic recovery")
	}

	mu.Lock()
	finalCallCount := callCount
	mu.Unlock()

	if finalCallCount < 2 {
		t.Errorf("OnLines should have been called multiple times after recovery, got %d", finalCallCount)
	}
}

// TestTailerRestartResetsLastDataAt verifies that Restart() resets the
// watchdog timer so it doesn't immediately trigger again.
func TestTailerRestartResetsLastDataAt(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.log")
	if err := os.WriteFile(path, []byte("line 1\n"), 0644); err != nil {
		t.Fatal(err)
	}

	tail := New(path, 50*time.Millisecond, 1000)
	if err := tail.Start(); err != nil {
		t.Fatal(err)
	}
	defer tail.Stop()

	time.Sleep(200 * time.Millisecond)

	tail.Restart()
	time.Sleep(100 * time.Millisecond)

	tail.mu.RLock()
	lastData := tail.lastDataAt
	tail.mu.RUnlock()

	// lastDataAt should have been reset to ~now by Restart
	if time.Since(lastData) > 2*time.Second {
		t.Errorf("Restart should reset lastDataAt, but it's %v old", time.Since(lastData))
	}
}
