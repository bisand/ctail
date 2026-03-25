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

	"github.com/ncruces/zenity"

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

// fixedSquareContainer constrains a canvas object to a fixed square size.
type fixedSquareContainer struct {
	widget.BaseWidget
	size    float32
	content fyne.CanvasObject
}

func newFixedSquare(size float32, content fyne.CanvasObject) *fixedSquareContainer {
	fs := &fixedSquareContainer{size: size, content: content}
	fs.ExtendBaseWidget(fs)
	return fs
}

func (fs *fixedSquareContainer) CreateRenderer() fyne.WidgetRenderer {
	return &fixedSquareRenderer{fs: fs}
}

type fixedSquareRenderer struct {
	fs *fixedSquareContainer
}

func (r *fixedSquareRenderer) MinSize() fyne.Size {
	return fyne.NewSize(r.fs.size, r.fs.size)
}
func (r *fixedSquareRenderer) Layout(size fyne.Size) {
	r.fs.content.Resize(fyne.NewSize(r.fs.size, r.fs.size))
	r.fs.content.Move(fyne.NewPos(0, 0))
}
func (r *fixedSquareRenderer) Refresh()                     { r.fs.content.Refresh() }
func (r *fixedSquareRenderer) Objects() []fyne.CanvasObject { return []fyne.CanvasObject{r.fs.content} }
func (r *fixedSquareRenderer) Destroy()                     {}

// ctailApp holds all application state.
type ctailApp struct {
	mu sync.Mutex

	fyneApp    fyne.App
	mainWindow fyne.Window

	cfg      *config.Manager
	settings config.AppSettings
	engine   *rules.Engine
	theme    *ctailTheme

	tabs       []*logTab
	activeTab  int
	tabBar     *container.DocTabs
	statusBar  *widget.Label
	statusSize *widget.Label
	followChk  *widget.Check

	settingsPanel       *fyne.Container
	settingsPanelWidget fyne.CanvasObject
	settingsVisible     bool
	contentWrapper      *fyne.Container
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

	autoScroll    bool
	loading       bool
	fetching      bool // prevents concurrent preloads
	lastVisibleID int  // last rendered item index (for scroll adjustment)

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
	ct := &ctailTheme{
		cfg:           cfg,
		settings:      settings,
		dynamicColors: make(map[fyne.ThemeColorName]color.Color),
	}
	fyneApp.Settings().SetTheme(ct)

	w := fyneApp.NewWindow("ctail")
	w.Resize(fyne.NewSize(1200, 800))

	ca := &ctailApp{
		fyneApp:    fyneApp,
		mainWindow: w,
		cfg:        cfg,
		settings:   settings,
		engine:     engine,
		theme:      ct,
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

	// Toolbar — text+icon buttons matching Svelte style
	openBtn := widget.NewButtonWithIcon("Open", theme.FolderOpenIcon(), func() {
		ca.showOpenDialog()
	})
	openBtn.Importance = widget.HighImportance

	settingsBtn := widget.NewButtonWithIcon("Settings", theme.SettingsIcon(), func() {
		ca.toggleSettingsPanel()
	})
	settingsBtn.Importance = widget.MediumImportance

	brandLabel := widget.NewLabel("ctail")

	toolbarRight := container.NewHBox(openBtn, settingsBtn)
	toolbarRow := container.NewBorder(nil, nil, brandLabel, toolbarRight)

	// Build settings panel
	ca.settingsPanel = ca.buildSettingsPanel()
	ca.settingsPanelWidget = newFixedWidth(320, ca.settingsPanel)
	ca.settingsVisible = false

	// Content wrapper: swapped between full-width and split layout
	ca.contentWrapper = container.NewStack(ca.tabBar)

	topBar := container.NewVBox(toolbarRow, widget.NewSeparator())
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

	// Restore tabs from previous session (if enabled and no CLI args)
	if settings.RestoreTabs && len(flag.Args()) == 0 {
		for _, ts := range settings.Tabs {
			if _, err := os.Stat(ts.FilePath); err == nil {
				ca.openFile(ts.FilePath)
			}
		}
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

	// Font Size
	fontSizeEntry := widget.NewEntry()
	fontSizeEntry.SetText(strconv.Itoa(s.FontSize))
	fontSizeEntry.OnChanged = func(val string) {
		if fs, err := strconv.Atoi(val); err == nil && fs >= 8 && fs <= 32 {
			ca.mu.Lock()
			ca.settings.FontSize = fs
			newSettings := ca.settings
			ca.mu.Unlock()
			ca.theme.updateSettings(newSettings)
			_ = ca.cfg.SaveSettings(newSettings)
			ca.refreshAllTabs()
		}
	}

	// Smooth Scroll
	smoothScrollChk := widget.NewCheck("", nil)
	smoothScrollChk.Checked = s.SmoothScroll
	smoothScrollChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.SmoothScroll = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
	}

	// Show Line Numbers
	lineNumChk := widget.NewCheck("", nil)
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
	wordWrapChk := widget.NewCheck("", nil)
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
	restoreTabsChk := widget.NewCheck("", nil)
	restoreTabsChk.Checked = s.RestoreTabs
	restoreTabsChk.OnChanged = func(on bool) {
		ca.mu.Lock()
		ca.settings.RestoreTabs = on
		newSettings := ca.settings
		ca.mu.Unlock()
		_ = ca.cfg.SaveSettings(newSettings)
	}

	// Theme dropdown
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
		ca.theme.updateSettings(newSettings)
		ca.fyneApp.Settings().SetTheme(ca.theme)
		_ = ca.cfg.SaveSettings(newSettings)
		ca.refreshAllTabs()
	})
	themeSelect.SetSelected(s.Theme)

	// Profile dropdown
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

	// Build forms
	displayForm := widget.NewForm(
		&widget.FormItem{Text: "Font Size", Widget: fontSizeEntry, HintText: "8–32"},
		&widget.FormItem{Text: "Line Numbers", Widget: lineNumChk},
		&widget.FormItem{Text: "Word Wrap", Widget: wordWrapChk},
		&widget.FormItem{Text: "Theme", Widget: themeSelect},
		&widget.FormItem{Text: "Profile", Widget: profileSelect},
	)

	scrollForm := widget.NewForm(
		&widget.FormItem{Text: "Poll Interval", Widget: pollEntry, HintText: "ms (≥50)"},
		&widget.FormItem{Text: "Scroll Buffer", Widget: scrollBufEntry, HintText: "lines (≥100)"},
		&widget.FormItem{Text: "Scroll Speed", Widget: scrollSpeedSlider},
		&widget.FormItem{Text: "Smooth Scroll", Widget: smoothScrollChk},
	)

	generalForm := widget.NewForm(
		&widget.FormItem{Text: "Restore Tabs", Widget: restoreTabsChk},
	)

	// Section headers
	sectionLabel := func(text string) fyne.CanvasObject {
		l := widget.NewLabel(text)
		return l
	}

	content := container.NewVBox(
		sectionLabel("Display"),
		displayForm,
		widget.NewSeparator(),
		sectionLabel("Scrolling"),
		scrollForm,
		widget.NewSeparator(),
		sectionLabel("General"),
		generalForm,
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

	var refreshRules func(string)

	showRuleEditor := func(profileName string, rule *config.Rule, isNew bool) {
		nameEntry := widget.NewEntry()
		nameEntry.SetPlaceHolder("Rule name")
		patternEntry := widget.NewEntry()
		patternEntry.SetPlaceHolder("Regex pattern")
		matchTypeSelect := widget.NewSelect([]string{"line", "match"}, nil)
		fgEntry := widget.NewEntry()
		fgEntry.SetPlaceHolder("#ff6b6b")
		bgEntry := widget.NewEntry()
		bgEntry.SetPlaceHolder("#3d1f1f")
		boldChk := widget.NewCheck("", nil)
		italicChk := widget.NewCheck("", nil)
		enabledChk := widget.NewCheck("", nil)

		if rule != nil {
			nameEntry.SetText(rule.Name)
			patternEntry.SetText(rule.Pattern)
			matchTypeSelect.SetSelected(rule.MatchType)
			fgEntry.SetText(rule.Foreground)
			bgEntry.SetText(rule.Background)
			boldChk.Checked = rule.Bold
			italicChk.Checked = rule.Italic
			enabledChk.Checked = rule.Enabled
		} else {
			matchTypeSelect.SetSelected("line")
			enabledChk.Checked = true
		}

		form := widget.NewForm(
			&widget.FormItem{Text: "Name", Widget: nameEntry},
			&widget.FormItem{Text: "Pattern", Widget: patternEntry, HintText: "regex"},
			&widget.FormItem{Text: "Match Type", Widget: matchTypeSelect},
			&widget.FormItem{Text: "Foreground", Widget: fgEntry, HintText: "hex color"},
			&widget.FormItem{Text: "Background", Widget: bgEntry, HintText: "hex color"},
			&widget.FormItem{Text: "Bold", Widget: boldChk},
			&widget.FormItem{Text: "Italic", Widget: italicChk},
			&widget.FormItem{Text: "Enabled", Widget: enabledChk},
		)

		title := "Edit Rule"
		if isNew {
			title = "Add Rule"
		}

		d := dialog.NewCustomConfirm(title, "Save", "Cancel", form, func(ok bool) {
			if !ok {
				return
			}
			p, exists := ca.cfg.GetProfile(profileName)
			if !exists {
				return
			}

			newRule := config.Rule{
				Name:       nameEntry.Text,
				Pattern:    patternEntry.Text,
				MatchType:  matchTypeSelect.Selected,
				Foreground: fgEntry.Text,
				Background: bgEntry.Text,
				Bold:       boldChk.Checked,
				Italic:     italicChk.Checked,
				Enabled:    enabledChk.Checked,
			}

			if isNew {
				newRule.ID = fmt.Sprintf("rule-%d", time.Now().UnixMilli())
				newRule.Priority = len(p.Rules)
				p.Rules = append(p.Rules, newRule)
			} else if rule != nil {
				newRule.ID = rule.ID
				newRule.Priority = rule.Priority
				for i, r := range p.Rules {
					if r.ID == rule.ID {
						p.Rules[i] = newRule
						break
					}
				}
			}

			_ = ca.cfg.SaveProfile(p)
			ca.mu.Lock()
			ca.engine = loadRulesEngine(ca.cfg, ca.settings)
			ca.mu.Unlock()
			refreshRules(profileName)
			ca.refreshAllTabs()
		}, ca.mainWindow)
		d.Resize(fyne.NewSize(400, 450))
		d.Show()
	}

	refreshRules = func(profileName string) {
		rulesList.Objects = nil
		p, ok := ca.cfg.GetProfile(profileName)
		if !ok {
			rulesList.Refresh()
			return
		}
		for idx, r := range p.Rules {
			rule := r
			ruleIdx := idx

			// Colored indicator dot (using rule's foreground color)
			indicatorColor := color.NRGBA{R: 128, G: 128, B: 128, A: 255}
			if rule.Foreground != "" {
				indicatorColor = hexToNRGBA(rule.Foreground)
			}
			indicator := canvas.NewCircle(indicatorColor)
			indicator.StrokeWidth = 0
			indicatorWrap := container.NewStack(newFixedSquare(10, indicator))

			// Rule name in the rule's foreground color
			var nameColor color.Color = color.White
			if rule.Foreground != "" {
				nameColor = hexToColor(rule.Foreground)
			}
			nameText := canvas.NewText(rule.Name, nameColor)
			nameText.TextStyle = fyne.TextStyle{}
			nameText.TextSize = 12

			// Match type badge
			typeBadge := widget.NewLabel(rule.MatchType)

			// Edit button
			editBtn := widget.NewButtonWithIcon("Edit", theme.DocumentCreateIcon(), func() {
				showRuleEditor(profileName, &rule, false)
			})
			editBtn.Importance = widget.LowImportance

			// Delete button (×)
			deleteBtn := widget.NewButtonWithIcon("", theme.CancelIcon(), func() {
				pp, exists := ca.cfg.GetProfile(profileName)
				if !exists {
					return
				}
				for i, rr := range pp.Rules {
					if rr.ID == rule.ID {
						pp.Rules = append(pp.Rules[:i], pp.Rules[i+1:]...)
						break
					}
				}
				_ = ca.cfg.SaveProfile(pp)
				ca.mu.Lock()
				ca.engine = loadRulesEngine(ca.cfg, ca.settings)
				ca.mu.Unlock()
				refreshRules(profileName)
				ca.refreshAllTabs()
			})
			deleteBtn.Importance = widget.LowImportance

			// Enable/disable toggle
			enabledChk := widget.NewCheck("", func(on bool) {
				pp, exists := ca.cfg.GetProfile(profileName)
				if !exists || ruleIdx >= len(pp.Rules) {
					return
				}
				pp.Rules[ruleIdx].Enabled = on
				_ = ca.cfg.SaveProfile(pp)
				ca.mu.Lock()
				ca.engine = loadRulesEngine(ca.cfg, ca.settings)
				ca.mu.Unlock()
				ca.refreshAllTabs()
			})
			enabledChk.Checked = rule.Enabled

			// Pattern text below
			patternText := canvas.NewText(rule.Pattern, color.Gray{Y: 160})
			patternText.TextStyle = fyne.TextStyle{Monospace: true}
			patternText.TextSize = 11

			// Layout: top row = indicator + name + badge + buttons
			nameAndBadge := container.NewHBox(indicatorWrap, enabledChk, nameText, typeBadge)
			btns := container.NewHBox(editBtn, deleteBtn)
			topRow := container.NewBorder(nil, nil, nameAndBadge, btns)

			// Card-like container with padding
			ruleCard := container.NewVBox(topRow, container.NewPadded(patternText))

			// Subtle separator between rules
			rulesList.Add(container.NewVBox(ruleCard, widget.NewSeparator()))
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

	// Profile header with +/trash buttons (matching Svelte)
	addProfileBtn := widget.NewButtonWithIcon("", theme.ContentAddIcon(), func() {
		nameEntry := widget.NewEntry()
		nameEntry.SetPlaceHolder("New profile name")
		d := dialog.NewCustomConfirm("New Profile", "Create", "Cancel",
			widget.NewForm(&widget.FormItem{Text: "Name", Widget: nameEntry}),
			func(ok bool) {
				if !ok || nameEntry.Text == "" {
					return
				}
				newProfile := config.Profile{Name: nameEntry.Text, Rules: []config.Rule{}}
				_ = ca.cfg.SaveProfile(newProfile)
				profileSelect.Options = ca.cfg.ListProfiles()
				profileSelect.SetSelected(nameEntry.Text)
				profileSelect.Refresh()
			}, ca.mainWindow)
		d.Show()
	})
	addProfileBtn.Importance = widget.LowImportance

	deleteProfileBtn := widget.NewButtonWithIcon("", theme.DeleteIcon(), func() {
		ca.mu.Lock()
		pName := ca.settings.ActiveProfile
		ca.mu.Unlock()
		if pName == "" {
			return
		}
		dialog.ShowConfirm("Delete Profile", fmt.Sprintf("Delete profile '%s'?", pName), func(ok bool) {
			if !ok {
				return
			}
			_ = ca.cfg.DeleteProfile(pName)
			profiles := ca.cfg.ListProfiles()
			profileSelect.Options = profiles
			if len(profiles) > 0 {
				profileSelect.SetSelected(profiles[0])
			}
			profileSelect.Refresh()
		}, ca.mainWindow)
	})
	deleteProfileBtn.Importance = widget.LowImportance

	profileRow := container.NewBorder(nil, nil, nil,
		container.NewHBox(addProfileBtn, deleteProfileBtn),
		profileSelect,
	)

	helpText := widget.NewLabel("Rules are applied top to bottom. Rules lower in the list take precedence over earlier ones.")
	helpText.Wrapping = fyne.TextWrapWord
	helpText.TextStyle = fyne.TextStyle{Italic: true}

	profileHeader := container.NewVBox(profileRow, helpText, widget.NewSeparator())

	addRuleBtn := widget.NewButtonWithIcon("Add Rule", theme.ContentAddIcon(), func() {
		ca.mu.Lock()
		pName := ca.settings.ActiveProfile
		ca.mu.Unlock()
		showRuleEditor(pName, nil, true)
	})
	addRuleBtn.Importance = widget.HighImportance

	rulesScroll := container.NewVScroll(rulesList)
	return container.NewBorder(profileHeader, addRuleBtn, nil, nil, rulesScroll)
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
// edges of the buffer and slides the window in that direction.
// Also manages the Follow checkbox based on scroll position.
func (ca *ctailApp) checkScrollPreload(tab *logTab, visibleID int) {
	ca.mu.Lock()
	nLines := len(tab.lines)
	if nLines == 0 {
		ca.mu.Unlock()
		return
	}

	tab.lastVisibleID = visibleID

	// Auto-enable/disable Follow based on proximity to end of file
	nearBottom := visibleID >= nLines-5
	atFileEnd := tab.lines[nLines-1].Number >= tab.total

	if !tab.autoScroll && nearBottom && atFileEnd {
		tab.autoScroll = true
		ca.mu.Unlock()
		fyne.Do(func() {
			ca.followChk.Checked = true
			ca.followChk.Refresh()
		})
		return
	} else if tab.autoScroll && !nearBottom && nLines > 10 {
		tab.autoScroll = false
		ca.mu.Unlock()
		fyne.Do(func() {
			ca.followChk.Checked = false
			ca.followChk.Refresh()
		})
		// Re-lock to continue to threshold check
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
		ca.slideWindowUp(tab)
	} else if visibleID > triggerBottom && lastLineNum < totalLines {
		ca.slideWindowDown(tab)
	}
}

// slideWindowUp shifts the buffer window toward the beginning of the file.
// Prepends N earlier lines and removes the same N from the end, keeping
// the buffer length constant. Adjusts scroll position so the user sees
// no visual change — the window slides underneath.
func (ca *ctailApp) slideWindowUp(tab *logTab) {
	ca.mu.Lock()
	if tab.fetching || len(tab.lines) == 0 {
		ca.mu.Unlock()
		return
	}
	tab.fetching = true
	firstLineNum := tab.lines[0].Number
	currentLen := len(tab.lines)
	visibleID := tab.lastVisibleID
	ca.mu.Unlock()

	if firstLineNum <= 1 {
		ca.mu.Lock()
		tab.fetching = false
		ca.mu.Unlock()
		return
	}

	go func() {
		// Calculate how many lines to fetch
		fetchCount := fetchBatch
		fetchStart := firstLineNum - int64(fetchCount)
		if fetchStart < 1 {
			fetchStart = 1
			fetchCount = int(firstLineNum - 1)
		}
		if fetchCount <= 0 {
			ca.mu.Lock()
			tab.fetching = false
			ca.mu.Unlock()
			return
		}

		olderLines := tab.tailer.ReadRange(fetchStart, fetchCount)
		if len(olderLines) == 0 {
			ca.mu.Lock()
			tab.fetching = false
			ca.mu.Unlock()
			return
		}

		added := len(olderLines)

		ca.mu.Lock()
		// Prepend older lines, pop same count from the end
		combined := make([]tailer.Line, 0, added+currentLen)
		combined = append(combined, olderLines...)
		combined = append(combined, tab.lines...)
		// Remove exactly `added` lines from the end to keep length constant
		if len(combined) > currentLen {
			combined = combined[:currentLen]
		}
		tab.lines = combined
		// The user's content shifted right by `added` indices
		scrollTarget := visibleID + added
		if scrollTarget >= len(combined) {
			scrollTarget = len(combined) - 1
		}
		tab.fetching = false
		ca.mu.Unlock()

		fyne.Do(func() {
			tab.list.Refresh()
			tab.list.ScrollTo(scrollTarget)
		})
	}()
}

// slideWindowDown shifts the buffer window toward the end of the file.
// Appends N later lines and removes the same N from the start, keeping
// the buffer length constant. Adjusts scroll position so the user sees
// no visual change.
func (ca *ctailApp) slideWindowDown(tab *logTab) {
	ca.mu.Lock()
	if tab.fetching || len(tab.lines) == 0 {
		ca.mu.Unlock()
		return
	}
	tab.fetching = true
	lastLineNum := tab.lines[len(tab.lines)-1].Number
	currentLen := len(tab.lines)
	visibleID := tab.lastVisibleID
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

		added := len(newerLines)

		ca.mu.Lock()
		// Append newer lines, pop same count from the start
		combined := make([]tailer.Line, 0, currentLen+added)
		combined = append(combined, tab.lines...)
		combined = append(combined, newerLines...)
		// Remove exactly `added` lines from the start to keep length constant
		if len(combined) > currentLen {
			removed := len(combined) - currentLen
			combined = combined[removed:]
			// Shift scroll position back by removed count
			visibleID -= removed
			if visibleID < 0 {
				visibleID = 0
			}
		}
		tab.lines = combined
		tab.fetching = false
		ca.mu.Unlock()

		fyne.Do(func() {
			tab.list.Refresh()
			tab.list.ScrollTo(visibleID)
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
	go func() {
		path, err := zenity.SelectFile(
			zenity.Title("Open File"),
			zenity.Filename(os.Getenv("HOME")+"/"),
		)
		if err != nil || path == "" {
			return
		}
		fyne.Do(func() {
			ca.openFile(path)
		})
	}()
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
	ca.saveOpenTabs()

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
		nLines := len(tab.lines)
		shouldScroll := tab.autoScroll
		ca.mu.Unlock()
		updateFileSize()
		fyne.Do(func() {
			tab.list.Refresh()
			ca.updateStatusBar()
			if shouldScroll && nLines > 0 {
				// Delay scroll to allow the list to finish layout
				go func() {
					time.Sleep(50 * time.Millisecond)
					fyne.Do(func() {
						tab.list.ScrollTo(nLines - 1)
					})
				}()
			}
		})
	})

	go func() {
		if err := t.Start(); err != nil {
			fmt.Fprintf(os.Stderr, "tailer error for %s: %v\n", filePath, err)
		}
	}()
}

// Custom theme size/color names for log view text
const (
	logTextSizeName fyne.ThemeSizeName  = "logText"
	logNumColorName fyne.ThemeColorName = "logNum"
)

func (ca *ctailApp) createLineTemplate() fyne.CanvasObject {
	numRT := widget.NewRichText(&widget.TextSegment{
		Text: "      ",
		Style: widget.RichTextStyle{
			ColorName: logNumColorName,
			SizeName:  logTextSizeName,
			TextStyle: fyne.TextStyle{Monospace: true},
			Inline:    true,
		},
	})
	numRT.Wrapping = fyne.TextWrapOff
	numRT.Truncation = fyne.TextTruncateOff

	lineRT := widget.NewRichText(&widget.TextSegment{
		Text: "",
		Style: widget.RichTextStyle{
			SizeName:  logTextSizeName,
			TextStyle: fyne.TextStyle{Monospace: true},
			Inline:    true,
		},
	})
	lineRT.Wrapping = fyne.TextWrapOff
	lineRT.Truncation = fyne.TextTruncateOff

	return container.NewBorder(nil, nil, numRT, nil, lineRT)
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

	border := obj.(*fyne.Container)
	// Border layout: Objects = [top, bottom, left, right, center]
	// We used NewBorder(nil, nil, numRT, nil, lineRT)
	// Objects[0]=numRT (left), Objects[1]=lineRT (center)
	numRT := border.Objects[0].(*widget.RichText)
	lineRT := border.Objects[1].(*widget.RichText)

	if showNums {
		numRT.Segments = []widget.RichTextSegment{
			&widget.TextSegment{
				Text: fmt.Sprintf("%6d ", line.Number),
				Style: widget.RichTextStyle{
					ColorName: logNumColorName,
					SizeName:  logTextSizeName,
					TextStyle: fyne.TextStyle{Monospace: true},
					Inline:    true,
				},
			},
		}
		numRT.Show()
	} else {
		numRT.Segments = nil
		numRT.Hide()
	}

	// Apply highlighting
	result := engine.Apply(line.Text)

	if len(result.Matches) > 0 && !result.FullLine {
		lineRT.Segments = ca.buildRichSegments(line.Text, result)
	} else if result.FullLine {
		colorName := fyne.ThemeColorName(theme.ColorNameForeground)
		if result.Foreground != "" {
			colorName = ca.theme.registerColor(result.Foreground)
		}
		lineRT.Segments = []widget.RichTextSegment{
			&widget.TextSegment{
				Text: line.Text,
				Style: widget.RichTextStyle{
					ColorName: colorName,
					SizeName:  logTextSizeName,
					TextStyle: fyne.TextStyle{Monospace: true, Bold: result.Bold, Italic: result.Italic},
					Inline:    true,
				},
			},
		}
	} else {
		lineRT.Segments = []widget.RichTextSegment{
			&widget.TextSegment{
				Text: line.Text,
				Style: widget.RichTextStyle{
					ColorName: theme.ColorNameForeground,
					SizeName:  logTextSizeName,
					TextStyle: fyne.TextStyle{Monospace: true},
					Inline:    true,
				},
			},
		}
	}

	numRT.Refresh()
	lineRT.Refresh()

	// Trigger scroll preloading when rendering near buffer edges
	go ca.checkScrollPreload(tab, id)
}

// buildRichSegments creates RichText segments for per-match highlighting.
func (ca *ctailApp) buildRichSegments(text string, result rules.LineResult) []widget.RichTextSegment {
	type region struct {
		start, end   int
		fg           string
		bold, italic bool
	}

	sorted := make([]region, 0, len(result.Matches))
	for _, m := range result.Matches {
		sorted = append(sorted, region{m.Start, m.End, m.Foreground, m.Bold, m.Italic})
	}
	for i := 1; i < len(sorted); i++ {
		for j := i; j > 0 && sorted[j].start < sorted[j-1].start; j-- {
			sorted[j], sorted[j-1] = sorted[j-1], sorted[j]
		}
	}

	defaultStyle := widget.RichTextStyle{
		ColorName: theme.ColorNameForeground,
		SizeName:  logTextSizeName,
		TextStyle: fyne.TextStyle{Monospace: true},
		Inline:    true,
	}

	var segments []widget.RichTextSegment
	pos := 0
	for _, m := range sorted {
		if m.start > pos && m.start <= len(text) {
			segments = append(segments, &widget.TextSegment{
				Text:  text[pos:m.start],
				Style: defaultStyle,
			})
		}
		colorName := fyne.ThemeColorName(theme.ColorNameForeground)
		if m.fg != "" {
			colorName = ca.theme.registerColor(m.fg)
		}
		end := m.end
		if end > len(text) {
			end = len(text)
		}
		start := m.start
		if start > len(text) {
			start = len(text)
		}
		segments = append(segments, &widget.TextSegment{
			Text: text[start:end],
			Style: widget.RichTextStyle{
				ColorName: colorName,
				SizeName:  logTextSizeName,
				TextStyle: fyne.TextStyle{Monospace: true, Bold: m.bold, Italic: m.italic},
				Inline:    true,
			},
		})
		if m.end > pos {
			pos = m.end
		}
	}
	if pos < len(text) {
		segments = append(segments, &widget.TextSegment{
			Text:  text[pos:],
			Style: defaultStyle,
		})
	}
	if len(segments) == 0 {
		segments = append(segments, &widget.TextSegment{
			Text:  text,
			Style: defaultStyle,
		})
	}
	return segments
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
	ca.saveOpenTabs()
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

// saveOpenTabs persists the current open tab paths to settings.
func (ca *ctailApp) saveOpenTabs() {
	ca.mu.Lock()
	var tabStates []config.TabState
	for _, t := range ca.tabs {
		tabStates = append(tabStates, config.TabState{
			FilePath:   t.filePath,
			AutoScroll: t.autoScroll,
		})
	}
	ca.settings.Tabs = tabStates
	s := ca.settings
	ca.mu.Unlock()
	_ = ca.cfg.SaveSettings(s)
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

	ca.theme.updateSettings(s)
	ca.fyneApp.Settings().SetTheme(ca.theme)
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
	cfg           *config.Manager
	settings      config.AppSettings
	mu            sync.RWMutex
	dynamicColors map[fyne.ThemeColorName]color.Color
}

func (t *ctailTheme) updateSettings(s config.AppSettings) {
	t.mu.Lock()
	t.settings = s
	t.mu.Unlock()
}

// registerColor registers a hex color and returns its ThemeColorName.
func (t *ctailTheme) registerColor(hex string) fyne.ThemeColorName {
	if hex == "" {
		return theme.ColorNameForeground
	}
	name := fyne.ThemeColorName("dyn-" + strings.TrimPrefix(hex, "#"))
	t.mu.Lock()
	t.dynamicColors[name] = hexToColor(hex)
	t.mu.Unlock()
	return name
}

func (t *ctailTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	// Check dynamic colors first
	t.mu.RLock()
	if c, ok := t.dynamicColors[name]; ok {
		t.mu.RUnlock()
		return c
	}
	t.mu.RUnlock()

	// Log line number color
	if name == logNumColorName {
		return color.NRGBA{R: 128, G: 128, B: 128, A: 255}
	}

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
		return 12
	case theme.SizeNamePadding:
		return 3
	case theme.SizeNameInnerPadding:
		return 2
	case theme.SizeNameLineSpacing:
		return 0
	case theme.SizeNameSeparatorThickness:
		return 0
	case logTextSizeName:
		t.mu.RLock()
		fs := t.settings.FontSize
		t.mu.RUnlock()
		if fs < 8 || fs > 32 {
			fs = 12
		}
		return float32(fs) * 0.65
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

func hexToNRGBA(hex string) color.NRGBA {
	hex = strings.TrimPrefix(hex, "#")
	if len(hex) == 3 {
		hex = string([]byte{hex[0], hex[0], hex[1], hex[1], hex[2], hex[2]})
	}
	if len(hex) != 6 {
		return color.NRGBA{R: 255, G: 255, B: 255, A: 255}
	}
	r, _ := strconv.ParseUint(hex[0:2], 16, 8)
	g, _ := strconv.ParseUint(hex[2:4], 16, 8)
	b, _ := strconv.ParseUint(hex[4:6], 16, 8)
	return color.NRGBA{R: uint8(r), G: uint8(g), B: uint8(b), A: 255}
}
