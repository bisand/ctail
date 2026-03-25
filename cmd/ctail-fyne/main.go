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
	followChk  *widget.Check
}

// logTab represents one open file tab.
type logTab struct {
	id       string
	filePath string
	name     string
	tailer   *tailer.Tailer
	lines    []tailer.Line
	total    int64

	autoScroll bool
	loading    bool

	// UI widgets
	list      *widget.List
	tabItem   *container.TabItem
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

	ca.statusBar = widget.NewLabel("No file open — Ctrl+O to open")
	ca.statusBar.TextStyle = fyne.TextStyle{Monospace: true}

	ca.followChk = widget.NewCheck("Follow tail", func(on bool) {
		ca.mu.Lock()
		tab := ca.getActiveTab()
		ca.mu.Unlock()
		if tab != nil {
			ca.mu.Lock()
			tab.autoScroll = on
			ca.mu.Unlock()
			if on {
				tab.list.ScrollToBottom()
			}
		}
	})
	ca.followChk.Checked = true

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
	toolbar := container.NewHBox(ca.followChk)

	// Main layout
	content := container.NewBorder(
		container.NewVBox(toolbar, widget.NewSeparator()), // top
		ca.statusBar, // bottom
		nil, nil,     // left, right
		ca.tabBar,    // center
	)

	w.SetContent(content)
	ca.setupMenus()

	// Open files from command line
	for _, f := range flag.Args() {
		ca.openFile(f)
	}

	w.ShowAndRun()
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
		fyne.NewMenuItem("Settings", func() {
			ca.showSettingsDialog()
		}),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Toggle Theme", func() {
			ca.toggleTheme()
		}),
	)

	helpMenu := fyne.NewMenu("Help",
		fyne.NewMenuItem("About ctail", func() {
			ca.showAboutDialog()
		}),
	)

	ca.mainWindow.SetMainMenu(fyne.NewMainMenu(fileMenu, editMenu, viewMenu, helpMenu))

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
		ca.showSettingsDialog()
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

	// Wire callbacks
	t.OnLines(func(_ []tailer.Line) {
		ca.mu.Lock()
		tab.lines = t.GetLines()
		tab.total = t.GetTotalLines()
		shouldScroll := tab.autoScroll
		ca.mu.Unlock()
		tab.list.Refresh()
		if shouldScroll {
			tab.list.ScrollToBottom()
		}
		ca.updateStatusBar()
	})
	t.OnTruncated(func() {
		ca.mu.Lock()
		tab.lines = t.GetLines()
		tab.total = t.GetTotalLines()
		ca.mu.Unlock()
		tab.list.Refresh()
		ca.updateStatusBar()
	})
	t.OnReady(func() {
		ca.mu.Lock()
		tab.lines = t.GetLines()
		tab.total = t.GetTotalLines()
		shouldScroll := tab.autoScroll
		ca.mu.Unlock()
		tab.list.Refresh()
		if shouldScroll {
			tab.list.ScrollToBottom()
		}
		ca.updateStatusBar()
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
// Config fontSize is in points (8-32), Fyne default is 14dp.
func (ca *ctailApp) getTextSize() float32 {
	fs := ca.settings.FontSize
	if fs < 8 || fs > 32 {
		fs = 12
	}
	return float32(fs)
}

func (ca *ctailApp) updateLineItem(tab *logTab, id widget.ListItemID, obj fyne.CanvasObject) {
	ca.mu.Lock()
	if id < 0 || id >= len(tab.lines) {
		ca.mu.Unlock()
		return
	}
	line := tab.lines[id]
	showNums := ca.settings.ShowLineNumbers
	fontSize := ca.settings.FontSize
	engine := ca.engine
	ca.mu.Unlock()

	if fontSize < 8 || fontSize > 32 {
		fontSize = 12
	}

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
	numLabel.TextSize = float32(fontSize)
	numLabel.Color = color.Gray{Y: 128}

	textLabel.Text = line.Text
	textLabel.TextSize = float32(fontSize)

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
	var text string
	if tab != nil {
		text = fmt.Sprintf("%s — %d lines (total: %d)", tab.filePath, len(tab.lines), tab.total)
		if tab.autoScroll {
			text += " [following]"
		}
	} else {
		text = "No file open — Ctrl+O to open"
	}
	ca.mu.Unlock()
	ca.statusBar.SetText(text)
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

func (ca *ctailApp) showSettingsDialog() {
	ca.mu.Lock()
	s := ca.settings
	ca.mu.Unlock()

	// Theme selector
	themes := ca.cfg.ListThemes()
	themeNames := make([]string, len(themes))
	currentThemeIdx := 0
	for i, t := range themes {
		themeNames[i] = t.Name
		if t.Name == s.Theme {
			currentThemeIdx = i
		}
	}
	themeSelect := widget.NewSelect(themeNames, nil)
	if currentThemeIdx < len(themeNames) {
		themeSelect.SetSelectedIndex(currentThemeIdx)
	}

	// Theme mode
	modeSelect := widget.NewRadioGroup([]string{"dark", "light"}, nil)
	modeSelect.Selected = s.ThemeMode
	modeSelect.Horizontal = true

	// Profile selector
	profiles := ca.cfg.ListProfiles()
	profileSelect := widget.NewSelect(profiles, nil)
	profileSelect.SetSelected(s.ActiveProfile)

	// Font size
	fontEntry := widget.NewEntry()
	fontEntry.SetText(strconv.Itoa(s.FontSize))

	// Checkboxes
	lineNumChk := widget.NewCheck("Show line numbers", nil)
	lineNumChk.Checked = s.ShowLineNumbers

	wordWrapChk := widget.NewCheck("Word wrap", nil)
	wordWrapChk.Checked = s.WordWrap

	// Buffer size
	bufEntry := widget.NewEntry()
	bufEntry.SetText(strconv.Itoa(s.BufferSize))

	// Scroll buffer
	scrollBufEntry := widget.NewEntry()
	scrollBufEntry.SetText(strconv.Itoa(s.ScrollBuffer))

	form := widget.NewForm(
		widget.NewFormItem("Theme", themeSelect),
		widget.NewFormItem("Mode", modeSelect),
		widget.NewFormItem("Profile", profileSelect),
		widget.NewFormItem("Font Size", fontEntry),
		widget.NewFormItem("", lineNumChk),
		widget.NewFormItem("", wordWrapChk),
		widget.NewFormItem("Buffer Size", bufEntry),
		widget.NewFormItem("Scroll Buffer", scrollBufEntry),
	)

	d := dialog.NewCustomConfirm("Settings", "Save", "Cancel", form, func(save bool) {
		if !save {
			return
		}
		ca.mu.Lock()
		if themeSelect.Selected != "" {
			ca.settings.Theme = themeSelect.Selected
		}
		ca.settings.ThemeMode = modeSelect.Selected
		if profileSelect.Selected != "" {
			ca.settings.ActiveProfile = profileSelect.Selected
		}
		if fs, err := strconv.Atoi(fontEntry.Text); err == nil && fs >= 8 && fs <= 32 {
			ca.settings.FontSize = fs
		}
		ca.settings.ShowLineNumbers = lineNumChk.Checked
		ca.settings.WordWrap = wordWrapChk.Checked
		if bs, err := strconv.Atoi(bufEntry.Text); err == nil && bs >= 1000 {
			ca.settings.BufferSize = bs
		}
		if sb, err := strconv.Atoi(scrollBufEntry.Text); err == nil && sb >= 100 {
			ca.settings.ScrollBuffer = sb
		}
		newSettings := ca.settings
		ca.engine = loadRulesEngine(ca.cfg, newSettings)
		ca.mu.Unlock()

		ca.fyneApp.Settings().SetTheme(&ctailTheme{cfg: ca.cfg, settings: newSettings})
		_ = ca.cfg.SaveSettings(newSettings)

		// Refresh all tabs
		ca.mu.Lock()
		for _, t := range ca.tabs {
			t.list.Refresh()
		}
		ca.mu.Unlock()
	}, ca.mainWindow)

	d.Resize(fyne.NewSize(500, 450))
	d.Show()
}

func (ca *ctailApp) showAboutDialog() {
	msg := fmt.Sprintf("ctail %s (build %s)\n\nA cross-platform log file viewer\nwith syntax highlighting.", version, buildNumber)
	dialog.ShowInformation("About ctail", msg, ca.mainWindow)
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
		return float32(fs)
	case theme.SizeNamePadding:
		return 3
	case theme.SizeNameLineSpacing:
		return 1
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
		BgPrimary:    "#1e1e2e",
		BgSecondary:  "#181825",
		BgSurface:    "#313244",
		BgHover:      "#45475a",
		TextPrimary:  "#cdd6f4",
		TextSecondary: "#bac2de",
		TextMuted:    "#6c7086",
		Accent:       "#89b4fa",
		AccentHover:  "#74c7ec",
		Border:       "#45475a",
		Danger:       "#f38ba8",
		Success:      "#a6e3a1",
		Warning:      "#f9e2af",
		TabActive:    "#1e1e2e",
		TabInactive:  "#181825",
		BadgeColor:   "#f38ba8",
		ScrollTrack:  "#313244",
		ScrollThumb:  "#585b70",
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
