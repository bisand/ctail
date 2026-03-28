package tailer

import (
	"bufio"
	"fmt"
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

	mu             sync.RWMutex
	lines          []Line
	lineNum        int64
	offset         int64
	fileSize       int64
	lineOffsets    []int64    // byte offset of each line start (0-indexed: lineOffsets[0] = line 1)
	fileIdent      os.FileInfo // identity of the file being tailed (for detecting replacement via inode change)
	running        bool
	inError        bool      // true when the last poll attempt failed
	lastReady      time.Time // last time onReady was fired (prevents flicker cycling)
	lastDataAt     time.Time // last time new data was read (for staleness detection)
	staleThreshold time.Duration // how long without data before watchdog triggers (default 30s)
	stopCh         chan struct{}

	onLines        func([]Line)
	onTruncated    func()
	onError        func(error)
	onReady        func() // called once after successful initial read
	onReconnecting func() // called when auto-restart is triggered
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
		filePath:       filePath,
		pollInterval:   pollInterval,
		bufferSize:     bufferSize,
		staleThreshold: 30 * time.Second,
		lines:          make([]Line, 0, bufferSize),
		lineOffsets:    make([]int64, 0, 1024),
		stopCh:         make(chan struct{}),
	}
}

// OnLines sets the callback for new lines
func (t *Tailer) OnLines(fn func([]Line)) { t.onLines = fn }

// OnTruncated sets the callback for file truncation
func (t *Tailer) OnTruncated(fn func()) { t.onTruncated = fn }

// OnError sets the callback for errors
func (t *Tailer) OnError(fn func(error)) { t.onError = fn }

// OnReady sets the callback for when the initial read completes successfully
func (t *Tailer) OnReady(fn func()) { t.onReady = fn }

// OnReconnecting sets the callback for when the tailer auto-restarts after timeouts
func (t *Tailer) OnReconnecting(fn func()) { t.onReconnecting = fn }

// Start begins tailing the file (non-blocking — initial read happens in background)
func (t *Tailer) Start() error {
	t.mu.Lock()
	if t.running {
		t.mu.Unlock()
		return nil
	}
	t.running = true
	t.mu.Unlock()

	go t.startLoop()
	return nil
}

// startLoop performs the initial read then enters the poll loop.
// Runs entirely in a goroutine so the caller of Start() never blocks on I/O.
func (t *Tailer) startLoop() {
	// Capture stopCh under lock so Restart() can safely replace it.
	t.mu.RLock()
	stopCh := t.stopCh
	t.mu.RUnlock()

	// Initial read with a timeout so stale mounts don't block forever
	initDone := make(chan error, 1)
	go func() {
		initDone <- t.initialRead()
	}()

	select {
	case err := <-initDone:
		if err != nil {
			t.mu.Lock()
			t.inError = true
			t.mu.Unlock()
			if t.onError != nil {
				t.onError(err)
			}
			// Still enter poll loop — file may become available later
		} else if t.onReady != nil {
			t.onReady()
		}
	case <-stopCh:
		return
	case <-time.After(10 * time.Second):
		if t.onError != nil {
			t.onError(fmt.Errorf("timeout reading %s (file may be on an unreachable mount)", t.filePath))
		}
		// Enter poll loop anyway — will retry on next tick
	}

	t.pollLoop()
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

// Restart stops the current polling loop and starts a fresh one.
// Existing buffered lines are preserved so the frontend sees no disruption.
// This breaks free of goroutines stuck on stale network mounts.
func (t *Tailer) Restart() {
	t.mu.Lock()
	if !t.running {
		t.mu.Unlock()
		return
	}
	// Signal the old loop to stop
	close(t.stopCh)
	// Create a fresh stop channel for the new loop.
	// Keep inError=true so the first successful poll fires onReady,
	// which tells the frontend to re-fetch lines.
	t.stopCh = make(chan struct{})
	t.lastDataAt = time.Now() // reset watchdog
	t.mu.Unlock()

	// Start the new loop — skips initialRead since we already have buffered data
	go t.pollLoop()
}

// GetLines returns the current buffer of lines
func (t *Tailer) GetLines() []Line {
	t.mu.RLock()
	defer t.mu.RUnlock()
	result := make([]Line, len(t.lines))
	copy(result, t.lines)
	return result
}

// GetTotalLines returns the total number of lines known in the file
func (t *Tailer) GetTotalLines() int64 {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return t.lineNum
}

// GetFileSize returns the current known file size in bytes
func (t *Tailer) GetFileSize() int64 {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return t.fileSize
}

// ReadRange reads lines from the file starting at startLine (1-based), returning up to count lines.
// Uses a timeout to avoid blocking on unreachable mounts.
func (t *Tailer) ReadRange(startLine int64, count int) []Line {
	if startLine < 1 || count < 1 {
		return nil
	}

	t.mu.RLock()
	totalLines := t.lineNum
	offsets := t.lineOffsets
	t.mu.RUnlock()

	if startLine > totalLines {
		return nil
	}

	idx := int(startLine - 1)
	if idx >= len(offsets) {
		return nil
	}

	byteOffset := offsets[idx]

	type result struct {
		lines []Line
	}

	ch := make(chan result, 1)
	go func() {
		f, err := os.Open(t.filePath)
		if err != nil {
			ch <- result{nil}
			return
		}
		defer f.Close()

		if _, err := f.Seek(byteOffset, io.SeekStart); err != nil {
			ch <- result{nil}
			return
		}

		scanner := bufio.NewScanner(f)
		scanner.Buffer(make([]byte, 1024*1024), 1024*1024)

		var lines []Line
		lineNo := startLine
		for scanner.Scan() && len(lines) < count {
			lines = append(lines, Line{Number: lineNo, Text: scanner.Text()})
			lineNo++
		}
		ch <- result{lines}
	}()

	select {
	case r := <-ch:
		return r.lines
	case <-time.After(5 * time.Second):
		return nil
	}
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

	fileSize := info.Size()

	// Build line offset index and read all lines (keep last N in buffer).
	// All I/O happens outside the lock to avoid blocking concurrent readers.
	reader := bufio.NewReader(f)
	var allLines []Line
	var offsets []int64
	var num int64
	var bytePos int64

	for {
		offsets = append(offsets, bytePos)
		lineBytes, err := reader.ReadBytes('\n')
		if len(lineBytes) > 0 {
			num++
			text := string(lineBytes)
			if len(text) > 0 && text[len(text)-1] == '\n' {
				text = text[:len(text)-1]
			}
			if len(text) > 0 && text[len(text)-1] == '\r' {
				text = text[:len(text)-1]
			}
			allLines = append(allLines, Line{Number: num, Text: text})
			bytePos += int64(len(lineBytes))
			if len(allLines) > t.bufferSize*2 {
				allLines = allLines[len(allLines)-t.bufferSize:]
			}
		}
		if err != nil {
			if err == io.EOF {
				break
			}
			return err
		}
	}

	if len(allLines) > t.bufferSize {
		allLines = allLines[len(allLines)-t.bufferSize:]
	}

	// Commit state under lock — short critical section, no I/O or callbacks.
	t.mu.Lock()
	t.fileSize = fileSize
	t.fileIdent = info
	t.lineOffsets = offsets
	t.lines = allLines
	t.lineNum = num
	t.offset = fileSize
	t.lastDataAt = time.Now()
	t.mu.Unlock()

	// No onLines callback — onReady (fired by startLoop) tells the frontend
	// to fetch the windowed view via loadInitialLines / ReadRange RPCs.
	return nil
}

func (t *Tailer) pollLoop() {
	// Capture stopCh under lock so Restart() can safely replace it.
	t.mu.RLock()
	stopCh := t.stopCh
	t.mu.RUnlock()

	ticker := time.NewTicker(t.pollInterval)
	defer ticker.Stop()

	// pollActive tracks whether a poll goroutine is currently running.
	// This prevents spawning new goroutines while one is blocked on I/O
	// (e.g. stale network mount), avoiding unbounded goroutine leaks.
	pollActive := make(chan struct{}, 1)

	// Backoff state: when the tailer is in error, progressively slow down
	// polling to reduce wasted I/O and event noise on unreachable mounts.
	const maxBackoff = 10 * time.Second
	var consecutiveErrors int

	// Staleness watchdog: if the file is readable and larger than our offset
	// but we haven't read new data recently, force a full re-read.
	// This catches stale offsets after network reconnections.
	t.mu.RLock()
	staleThreshold := t.staleThreshold
	t.mu.RUnlock()
	var staleChecks int

	// After this many consecutive poll timeouts, auto-restart to break free
	// of goroutines stuck on stale GVFS/SMB mounts.
	const maxConsecutiveTimeouts = 3
	var consecutiveTimeouts int

	for {
		select {
		case <-stopCh:
			return
		case <-ticker.C:
			t.mu.RLock()
			interval := t.pollInterval
			inError := t.inError
			t.mu.RUnlock()

			// Apply exponential backoff when in error state
			if inError {
				consecutiveErrors++
				backoff := interval * time.Duration(1<<min(consecutiveErrors, 5))
				if backoff > maxBackoff {
					backoff = maxBackoff
				}
				ticker.Reset(backoff)
			} else {
				consecutiveErrors = 0
				consecutiveTimeouts = 0
				ticker.Reset(interval)

				// Staleness watchdog: detect when our offset is behind the
				// actual file size but poll() isn't reading new data.
				// This catches stale state after network reconnections.
				t.mu.RLock()
				offset := t.offset
				lastData := t.lastDataAt
				t.mu.RUnlock()

				if !lastData.IsZero() && time.Since(lastData) > staleThreshold {
					if info, err := os.Stat(t.filePath); err == nil && info.Size() != offset {
						staleChecks++
						if staleChecks >= 2 {
							staleChecks = 0
							// Force a full re-read — our offset is out of sync
							if t.onReconnecting != nil {
								t.onReconnecting()
							}
							t.mu.Lock()
							t.inError = true
							t.mu.Unlock()
							go t.Restart()
							return
						}
					} else {
						staleChecks = 0
					}
				} else {
					staleChecks = 0
				}
			}

			// Skip this tick if a previous poll is still running
			select {
			case pollActive <- struct{}{}:
				// Got the slot — spawn poll goroutine
			default:
				// Previous poll still in progress, skip
				continue
			}

			go func() {
				defer func() { <-pollActive }()
				defer func() {
					if r := recover(); r != nil {
						if t.onError != nil {
							t.onError(fmt.Errorf("poll panic recovered: %v", r))
						}
					}
				}()

				pollDone := make(chan struct{})
				go func() {
					defer func() {
						if r := recover(); r != nil {
							if t.onError != nil {
								t.onError(fmt.Errorf("poll panic recovered: %v", r))
							}
						}
						close(pollDone)
					}()
					t.poll()
				}()

				select {
				case <-pollDone:
				case <-stopCh:
					return
				case <-time.After(5 * time.Second):
					// Poll timed out (likely stale mount) — report error and move on.
					// The blocked goroutine will eventually return when the OS times out.
					t.mu.Lock()
					wasInError := t.inError
					t.inError = true
					t.mu.Unlock()
					if !wasInError && t.onError != nil {
						t.onError(fmt.Errorf("timeout polling %s (file may be on an unreachable mount)", t.filePath))
					}

					consecutiveTimeouts++
					if consecutiveTimeouts >= maxConsecutiveTimeouts {
						if t.onReconnecting != nil {
							t.onReconnecting()
						}
						// Restart creates a fresh pollLoop, abandoning stuck goroutines.
						// This pollLoop will exit when it sees stopCh closed by Restart().
						go t.Restart()
					}
				}
			}()
		}
	}
}

// readyCooldown is the minimum interval between onReady callbacks to prevent
// rapid ready→error→ready cycling on flaky network connections.
const readyCooldown = 5 * time.Second

func (t *Tailer) poll() {
	info, err := os.Stat(t.filePath)
	if err != nil {
		t.mu.Lock()
		wasInError := t.inError
		t.inError = true
		t.mu.Unlock()
		// Only fire the error callback on the transition from OK → error
		// to avoid flooding the frontend with repeated identical errors.
		if !wasInError && t.onError != nil {
			t.onError(err)
		}
		return
	}

	currentSize := info.Size()

	t.mu.Lock()
	wasInError := t.inError
	t.inError = false
	prevSize := t.fileSize
	prevOffset := t.offset
	prevIdent := t.fileIdent
	t.mu.Unlock()

	// Detect file replacement (different inode) — the old file was renamed
	// and a new one created at the same path.  Treat as truncation even if
	// the new file is already larger than the old one.
	replaced := prevIdent != nil && !os.SameFile(prevIdent, info)
	if replaced {
		t.mu.Lock()
		t.fileIdent = info
		t.mu.Unlock()
	}

	// File is accessible again after an error — do a full re-read since our
	// offset/lineNum state may be completely stale after a network drop.
	// The file could have been rotated, truncated, or had new content written
	// while the mount was unreachable.
	if wasInError {
		rf, err := os.Open(t.filePath)
		if err != nil {
			t.mu.Lock()
			t.inError = true
			t.mu.Unlock()
			if t.onError != nil {
				t.onError(err)
			}
			return
		}
		t.handleFullReread(rf, currentSize, info)
		rf.Close()

		// Notify frontend that the tab is ready again.
		// Apply a cooldown to prevent rapid ready→error→ready cycling on flaky connections.
		if t.onReady != nil {
			t.mu.RLock()
			lastReady := t.lastReady
			t.mu.RUnlock()
			if time.Since(lastReady) >= readyCooldown {
				t.mu.Lock()
				t.lastReady = time.Now()
				t.mu.Unlock()
				t.onReady()
			}
		}
		return
	}

	// No new data and file not replaced
	if currentSize == prevOffset && !replaced {
		return
	}

	// Only open the file when there's something to read
	f, err := os.Open(t.filePath)
	if err != nil {
		t.mu.Lock()
		wasInError2 := t.inError
		t.inError = true
		t.mu.Unlock()
		if !wasInError2 && t.onError != nil {
			t.onError(err)
		}
		return
	}
	defer f.Close()

	// Detect truncation (size shrank) or file replacement (different inode)
	if currentSize < prevSize || replaced {
		t.handleTruncation(f, currentSize)
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
		t.lastDataAt = time.Now()
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
	t.lineOffsets = t.lineOffsets[:0]
	t.lastDataAt = time.Now()
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

// handleFullReread resets internal state and re-reads the file from scratch.
// Used on recovery from errors (e.g. VPN reconnection) where the offset/lineNum
// state may be stale because the file changed while the mount was unreachable.
// Does NOT fire onLines — the caller fires onReady which triggers the frontend
// to fetch lines via loadInitialLines with proper windowed pagination.
func (t *Tailer) handleFullReread(f *os.File, currentSize int64, info os.FileInfo) {
	t.mu.Lock()
	t.lines = t.lines[:0]
	t.lineNum = 0
	t.offset = 0
	t.fileSize = currentSize
	t.fileIdent = info
	t.lineOffsets = t.lineOffsets[:0]
	t.lastDataAt = time.Now()
	t.mu.Unlock()

	if t.onTruncated != nil {
		t.onTruncated()
	}

	newLines := t.readNewLines(f, 0, currentSize)
	if len(newLines) > 0 {
		t.mu.Lock()
		t.lines = newLines
		if len(t.lines) > t.bufferSize {
			t.lines = t.lines[len(t.lines)-t.bufferSize:]
		}
		t.offset = currentSize
		t.mu.Unlock()
		// No onLines callback — onReady will tell the frontend to re-fetch.
	}
}

func (t *Tailer) readNewLines(f *os.File, offset, size int64) []Line {
	if _, err := f.Seek(offset, io.SeekStart); err != nil {
		return nil
	}

	reader := bufio.NewReader(f)
	var newLines []Line
	var newOffsets []int64

	t.mu.RLock()
	num := t.lineNum
	t.mu.RUnlock()

	bytePos := offset
	for {
		newOffsets = append(newOffsets, bytePos)

		lineBytes, err := reader.ReadBytes('\n')
		if len(lineBytes) > 0 {
			num++
			text := string(lineBytes)
			if len(text) > 0 && text[len(text)-1] == '\n' {
				text = text[:len(text)-1]
			}
			if len(text) > 0 && text[len(text)-1] == '\r' {
				text = text[:len(text)-1]
			}
			newLines = append(newLines, Line{Number: num, Text: text})
			bytePos += int64(len(lineBytes))
		}
		if err != nil {
			break
		}
	}

	t.mu.Lock()
	t.lineOffsets = append(t.lineOffsets, newOffsets...)
	t.lineNum = num
	t.mu.Unlock()

	return newLines
}
