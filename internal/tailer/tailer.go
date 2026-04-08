package tailer

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"regexp"
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
	readTimeout    time.Duration // I/O timeout for file reads (default 30s)
	stopCh         chan struct{}

	// Tail-first fields for large file support
	tailFirstThreshold int64         // file size above which tail-first mode is used (default 1MB)
	tailSeekBack       int64         // bytes to seek back from end in tail-first mode (default 512KB)
	indexingComplete   bool          // true when background indexing has finished
	indexedBytes       int64         // bytes indexed so far (for progress reporting)
	indexStopCh        chan struct{} // signal to cancel background indexing

	onLines        func([]Line)
	onTruncated    func()
	onError        func(error)
	onReady        func()        // called once after successful initial read
	onReconnecting func()        // called when auto-restart is triggered
	onIndexed      func(int64)  // called when background indexing completes (large files only)
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
		filePath:           filePath,
		pollInterval:       pollInterval,
		bufferSize:         bufferSize,
		staleThreshold:     30 * time.Second,
		readTimeout:        30 * time.Second,
		tailFirstThreshold: 1 * 1024 * 1024,   // 1MB
		tailSeekBack:       512 * 1024,          // 512KB
		indexingComplete:   true,                 // true until a tail-first read makes it false
		lines:              make([]Line, 0, bufferSize),
		lineOffsets:        make([]int64, 0, 1024),
		stopCh:             make(chan struct{}),
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

// OnIndexed sets the callback fired when background indexing completes for large files.
// The argument is the final total line count. Not called for small files (indexing is
// synchronous for those and complete before OnReady fires).
func (t *Tailer) OnIndexed(fn func(int64)) { t.onIndexed = fn }

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

	// Adaptive timeout: for large files (especially on SMB), scale the timeout
	// with file size assuming at least 256KB/s throughput. Capped at 5 minutes.
	// For tail-first reads this is generous since we only read the tail portion.
	//
	// IMPORTANT: os.Stat on GVFS/SMB mounts can block for many seconds, so we
	// run it in a goroutine with its own timeout. If Stat hangs we fall back to
	// a conservative 5-minute cap so the initialRead timeout still fires.
	t.mu.RLock()
	readTimeout := t.readTimeout
	t.mu.RUnlock()

	adaptiveTimeout := readTimeout
	statDone := make(chan int64, 1)
	go func() {
		if info, err := os.Stat(t.filePath); err == nil {
			statDone <- info.Size()
		} else {
			statDone <- 0
		}
	}()
	select {
	case size := <-statDone:
		if size > 0 {
			sizeBasedTimeout := time.Duration(size/(256*1024)+1) * time.Second
			if sizeBasedTimeout > adaptiveTimeout {
				adaptiveTimeout = sizeBasedTimeout
			}
			const maxTimeout = 5 * time.Minute
			if adaptiveTimeout > maxTimeout {
				adaptiveTimeout = maxTimeout
			}
		}
	case <-time.After(3 * time.Second):
		// Stat timed out (likely slow GVFS/SMB mount) — use a generous cap
		// so the initialRead timeout still fires rather than never.
		const conservativeTimeout = 5 * time.Minute
		if adaptiveTimeout < conservativeTimeout {
			adaptiveTimeout = conservativeTimeout
		}
	}

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
	case <-time.After(adaptiveTimeout):
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
	// Cancel background indexing if running
	if t.indexStopCh != nil {
		select {
		case <-t.indexStopCh:
			// Already closed
		default:
			close(t.indexStopCh)
		}
	}
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
	// Cancel background indexing if running
	if t.indexStopCh != nil {
		select {
		case <-t.indexStopCh:
		default:
			close(t.indexStopCh)
		}
	}
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
// Returns nil if the requested range is not yet indexed (background indexing in progress).
func (t *Tailer) ReadRange(startLine int64, count int) []Line {
	if startLine < 1 || count < 1 {
		return nil
	}

	t.mu.RLock()
	totalLines := t.lineNum
	offsets := t.lineOffsets
	indexComplete := t.indexingComplete
	t.mu.RUnlock()

	if startLine > totalLines {
		return nil
	}

	idx := int(startLine - 1)
	if idx >= len(offsets) {
		// Requested line is beyond the indexed range.
		// During background indexing after a tail-first read, this is expected
		// for lines in the early portion of the file.
		if !indexComplete {
			return nil
		}
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

	t.mu.RLock()
	readTimeout := t.readTimeout
	t.mu.RUnlock()

	select {
	case r := <-ch:
		return r.lines
	case <-time.After(readTimeout):
		return nil
	}
}

// SearchResult holds the result of a full-file search.
type SearchResult struct {
	MatchLineNumbers []int64 `json:"matchLineNumbers"`
	TotalMatches     int     `json:"totalMatches"`
	TotalLines       int64   `json:"totalLines"`
}

// SearchLines scans the entire file for lines matching the compiled regex,
// returning all matching line numbers. Uses a timeout to handle unreachable mounts.
func (t *Tailer) SearchLines(re *regexp.Regexp) SearchResult {
	if re == nil {
		return SearchResult{}
	}

	t.mu.RLock()
	filePath := t.filePath
	t.mu.RUnlock()

	type result struct {
		matches    []int64
		totalLines int64
	}

	ch := make(chan result, 1)
	go func() {
		f, err := os.Open(filePath)
		if err != nil {
			ch <- result{nil, 0}
			return
		}
		defer f.Close()

		scanner := bufio.NewScanner(f)
		scanner.Buffer(make([]byte, 1024*1024), 1024*1024)

		var matches []int64
		var lineNo int64
		for scanner.Scan() {
			lineNo++
			if re.MatchString(scanner.Text()) {
				matches = append(matches, lineNo)
			}
		}
		ch <- result{matches, lineNo}
	}()

	t.mu.RLock()
	readTimeout := t.readTimeout
	t.mu.RUnlock()

	select {
	case r := <-ch:
		return SearchResult{
			MatchLineNumbers: r.matches,
			TotalMatches:     len(r.matches),
			TotalLines:       r.totalLines,
		}
	case <-time.After(readTimeout):
		return SearchResult{}
	}
}

// SetPollInterval updates the poll interval
func (t *Tailer) SetPollInterval(d time.Duration) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.pollInterval = d
}

// SetReadTimeout updates the I/O timeout for file reads.
// This controls how long the tailer waits for file operations on slow/remote mounts.
func (t *Tailer) SetReadTimeout(d time.Duration) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if d < 5*time.Second {
		d = 5 * time.Second
	}
	t.readTimeout = d
}

// SetTailFirstThreshold sets the file size above which tail-first mode is used.
// Files larger than this will seek near the end for fast initial display,
// then index the rest in the background. Set to 0 to disable tail-first mode.
func (t *Tailer) SetTailFirstThreshold(bytes int64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.tailFirstThreshold = bytes
}

// IsIndexingComplete returns true when the full line-offset index is built.
// During background indexing after a tail-first read, ReadRange for early
// portions of the file may return nil until indexing completes.
func (t *Tailer) IsIndexingComplete() bool {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return t.indexingComplete
}

// GetIndexProgress returns (indexedBytes, totalBytes) for the background indexer.
// If indexing is complete or was never needed, both values equal the file size.
func (t *Tailer) GetIndexProgress() (int64, int64) {
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.indexingComplete {
		return t.fileSize, t.fileSize
	}
	return t.indexedBytes, t.fileSize
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

	t.mu.RLock()
	threshold := t.tailFirstThreshold
	t.mu.RUnlock()

	// Large files: tail-first for fast initial display
	if threshold > 0 && fileSize > threshold {
		return t.initialReadTail(f, info, fileSize)
	}

	return t.initialReadFull(f, info, fileSize)
}

// initialReadFull reads the entire file, building a complete line-offset index.
// Used for small files that fit comfortably within the read timeout.
func (t *Tailer) initialReadFull(f *os.File, info os.FileInfo, fileSize int64) error {
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
	t.indexingComplete = true
	t.indexedBytes = fileSize
	t.mu.Unlock()

	// No onLines callback — onReady (fired by startLoop) tells the frontend
	// to fetch the windowed view via loadInitialLines / ReadRange RPCs.
	return nil
}

// initialReadTail reads only the tail of a large file for fast initial display,
// then starts background indexing to build the full line-offset index.
func (t *Tailer) initialReadTail(f *os.File, info os.FileInfo, fileSize int64) error {
	t.mu.RLock()
	seekBack := t.tailSeekBack
	t.mu.RUnlock()

	seekPos := fileSize - seekBack
	if seekPos < 0 {
		seekPos = 0
	}

	if _, err := f.Seek(seekPos, io.SeekStart); err != nil {
		return err
	}

	reader := bufio.NewReader(f)

	// If we didn't seek to the beginning, skip the partial first line
	if seekPos > 0 {
		partial, err := reader.ReadBytes('\n')
		if err != nil && err != io.EOF {
			return err
		}
		seekPos += int64(len(partial))
	}

	// Read lines from seekPos to end of file
	var tailLines []Line
	var tailOffsets []int64
	var num int64
	bytePos := seekPos

	for {
		tailOffsets = append(tailOffsets, bytePos)
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
			tailLines = append(tailLines, Line{Number: num, Text: text})
			bytePos += int64(len(lineBytes))
			if len(tailLines) > t.bufferSize*2 {
				tailLines = tailLines[len(tailLines)-t.bufferSize:]
			}
		}
		if err != nil {
			break
		}
	}

	if len(tailLines) > t.bufferSize {
		tailLines = tailLines[len(tailLines)-t.bufferSize:]
	}

	// Commit state — line numbers are temporary (will be corrected by background indexer)
	indexStopCh := make(chan struct{})
	t.mu.Lock()
	t.fileSize = fileSize
	t.fileIdent = info
	t.lineOffsets = tailOffsets
	t.lines = tailLines
	t.lineNum = num
	t.offset = fileSize
	t.lastDataAt = time.Now()
	t.indexingComplete = false
	t.indexedBytes = 0
	t.indexStopCh = indexStopCh
	t.mu.Unlock()

	// Start background indexing to build the full line-offset index
	go t.backgroundIndex(indexStopCh, fileSize)

	return nil
}

// backgroundIndex reads the file from the beginning in chunks, building the
// complete line-offset index. Once done, it corrects the line numbers assigned
// during the tail-first read. Cancellable via indexStopCh.
func (t *Tailer) backgroundIndex(stopCh chan struct{}, targetSize int64) {
	f, err := os.Open(t.filePath)
	if err != nil {
		// Indexing failed — mark complete so we don't block ReadRange forever.
		// The partial index from the tail read is still usable.
		t.mu.Lock()
		t.indexingComplete = true
		t.indexedBytes = t.fileSize
		finalLineNum := t.lineNum
		t.mu.Unlock()
		// Notify with the line count we have (from tail read) so the UI clears the spinner.
		if t.onIndexed != nil {
			t.onIndexed(finalLineNum)
		}
		return
	}
	defer f.Close()

	const chunkSize = 256 * 1024 // 256KB chunks
	reader := bufio.NewReaderSize(f, chunkSize)
	var offsets []int64
	var lineCount int64
	var bytePos int64

	for bytePos < targetSize {
		// Check for cancellation between chunks
		select {
		case <-stopCh:
			return
		default:
		}

		offsets = append(offsets, bytePos)
		lineBytes, err := reader.ReadBytes('\n')
		if len(lineBytes) > 0 {
			lineCount++
			bytePos += int64(len(lineBytes))
		}

		// Periodically update progress (every ~1000 lines)
		if lineCount%1000 == 0 {
			t.mu.Lock()
			t.indexedBytes = bytePos
			t.mu.Unlock()
		}

		if err != nil {
			break
		}
	}

	// Compute the line-number offset: the tail-first read assigned line numbers
	// starting from 1, but the real first line in the tail portion starts at
	// (lineCount - tailLineCount + 1) where tailLineCount is the count from the tail read.
	t.mu.Lock()
	tailLineCount := t.lineNum
	lineNumOffset := lineCount - tailLineCount

	// Correct the line numbers on buffered lines
	for i := range t.lines {
		t.lines[i].Number += lineNumOffset
	}

	t.lineOffsets = offsets
	t.lineNum = lineCount
	t.indexedBytes = targetSize
	t.indexingComplete = true
	t.mu.Unlock()

	if t.onIndexed != nil {
		t.onIndexed(lineCount)
	}
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

				t.mu.RLock()
				timeout := t.readTimeout
				t.mu.RUnlock()

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
				case <-time.After(timeout):
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
// For large files, uses tail-first approach to avoid blocking on slow mounts.
// Does NOT fire onLines — the caller fires onReady which triggers the frontend
// to fetch lines via loadInitialLines with proper windowed pagination.
func (t *Tailer) handleFullReread(f *os.File, currentSize int64, info os.FileInfo) {
	// Cancel any in-progress background indexing from a previous tail-first read
	t.mu.Lock()
	if t.indexStopCh != nil {
		select {
		case <-t.indexStopCh:
		default:
			close(t.indexStopCh)
		}
	}
	t.mu.Unlock()

	t.mu.RLock()
	threshold := t.tailFirstThreshold
	seekBack := t.tailSeekBack
	t.mu.RUnlock()

	// Large files: tail-first for fast recovery
	if threshold > 0 && currentSize > threshold {
		t.handleFullRereadTail(f, currentSize, info, seekBack)
		return
	}

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
		t.indexingComplete = true
		t.indexedBytes = currentSize
		t.mu.Unlock()
		// No onLines callback — onReady will tell the frontend to re-fetch.
	}
}

// handleFullRereadTail performs a tail-first re-read for large files during error recovery.
func (t *Tailer) handleFullRereadTail(f *os.File, currentSize int64, info os.FileInfo, seekBack int64) {
	seekPos := currentSize - seekBack
	if seekPos < 0 {
		seekPos = 0
	}

	if _, err := f.Seek(seekPos, io.SeekStart); err != nil {
		return
	}

	reader := bufio.NewReader(f)

	// Skip partial first line if not at start
	if seekPos > 0 {
		partial, err := reader.ReadBytes('\n')
		if err != nil && err != io.EOF {
			return
		}
		seekPos += int64(len(partial))
	}

	var tailLines []Line
	var tailOffsets []int64
	var num int64
	bytePos := seekPos

	for {
		tailOffsets = append(tailOffsets, bytePos)
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
			tailLines = append(tailLines, Line{Number: num, Text: text})
			bytePos += int64(len(lineBytes))
			if len(tailLines) > t.bufferSize*2 {
				tailLines = tailLines[len(tailLines)-t.bufferSize:]
			}
		}
		if err != nil {
			break
		}
	}

	if len(tailLines) > t.bufferSize {
		tailLines = tailLines[len(tailLines)-t.bufferSize:]
	}

	indexStopCh := make(chan struct{})
	t.mu.Lock()
	t.lines = tailLines
	t.lineNum = num
	t.offset = currentSize
	t.fileSize = currentSize
	t.fileIdent = info
	t.lineOffsets = tailOffsets
	t.lastDataAt = time.Now()
	t.indexingComplete = false
	t.indexedBytes = 0
	t.indexStopCh = indexStopCh
	t.mu.Unlock()

	if t.onTruncated != nil {
		t.onTruncated()
	}

	// Background index the full file
	go t.backgroundIndex(indexStopCh, currentSize)
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
