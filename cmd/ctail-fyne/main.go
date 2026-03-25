package main

import (
	"flag"
	"fmt"
	"image/color"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	"ctail/internal/config"
	"ctail/internal/rules"
	"ctail/internal/tailer"
)

var (
	version     = "0.0.0-dev"
	buildNumber = "dev"
)

// fixedWidthContainer wraps a canvas object with a fixed minimum width.
type fixedWidthContainer struct {
	widget.BaseWidget
	width   float32
	content fyne.CanvasObject
}

func newFixedWidth(width float32, content fyne.CanvasObject) *fixedWidthContainer {
	fw := &fixedWidthContainer{width: width, content: content}
	fw.ExtendBaseWidget(fw)
	return fw
}

func (fw *fixedWidthContainer) CreateRenderer() fyne.WidgetRenderer {
	return &fixedWidthRenderer{fw: fw}
}

type fixedWidthRenderer struct {
	fw *fixedWidthContainer
}

func (r *fixedWidthRenderer) MinSize() fyne.Size {
	min := r.fw.content.MinSize()
	if min.Width < r.fw.width {
		min.Width = r.fw.width
	}
	return min
}

func (r *fixedWidthRenderer) Layout(size fyne.Size) {
	r.fw.content.Resize(size)
	r.fw.content.Move(fyne.NewPos(0, 0))
}

func (r *fixedWidthRenderer) Refresh()                     { r.fw.content.Refresh() }
func (r *fixedWidthRenderer) Objects() []fyne.CanvasObject { return []fyne.CanvasObject{r.fw.content} }
func (r *fixedWidthRenderer) Destroy()                     {}

// ctailApp holds all application state.
type ctailApp struct {
	mu sync.Mutex

	fyneApp    fyne.App
	mainWindow fyne.Window

	cfg      *config.Manager
	settings config.AppSettings
	engine   *rules.Engine

	tabs       []*logTab
	activeTab  int
	tabBar     *container.DocTabs
	statusBar  *widget.Label
	statusSize *widget.Label
	followChk  *widget.Check

	settingsPanel       *fyne.Container
	settingsPanelWidget fyne.CanvasObject
	settingsVisible     bool
	contentWrapper      *fyne.Container // Stack wrapping the content area
}

// logTab represents one open file tab.
type logTab struct {
	id       string
	filePath string
	name     string
	tailer   *tailer.Tailer
	lines    []tailer.Line
	total    int64
	fileSize int64

	autoScroll bool
	loading    bool
	fetching   bool // prevents concurrent preloads

	// UI widgets
	list    *widget.List
	tabItem *container.TabItem
}

func main() {
	flag.Parse()

	cfg, err := config.NewManager()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to initialize config: %v\n", err)
		os.Exit(1)
	}

	settings := cfg.GetSettings()
	engine := loadRulesEngine(cfg, settings)

	fyneApp := app.NewWithID("com.ctail.app")
	fyneApp.Settings().SetTheme(&ctailTheme{
		cfg:      cfg,
		settings: settings,
	})

	w := fyneApp.NewWindow("ctail")
	w.Resize(fyne.NewSize(1200, 800))

	ca := &ctailApp{
		fyneApp:    fyneApp,
		mainWindow: w,
		cfg:        cfg,
		settings:   settings,
		engine:     engine,
		activeTab:  -1,
	}

	// Status bar widgets
	ca.statusBar = widget.NewLabel("No file open — Ctrl+O to open")
	ca.statusBar.TextStyle = fyne.TextStyle{Monospace: true}

	ca.statusSize = widget.NewLabel("")
	ca.statusSize.TextStyle = fyne.TextStyle{Monospace: true}

	ca.followChk = widget.NewCheck("Follow", func(on bool) {
		ca.mu.Lock()
		tab := ca.getActiveTab()
		if tab == nil {
			ca.mu.Unlock()
			return
		}
		tab.autoScroll = on
		ca.mu.Unlock()
		if on {
			ca.jumpToEnd(tab)
		}
	})
	ca.followChk.Checked = true

	statusLeft := container.NewHBox(ca.statusBar, ca.statusSize)
	statusRight := container.NewHBox(ca.followChk)
	statusBarContainer := container.NewBorder(nil, nil, statusLeft, statusRight)

	// Tab bar
	ca.tabBar = container.NewDocTabs()
	ca.tabBar.OnSelected = func(item *container.TabItem) {
		ca.mu.Lock()
		for i, t := range ca.tabs {
			if t.tabItem == item {
				ca.activeTab = i
				ca.followChk.Checked = t.autoScroll
				ca.followChk.Refresh()
				break
			}
		}
		ca.mu.Unlock()
		ca.updateStatusBar()
	}
	ca.tabBar.SetTabLocation(container.TabLocationTop)
	ca.tabBar.CloseIntercept = func(item *container.TabItem) {
		ca.closeTabByItem(item)
	}

	// Toolbar
	titleLabel := canvas.NewText("ctail", hexToColor("#89b4fa"))
	titleLabel.TextStyle = fyne.TextStyle{Bold: true}
	titleLabel.TextSize = 12

	openBtn := widget.NewButton("Open", func() {
		ca.showOpenDialog()
	})
	openBtn.Importance = widget.LowImportance

	settingsBtn := widget.NewButton("Settings", nil)
	settingsBtn.Importance = widget.LowImportance
	settingsBtn.OnTapped = func() {
		ca.toggleSettingsPanel()
	}

	toolbarLeft := container.NewHBox(titleLabel)
	toolbarRight := container.NewHBox(openBtn, settingsBtn)
	toolbar := container.NewBorder(nil, nil, toolbarLeft, toolbarRight)

	// Build settings panel
	ca.settingsPanel = ca.buildSettingsPanel()
	ca.settingsPanelWidget = newFixedWidth(320, ca.settingsPanel)
	ca.settingsVisible = false

	// Content wrapper: swapped between full-width and split layout
	ca.contentWrapper = container.NewStack(ca.tabBar)

	topBar := container.NewVBox(toolbar, widget.NewSeparator())
	bottomBar := container.NewVBox(widget.NewSeparator(), statusBarContainer)

	// Main layout
	content := container.NewBorder(
		topBar,
		bottomBar,
		nil, nil,
		ca.contentWrapper,
	)

	w.SetContent(content)
	ca.setupMenus()

	// Open files from command line
	for _, f := range flag.Args() {
		ca.openFile(f)
	}

	w.ShowAndRun()
}

func (ca *ctailApp) toggleSettingsPanel() {
	ca.mu.Lock()
	ca.settingsVisible = !ca.settingsVisible
	visible := ca.settingsVisible
	ca.mu.Unlock()

	if visible {
		// Split: tabBar center, settings panel right
		ca.contentWrapper.Objects = []fyne.CanvasObject{
			container.NewBorder(nil, nil, nil, ca.settingsPanelWidget, ca.tabBar),
		}
	} else {
		// Full width: tabBar only
		ca.contentWrapper.Objects = []fyne.CanvasObject{ca.tabBar}
	}
	ca.contentWrapper.Refresh()
}

func (ca *ctailApp) buildSettingsPanel() *fyne.Container {
	settingsTab := ca.buildSettingsTab()
	rulesTab := ca.buildRulesTab()

	panelTabs := container.NewAppTabs(
		container.NewTabItem("Settings", settingsTab),
		container.NewTabItem("Rules", rulesTab),
	)

	return container.NewStack(panelTabs)
}

func (ca *ctailApp) buildSettingsTab() fyne.CanvasObject {
	ca.mu.Lock()
	s := ca.settings
	ca.mu.Unlock()

	// Poll Interval
	pollLabel := widget.NewLabel("Poll Interval (ms)")
	pollLabel.TextStyle = fyne.TextStyle{Bold: true}
	pollEntry := widget.NewEntry()
	pollEntry.SetText(strconv.Itoa(s.PollIntervalMs))
	pollEntry.OnChanged = func(val string) {
		if ms, err := strconv.Atoi(val); err == nil && ms >= 50 {
			ca.mu.Lock()
			ca.settings.PollIntervalMs = ms
			ca.settings.PollInterval = time.Duration(ms) * time.Millisecond
			newSettings := ca.settings
			ca.mu.Unlock()
			_ = ca.cfg.SaveSettings(newSettings)
		}
	}

	// Scroll Buffer
	scrollBufLabel := widget.NewLabel("Scroll Buffer (lines)")
	scrollBufLabel.TextStyle = fyne.TextStyle{Bold: true}
	scrollBufEntry := widget.NewEntry()
	scrollBufEntry.SetText(strconv.Itoa(s.ScrollBuffer))
	scrollBufEntry.OnChanged = func(val string) {
		if sb, err := strconv.Atoi(val); err == nil && sb >= 100 {
			ca.mu.Lock()
			ca.settings.ScrollBuffer = sb
			newSettings := ca.settings
			ca.mu.Unlock()
			_ = ca.cfg.SaveSettings(newSettings)
		}
	}

	// Scroll Speed slider
	scrollSpeedLabel := widget.NewLabel("Scroll Speed")
	scrollSpeedLabel.TextStyle = fyne.TextStyle{Bold: true}
	scrollSpeedSlider := widget.NewSlider(1, 10)
	scrollSpeedSlider.Step = 1
	scrollSpeedSlider.Value = float64(s.ScrollSpeed)
	if scrollSpeedSlider.Value < 1 {
		scrollSpeedSlider.Value = 1
	}
	scrollSpeedSlider.OnChanged = func(val float64) {
		ca.mu.Lock()
		ca.settings.ScrollSpeed = int(val)
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
	}

	// Smooth Scroll
	smoothScrollChk := widget.NewCheck("Smooth Scroll (deceleration at edges)", nil)
	smoothScrollChk.Checked = s.SmoothScroll
	smoothScrollChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.SmoothScroll = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
	}

	// Font Size
	fontSizeLabel := widget.NewLabel("Font Size")
	fontSizeLabel.TextStyle = fyne.TextStyle{Bold: true}
	fontSizeEntry := widget.NewEntry()
	fontSizeEntry.SetText(strconv.Itoa(s.FontSize))
	fontSizeEntry.OnChanged = func(val string) {
		if fs, err := strconv.Atoi(val); err == nil && fs >= 8 && fs <= 32 {
			ca.mu.Lock()
			ca.settings.FontSize = fs
			newSettings := ca.settings
			ca.mu.Unlock()
			ca.fyneApp.Settings().SetTheme(&ctailTheme{cfg: ca.cfg, settings: newSettings})
			_ = ca.cfg.SaveSettings(newSettings)
			ca.refreshAllTabs()
		}
	}

	// Show Line Numbers
	lineNumChk := widget.NewCheck("Show Line Numbers", nil)
	lineNumChk.Checked = s.ShowLineNumbers
	lineNumChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.ShowLineNumbers = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
		ca.refreshAllTabs()
	}

	// Word Wrap
	wordWrapChk := widget.NewCheck("Word Wrap", nil)
	wordWrapChk.Checked = s.WordWrap
	wordWrapChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.WordWrap = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
		ca.refreshAllTabs()
	}

	// Restore Tabs
	restoreTabsChk := widget.NewCheck("Restore Tabs on Startup", nil)
	restoreTabsChk.Checked = s.RestoreTabs
	restoreTabsChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.RestoreTabs = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
	}

	// Theme dropdown
	themeLabel := widget.NewLabel("Theme")
	themeLabel.TextStyle = fyne.TextStyle{Bold: true}
	themes := ca.cfg.ListThemes()
	themeNames := make([]string, len(themes))
	for i, t := range themes {
		themeNames[i] = t.Name
	}
	themeSelect := widget.NewSelect(themeNames, func(selected string) {
		ca.mu.Lock()
		ca.settings.Theme = selected
		newSettings := ca.settings
		ca.mu.Unlock()
		ca.fyneApp.Settings().SetTheme(&ctailTheme{cfg: ca.cfg, settings: newSettings})
		_ = ca.cfg.SaveSettings(newSettings)
		ca.refreshAllTabs()
	})
	themeSelect.SetSelected(s.Theme)

	// Profile dropdown
	profileLabel := widget.NewLabel("Profile")
	profileLabel.TextStyle = fyne.TextStyle{Bold: true}
	profiles := ca.cfg.ListProfiles()
	profileSelect := widget.NewSelect(profiles, func(selected string) {
		ca.mu.Lock()
		ca.settings.ActiveProfile = selected
		newSettings := ca.settings
		ca.engine = loadRulesEngine(ca.cfg, newSettings)
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
		ca.refreshAllTabs()
	})
	profileSelect.SetSelected(s.ActiveProfile)

	content := container.NewVBox(
		pollLabel, pollEntry,
		scrollBufLabel, scrollBufEntry,
		scrollSpeedLabel, scrollSpeedSlider,
		smoothScrollChk,
		fontSizeLabel, fontSizeEntry,
		lineNumChk,
		wordWrapChk,
		restoreTabsChk,
		themeLabel, themeSelect,
		profileLabel, profileSelect,
	)

	return container.NewVScroll(content)
}

func (ca *ctailApp) buildRulesTab() fyne.CanvasObject {
	ca.mu.Lock()
	s := ca.settings
	ca.mu.Unlock()

	profiles := ca.cfg.ListProfiles()
	profileSelect := widget.NewSelect(profiles, nil)
	profileSelect.SetSelected(s.ActiveProfile)

	rulesList := container.NewVBox()

	refreshRules := func(profileName string) {
		rulesList.Objects = nil
		if p, ok := ca.cfg.GetProfile(profileName); ok {
			for _, r := range p.Rules {
				rule := r
				enabledChk := widget.NewCheck("", nil)
				enabledChk.Checked = rule.Enabled

				typeBadge := widget.NewLabel("[" + rule.MatchType + "]")
				typeBadge.TextStyle = fyne.TextStyle{Bold: true}

				nameLabel := widget.NewLabel(rule.Name)
				nameLabel.TextStyle = fyne.TextStyle{Bold: true}

				patternLabel := widget.NewLabel(rule.Pattern)
				patternLabel.TextStyle = fyne.TextStyle{Monospace: true}
				patternLabel.Wrapping = fyne.TextTruncate

				row := container.NewHBox(enabledChk, typeBadge, nameLabel)
				item := container.NewVBox(row, patternLabel, widget.NewSeparator())
				rulesList.Add(item)
			}
		}
		rulesList.Refresh()
	}

	refreshRules(s.ActiveProfile)

	profileSelect.OnChanged = func(selected string) {
		ca.mu.Lock()
		ca.settings.ActiveProfile = selected
		newSettings := ca.settings
		ca.engine = loadRulesEngine(ca.cfg, newSettings)
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
		refreshRules(selected)
		ca.refreshAllTabs()
	}

	addBtn := widget.NewButton("Add Rule", func() {
		// TODO: implement add rule dialog
	})

	top := container.NewVBox(
		widget.NewLabel("Profile"),
		profileSelect,
		widget.NewSeparator(),
	)

	bottom := container.NewVBox(widget.NewSeparator(), addBtn)

	rulesScroll := container.NewVScroll(rulesList)
	return container.NewBorder(top, bottom, nil, nil, rulesScroll)
}

func (ca *ctailApp) refreshAllTabs() {
	ca.mu.Lock()
	tabs := make([]*logTab, len(ca.tabs))
	copy(tabs, ca.tabs)
	ca.mu.Unlock()
	for _, t := range tabs {
		t.list.Refresh()
	}
}

const fetchBatch = 200

// checkScrollPreload detects when the list is rendering items near the
// edges of the buffer and triggers a preload of earlier/later lines.
// It also auto-disables Follow when the user scrolls away from the bottom.
func (ca *ctailApp) checkScrollPreload(tab *logTab, visibleID int) {
	ca.mu.Lock()
	nLines := len(tab.lines)
	if nLines == 0 {
		ca.mu.Unlock()
		return
	}

	// Auto-disable Follow when user scrolls away from the bottom
	nearBottom := visibleID >= nLines-5
	if tab.autoScroll && !nearBottom && nLines > 10 {
		tab.autoScroll = false
		ca.mu.Unlock()
		fyne.Do(func() {
			ca.followChk.Checked = false
			ca.followChk.Refresh()
		})
		ca.mu.Lock()
	}

	if tab.fetching {
		ca.mu.Unlock()
		return
	}

	triggerTop := nLines / 4
	triggerBottom := nLines * 3 / 4
	firstLineNum := tab.lines[0].Number
	lastLineNum := tab.lines[nLines-1].Number
	totalLines := tab.total
	ca.mu.Unlock()

	if visibleID < triggerTop && firstLineNum > 1 {
		// Scrolling near top — fetch earlier lines
		ca.fetchEarlierLines(tab)
	} else if visibleID > triggerBottom && lastLineNum < totalLines {
		// Scrolling near bottom — fetch later lines (regardless of autoScroll)
		ca.fetchLaterLines(tab, lastLineNum)
	}
}

func (ca *ctailApp) fetchEarlierLines(tab *logTab) {
	ca.mu.Lock()
	if tab.fetching || len(tab.lines) == 0 {
		ca.mu.Unlock()
		return
	}
	tab.fetching = true
	firstLineNum := tab.lines[0].Number
	bufSize := ca.settings.BufferSize
	if bufSize < 1000 {
		bufSize = 10000
	}
	ca.mu.Unlock()

	if firstLineNum <= 1 {
		ca.mu.Lock()
		tab.fetching = false
		ca.mu.Unlock()
		return
	}

	go func() {
		fetchStart := firstLineNum - int64(fetchBatch)
		if fetchStart < 1 {
			fetchStart = 1
		}
		count := int(firstLineNum - fetchStart)
		if count <= 0 {
			ca.mu.Lock()
			tab.fetching = false
			ca.mu.Unlock()
			return
		}

		olderLines := tab.tailer.ReadRange(fetchStart, count)
		if len(olderLines) == 0 {
			ca.mu.Lock()
			tab.fetching = false
			ca.mu.Unlock()
			return
		}

		ca.mu.Lock()
		// Prepend older lines, trim to buffer size
		combined := make([]tailer.Line, 0, len(olderLines)+len(tab.lines))
		combined = append(combined, olderLines...)
		combined = append(combined, tab.lines...)
		if len(combined) > bufSize {
			combined = combined[:bufSize]
		}
		tab.lines = combined
		tab.fetching = false
		ca.mu.Unlock()

		fyne.Do(func() {
			tab.list.Refresh()
			ca.updateStatusBar()
		})
	}()
}

func (ca *ctailApp) fetchLaterLines(tab *logTab, lastLineNum int64) {
	ca.mu.Lock()
	if tab.fetching {
		ca.mu.Unlock()
		return
	}
	tab.fetching = true
	bufSize := ca.settings.BufferSize
	if bufSize < 1000 {
		bufSize = 10000
	}
	ca.mu.Unlock()

	go func() {
		fetchStart := lastLineNum + 1
		newerLines := tab.tailer.ReadRange(fetchStart, fetchBatch)
		if len(newerLines) == 0 {
			ca.mu.Lock()
			tab.fetching = false
			ca.mu.Unlock()
			return
		}

		ca.mu.Lock()
		// Append newer lines, trim from start to buffer size
		combined := make([]tailer.Line, 0, len(tab.lines)+len(newerLines))
		combined = append(combined, tab.lines...)
		combined = append(combined, newerLines...)
		if len(combined) > bufSize {
			combined = combined[len(combined)-bufSize:]
		}
		tab.lines = combined
		tab.fetching = false
		ca.mu.Unlock()

		fyne.Do(func() {
			tab.list.Refresh()
			ca.updateStatusBar()
		})
	}()
}

// jumpToEnd loads the latest lines from the file into the buffer and scrolls to the bottom.
func (ca *ctailApp) jumpToEnd(tab *logTab) {
	go func() {
		ca.mu.Lock()
		totalLines := tab.total
		bufSize := ca.settings.BufferSize
		if bufSize < 1000 {
			bufSize = 10000
		}
		ca.mu.Unlock()

		if totalLines <= 0 {
			// Fallback: use tailer's current buffer
			ca.mu.Lock()
			tab.lines = tab.tailer.GetLines()
			ca.mu.Unlock()
		} else {
			// Read the last bufSize lines from file
			startLine := totalLines - int64(bufSize) + 1
			if startLine < 1 {
				startLine = 1
			}
			count := int(totalLines - startLine + 1)
			latestLines := tab.tailer.ReadRange(startLine, count)
			if len(latestLines) > 0 {
				ca.mu.Lock()
				tab.lines = latestLines
				if len(tab.lines) > bufSize {
					tab.lines = tab.lines[len(tab.lines)-bufSize:]
				}
				ca.mu.Unlock()
			}
		}

		fyne.Do(func() {
			tab.list.Refresh()
			tab.list.ScrollToBottom()
			ca.updateStatusBar()
		})
	}()
}

func (ca *ctailApp) setupMenus() {
	fileMenu := fyne.NewMenu("File",
		fyne.NewMenuItem("Open File...", func() {
			ca.showOpenDialog()
		}),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Close Tab", func() {
			ca.mu.Lock()
			idx := ca.activeTab
			ca.mu.Unlock()
			if idx >= 0 {
				ca.closeTab(idx)
			}
		}),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Quit", func() {
			ca.fyneApp.Quit()
		}),
	)

	editMenu := fyne.NewMenu("Edit",
		fyne.NewMenuItem("Find...", func() {
			// TODO: implement find
		}),
	)

	viewMenu := fyne.NewMenu("View",
		fyne.NewMenuItem("Toggle Settings Panel", func() {
			ca.toggleSettingsPanel()
		}),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Toggle Theme", func() {
			ca.toggleTheme()
		}),
	)

	toolsMenu := fyne.NewMenu("Tools")

	helpMenu := fyne.NewMenu("Help",
		fyne.NewMenuItem("About ctail", func() {
			ca.showAboutDialog()
		}),
	)

	ca.mainWindow.SetMainMenu(fyne.NewMainMenu(fileMenu, editMenu, viewMenu, toolsMenu, helpMenu))

	// Keyboard shortcuts
	ca.mainWindow.Canvas().AddShortcut(&fyne.ShortcutCopy{}, func(_ fyne.Shortcut) {})

	openShortcut := &customShortcut{KeyName: fyne.KeyO, Modifier: fyne.KeyModifierControl}
	ca.mainWindow.Canvas().AddShortcut(openShortcut, func(_ fyne.Shortcut) {
		ca.showOpenDialog()
	})

	closeShortcut := &customShortcut{KeyName: fyne.KeyW, Modifier: fyne.KeyModifierControl}
	ca.mainWindow.Canvas().AddShortcut(closeShortcut, func(_ fyne.Shortcut) {
		ca.mu.Lock()
		idx := ca.activeTab
		ca.mu.Unlock()
		if idx >= 0 {
			ca.closeTab(idx)
		}
	})

	settingsShortcut := &customShortcut{KeyName: fyne.KeyComma, Modifier: fyne.KeyModifierControl}
	ca.mainWindow.Canvas().AddShortcut(settingsShortcut, func(_ fyne.Shortcut) {
		ca.toggleSettingsPanel()
	})
}

type customShortcut struct {
	KeyName  fyne.KeyName
	Modifier fyne.KeyModifier
}

func (cs *customShortcut) ShortcutName() string {
	return fmt.Sprintf("Custom:%s+%s", cs.Modifier, cs.KeyName)
}

func (ca *ctailApp) showOpenDialog() {
	fd := dialog.NewFileOpen(func(reader fyne.URIReadCloser, err error) {
		if err != nil || reader == nil {
			return
		}
		path := reader.URI().Path()
		reader.Close()
		ca.openFile(path)
	}, ca.mainWindow)
	// No filter — show all files
	fd.Resize(fyne.NewSize(800, 600))
	fd.Show()
}

func (ca *ctailApp) openFile(filePath string) {
	ca.mu.Lock()

	// Activate existing tab if file already open
	for i, tab := range ca.tabs {
		if tab.filePath == filePath {
			ca.activeTab = i
			ca.tabBar.Select(tab.tabItem)
			ca.mu.Unlock()
			return
		}
	}

	id := fmt.Sprintf("tab-%d", len(ca.tabs))
	bufSize := ca.settings.BufferSize
	if bufSize < 1000 {
		bufSize = 10000
	}
	poll := ca.settings.PollInterval
	if poll < 100*time.Millisecond {
		poll = 200 * time.Millisecond
	}

	t := tailer.New(filePath, poll, bufSize)
	tab := &logTab{
		id:         id,
		filePath:   filePath,
		name:       filepath.Base(filePath),
		tailer:     t,
		autoScroll: true,
		loading:    true,
	}

	// Create the list widget for this tab
	tab.list = widget.NewList(
		func() int {
			ca.mu.Lock()
			n := len(tab.lines)
			ca.mu.Unlock()
			return n
		},
		func() fyne.CanvasObject {
			return ca.createLineTemplate()
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			ca.updateLineItem(tab, id, obj)
		},
	)

	tab.tabItem = container.NewTabItemWithIcon(tab.name, nil, tab.list)

	ca.tabs = append(ca.tabs, tab)
	ca.activeTab = len(ca.tabs) - 1
	ca.mu.Unlock()

	ca.tabBar.Append(tab.tabItem)
	ca.tabBar.Select(tab.tabItem)

	updateFileSize := func() {
		if info, err := os.Stat(filePath); err == nil {
			ca.mu.Lock()
			tab.fileSize = info.Size()
			ca.mu.Unlock()
		}
	}

	// Wire callbacks — all UI updates must go through fyne.Do()
	t.OnLines(func(newLines []tailer.Line) {
		ca.mu.Lock()
		tab.total = t.GetTotalLines()
		shouldScroll := tab.autoScroll

		if shouldScroll {
			// Follow mode: replace buffer with latest lines, trim from start
			tab.lines = t.GetLines()
			if len(tab.lines) > bufSize {
				tab.lines = tab.lines[len(tab.lines)-bufSize:]
			}
		} else {
			// Not following: only update total, keep current buffer position.
			// The user can scroll down to see new lines via preloading.
		}
		ca.mu.Unlock()
		updateFileSize()
		fyne.Do(func() {
			tab.list.Refresh()
			if shouldScroll {
				tab.list.ScrollToBottom()
			}
			ca.updateStatusBar()
		})
	})
	t.OnTruncated(func() {
		ca.mu.Lock()
		tab.lines = t.GetLines()
		tab.total = t.GetTotalLines()
		ca.mu.Unlock()
		updateFileSize()
		fyne.Do(func() {
			tab.list.Refresh()
			ca.updateStatusBar()
		})
	})
	t.OnReady(func() {
		ca.mu.Lock()
		tab.lines = t.GetLines()
		tab.total = t.GetTotalLines()
		tab.loading = false
		shouldScroll := tab.autoScroll
		ca.mu.Unlock()
		updateFileSize()
		fyne.Do(func() {
			tab.list.Refresh()
			if shouldScroll {
				tab.list.ScrollToBottom()
			}
			ca.updateStatusBar()
		})
	})

	go func() {
		if err := t.Start(); err != nil {
			fmt.Fprintf(os.Stderr, "tailer error for %s: %v\n", filePath, err)
		}
	}()
}

func (ca *ctailApp) createLineTemplate() fyne.CanvasObject {
	fontSize := ca.getTextSize()

	lineNum := canvas.NewText("      ", color.Gray{Y: 128})
	lineNum.TextStyle = fyne.TextStyle{Monospace: true}
	lineNum.TextSize = fontSize

	lineText := canvas.NewText("", color.White)
	lineText.TextStyle = fyne.TextStyle{Monospace: true}
	lineText.TextSize = fontSize

	return container.NewHBox(lineNum, lineText)
}

// getTextSize returns the font size scaled for Fyne's dp system.
// Fyne dp units are roughly 1.5x CSS px, so we scale down to match
// the density of the original Svelte UI.
func (ca *ctailApp) getTextSize() float32 {
	fs := ca.settings.FontSize
	if fs < 8 || fs > 32 {
		fs = 12
	}
	return float32(fs) * 0.65
}

func (ca *ctailApp) updateLineItem(tab *logTab, id widget.ListItemID, obj fyne.CanvasObject) {
	ca.mu.Lock()
	if id < 0 || id >= len(tab.lines) {
		ca.mu.Unlock()
		return
	}
	line := tab.lines[id]
	showNums := ca.settings.ShowLineNumbers
	engine := ca.engine
	ca.mu.Unlock()

	scaledSize := ca.getTextSize()

	box := obj.(*fyne.Container)
	numLabel := box.Objects[0].(*canvas.Text)
	textLabel := box.Objects[1].(*canvas.Text)

	if showNums {
		numLabel.Text = fmt.Sprintf("%6d ", line.Number)
		numLabel.Show()
	} else {
		numLabel.Text = ""
		numLabel.Hide()
	}
	numLabel.TextSize = scaledSize
	numLabel.Color = color.Gray{Y: 128}

	textLabel.Text = line.Text
	textLabel.TextSize = scaledSize

	// Apply highlighting
	result := engine.Apply(line.Text)
	if result.FullLine {
		if result.Foreground != "" {
			textLabel.Color = hexToColor(result.Foreground)
		} else {
			textLabel.Color = color.White
		}
		textLabel.TextStyle.Bold = result.Bold
		textLabel.TextStyle.Italic = result.Italic
	} else {
		textLabel.Color = color.White
		textLabel.TextStyle.Bold = false
		textLabel.TextStyle.Italic = false
	}

	numLabel.Refresh()
	textLabel.Refresh()

	// Trigger scroll preloading when rendering near buffer edges
	go ca.checkScrollPreload(tab, id)
}

func (ca *ctailApp) closeTab(idx int) {
	ca.mu.Lock()
	if idx < 0 || idx >= len(ca.tabs) {
		ca.mu.Unlock()
		return
	}
	tab := ca.tabs[idx]
	tab.tailer.Stop()
	ca.tabs = append(ca.tabs[:idx], ca.tabs[idx+1:]...)
	if ca.activeTab >= len(ca.tabs) {
		ca.activeTab = len(ca.tabs) - 1
	}
	ca.mu.Unlock()

	ca.tabBar.Remove(tab.tabItem)
	ca.updateStatusBar()
}

func (ca *ctailApp) closeTabByItem(item *container.TabItem) {
	ca.mu.Lock()
	for i, t := range ca.tabs {
		if t.tabItem == item {
			ca.mu.Unlock()
			ca.closeTab(i)
			return
		}
	}
	ca.mu.Unlock()
}

func (ca *ctailApp) getActiveTab() *logTab {
	if ca.activeTab < 0 || ca.activeTab >= len(ca.tabs) {
		return nil
	}
	return ca.tabs[ca.activeTab]
}

func (ca *ctailApp) updateStatusBar() {
	ca.mu.Lock()
	tab := ca.getActiveTab()
	var pathText, sizeText string
	if tab != nil {
		pathText = fmt.Sprintf("%s  %d lines (total: %d)", tab.filePath, len(tab.lines), tab.total)
		sizeText = formatFileSize(tab.fileSize)
		if tab.loading {
			sizeText += "  Loading..."
		}
	} else {
		pathText = "No file open — Ctrl+O to open"
	}
	ca.mu.Unlock()
	ca.statusBar.SetText(pathText)
	ca.statusSize.SetText(sizeText)
}

func (ca *ctailApp) toggleTheme() {
	ca.mu.Lock()
	if ca.settings.ThemeMode == "light" {
		ca.settings.ThemeMode = "dark"
	} else {
		ca.settings.ThemeMode = "light"
	}
	s := ca.settings
	ca.mu.Unlock()

	ca.fyneApp.Settings().SetTheme(&ctailTheme{cfg: ca.cfg, settings: s})
	_ = ca.cfg.SaveSettings(s)
}

func (ca *ctailApp) showAboutDialog() {
	msg := fmt.Sprintf("ctail %s (build %s)\n\nA cross-platform log file viewer\nwith syntax highlighting.", version, buildNumber)
	dialog.ShowInformation("About ctail", msg, ca.mainWindow)
}

// formatFileSize formats bytes into a human-readable string.
func formatFileSize(size int64) string {
	const (
		KB = 1024
		MB = 1024 * KB
		GB = 1024 * MB
	)
	switch {
	case size >= GB:
		return fmt.Sprintf("%.1f GB", float64(size)/float64(GB))
	case size >= MB:
		return fmt.Sprintf("%.1f MB", float64(size)/float64(MB))
	case size >= KB:
		return fmt.Sprintf("%.1f KB", float64(size)/float64(KB))
	default:
		return fmt.Sprintf("%d B", size)
	}
}

// ctailTheme applies colors from the config theme system.
type ctailTheme struct {
	cfg      *config.Manager
	settings config.AppSettings
}

func (t *ctailTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	tc := t.getColors()
	switch name {
	case theme.ColorNameBackground:
		return hexToColor(tc.BgPrimary)
	case theme.ColorNameForeground:
		return hexToColor(tc.TextPrimary)
	case theme.ColorNameButton:
		return hexToColor(tc.Accent)
	case theme.ColorNameDisabledButton:
		return hexToColor(tc.BgHover)
	case theme.ColorNameInputBackground:
		return hexToColor(tc.BgSurface)
	case theme.ColorNameInputBorder:
		return hexToColor(tc.Border)
	case theme.ColorNameMenuBackground:
		return hexToColor(tc.BgSurface)
	case theme.ColorNameOverlayBackground:
		return hexToColor(tc.BgSurface)
	case theme.ColorNameSeparator:
		return hexToColor(tc.Border)
	case theme.ColorNameScrollBar:
		return hexToColor(tc.ScrollThumb)
	case theme.ColorNameHeaderBackground:
		return hexToColor(tc.BgSecondary)
	case theme.ColorNameHover:
		return hexToColor(tc.BgHover)
	case theme.ColorNamePrimary:
		return hexToColor(tc.Accent)
	default:
		return theme.DefaultTheme().Color(name, variant)
	}
}

func (t *ctailTheme) Font(style fyne.TextStyle) fyne.Resource {
	return theme.DefaultTheme().Font(style)
}

func (t *ctailTheme) Icon(name fyne.ThemeIconName) fyne.Resource {
	return theme.DefaultTheme().Icon(name)
}

func (t *ctailTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameText:
		fs := t.settings.FontSize
		if fs < 8 || fs > 32 {
			fs = 12
		}
		return float32(fs) * 0.65
	case theme.SizeNamePadding:
		return 2
	case theme.SizeNameInnerPadding:
		return 2
	case theme.SizeNameLineSpacing:
		return 0
	}
	return theme.DefaultTheme().Size(name)
}

func (t *ctailTheme) getColors() config.ThemeColors {
	if th, ok := t.cfg.GetTheme(t.settings.Theme); ok {
		if t.settings.ThemeMode == "light" {
			return th.Light
		}
		return th.Dark
	}
	// Catppuccin Mocha fallback
	return config.ThemeColors{
		BgPrimary:     "#1e1e2e",
		BgSecondary:   "#181825",
		BgSurface:     "#313244",
		BgHover:       "#45475a",
		TextPrimary:   "#cdd6f4",
		TextSecondary: "#bac2de",
		TextMuted:     "#6c7086",
		Accent:        "#89b4fa",
		AccentHover:   "#74c7ec",
		Border:        "#45475a",
		Danger:        "#f38ba8",
		Success:       "#a6e3a1",
		Warning:       "#f9e2af",
		TabActive:     "#1e1e2e",
		TabInactive:   "#181825",
		BadgeColor:    "#f38ba8",
		ScrollTrack:   "#313244",
		ScrollThumb:   "#585b70",
	}
}

func loadRulesEngine(cfg *config.Manager, s config.AppSettings) *rules.Engine {
	engine := rules.NewEngine()
	if p, ok := cfg.GetProfile(s.ActiveProfile); ok {
		inputs := make([]rules.RuleInput, len(p.Rules))
		for i, r := range p.Rules {
			inputs[i] = rules.RuleInput{
				ID: r.ID, Name: r.Name, Pattern: r.Pattern,
				MatchType: r.MatchType, Foreground: r.Foreground,
				Background: r.Background, Bold: r.Bold, Italic: r.Italic,
				Enabled: r.Enabled, Priority: r.Priority,
			}
		}
		_ = engine.SetRules(inputs)
	}
	return engine
}

func hexToColor(hex string) color.Color {
	hex = strings.TrimPrefix(hex, "#")
	if len(hex) == 3 {
		hex = string([]byte{hex[0], hex[0], hex[1], hex[1], hex[2], hex[2]})
	}
	if len(hex) != 6 {
		return color.White
	}
	r, _ := strconv.ParseUint(hex[0:2], 16, 8)
	g, _ := strconv.ParseUint(hex[2:4], 16, 8)
	b, _ := strconv.ParseUint(hex[4:6], 16, 8)
	return color.NRGBA{R: uint8(r), G: uint8(g), B: uint8(b), A: 255}
}
