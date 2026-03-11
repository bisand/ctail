package tailer

import (
	"bufio"
	"io"
	"os"
	"sync"
	"time"
)

// Line represents a single log line with its number
type Line struct {
	Number int64  `json:"number"`
	Text   string `json:"text"`
}

// Tailer watches a file and streams new lines
type Tailer struct {
	filePath     string
	pollInterval time.Duration
	bufferSize   int

	mu       sync.RWMutex
	lines    []Line
	lineNum  int64
	offset   int64
	fileSize int64
	running  bool
	stopCh   chan struct{}

	onLines     func([]Line)
	onTruncated func()
	onError     func(error)
}

// New creates a new Tailer
func New(filePath string, pollInterval time.Duration, bufferSize int) *Tailer {
	if bufferSize < 100 {
		bufferSize = 100
	}
	if pollInterval < 50*time.Millisecond {
		pollInterval = 50 * time.Millisecond
	}
	return &Tailer{
		filePath:     filePath,
		pollInterval: pollInterval,
		bufferSize:   bufferSize,
		lines:        make([]Line, 0, bufferSize),
		stopCh:       make(chan struct{}),
	}
}

// OnLines sets the callback for new lines
func (t *Tailer) OnLines(fn func([]Line)) { t.onLines = fn }

// OnTruncated sets the callback for file truncation
func (t *Tailer) OnTruncated(fn func()) { t.onTruncated = fn }

// OnError sets the callback for errors
func (t *Tailer) OnError(fn func(error)) { t.onError = fn }

// Start begins tailing the file
func (t *Tailer) Start() error {
	t.mu.Lock()
	if t.running {
		t.mu.Unlock()
		return nil
	}
	t.running = true
	t.mu.Unlock()

	// Initial read: read last bufferSize lines
	if err := t.initialRead(); err != nil {
		return err
	}

	go t.pollLoop()
	return nil
}

// Stop stops tailing
func (t *Tailer) Stop() {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.running {
		return
	}
	t.running = false
	close(t.stopCh)
}

// GetLines returns the current buffer of lines
func (t *Tailer) GetLines() []Line {
	t.mu.RLock()
	defer t.mu.RUnlock()
	result := make([]Line, len(t.lines))
	copy(result, t.lines)
	return result
}

// SetPollInterval updates the poll interval
func (t *Tailer) SetPollInterval(d time.Duration) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.pollInterval = d
}

func (t *Tailer) initialRead() error {
	f, err := os.Open(t.filePath)
	if err != nil {
		return err
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return err
	}

	t.mu.Lock()
	defer t.mu.Unlock()

	t.fileSize = info.Size()

	// Read all lines (for initial load we scan the whole file but only keep last N)
	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024) // 1MB max line
	var allLines []Line
	var num int64
	for scanner.Scan() {
		num++
		allLines = append(allLines, Line{Number: num, Text: scanner.Text()})
		// Keep ring buffer bounded during scanning
		if len(allLines) > t.bufferSize*2 {
			allLines = allLines[len(allLines)-t.bufferSize:]
		}
	}
	if err := scanner.Err(); err != nil {
		return err
	}

	// Keep last bufferSize lines
	if len(allLines) > t.bufferSize {
		allLines = allLines[len(allLines)-t.bufferSize:]
	}
	t.lines = allLines
	t.lineNum = num
	t.offset = t.fileSize

	// Notify with initial lines
	if t.onLines != nil && len(allLines) > 0 {
		t.onLines(allLines)
	}

	return nil
}

func (t *Tailer) pollLoop() {
	ticker := time.NewTicker(t.pollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-t.stopCh:
			return
		case <-ticker.C:
			t.mu.RLock()
			interval := t.pollInterval
			t.mu.RUnlock()
			ticker.Reset(interval)
			t.poll()
		}
	}
}

func (t *Tailer) poll() {
	f, err := os.Open(t.filePath)
	if err != nil {
		if t.onError != nil {
			t.onError(err)
		}
		return
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		if t.onError != nil {
			t.onError(err)
		}
		return
	}

	currentSize := info.Size()

	t.mu.Lock()
	prevSize := t.fileSize
	prevOffset := t.offset
	t.mu.Unlock()

	// Detect truncation
	if currentSize < prevSize {
		t.handleTruncation(f, currentSize)
		return
	}

	// No new data
	if currentSize == prevOffset {
		return
	}

	// Read new data from offset
	newLines := t.readNewLines(f, prevOffset, currentSize)
	if len(newLines) > 0 {
		t.mu.Lock()
		t.lines = append(t.lines, newLines...)
		if len(t.lines) > t.bufferSize {
			t.lines = t.lines[len(t.lines)-t.bufferSize:]
		}
		t.fileSize = currentSize
		t.offset = currentSize
		t.mu.Unlock()

		if t.onLines != nil {
			t.onLines(newLines)
		}
	}
}

func (t *Tailer) handleTruncation(f *os.File, currentSize int64) {
	t.mu.Lock()
	t.lines = t.lines[:0]
	t.lineNum = 0
	t.offset = 0
	t.fileSize = currentSize
	t.mu.Unlock()

	if t.onTruncated != nil {
		t.onTruncated()
	}

	// Re-read from beginning
	newLines := t.readNewLines(f, 0, currentSize)
	if len(newLines) > 0 {
		t.mu.Lock()
		t.lines = newLines
		if len(t.lines) > t.bufferSize {
			t.lines = t.lines[len(t.lines)-t.bufferSize:]
		}
		t.offset = currentSize
		t.mu.Unlock()

		if t.onLines != nil {
			t.onLines(newLines)
		}
	}
}

func (t *Tailer) readNewLines(f *os.File, offset, size int64) []Line {
	if _, err := f.Seek(offset, io.SeekStart); err != nil {
		return nil
	}

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)
	var newLines []Line

	t.mu.RLock()
	num := t.lineNum
	t.mu.RUnlock()

	for scanner.Scan() {
		num++
		newLines = append(newLines, Line{Number: num, Text: scanner.Text()})
	}

	t.mu.Lock()
	t.lineNum = num
	t.mu.Unlock()

	return newLines
}
