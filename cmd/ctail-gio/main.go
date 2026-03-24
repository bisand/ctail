package main

import (
	"flag"
	"fmt"
	"image"
	"os"

	"gioui.org/app"
	"gioui.org/io/key"
	"gioui.org/io/system"
	"gioui.org/layout"
	"gioui.org/op"
	"gioui.org/unit"
	"gioui.org/widget/material"
	"gioui.org/x/explorer"

	"ctail/internal/ui"
)

var (
	version     = "0.0.0-dev"
	buildNumber = "dev"
)

func main() {
	flag.Parse()

	appState, err := ui.NewApp()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to initialize: %v\n", err)
		os.Exit(1)
	}

	initialFiles := flag.Args()

	go func() {
		w := new(app.Window)
		w.Option(
			app.Title("ctail"),
			app.Size(unit.Dp(1200), unit.Dp(800)),
			app.MinSize(unit.Dp(400), unit.Dp(300)),
		)

		appState.Invalidate = w.Invalidate

		for _, f := range initialFiles {
			appState.OpenFile(f)
		}

		if err := run(w, appState); err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		os.Exit(0)
	}()
	app.Main()
}

func run(w *app.Window, appState *ui.App) error {
	th := material.NewTheme()

	tabBar := &ui.TabBar{}
	toolbar := &ui.Toolbar{}
	logView := ui.NewLogView()
	settingsPanel := &ui.SettingsPanel{}

	fileExplorer := explorer.NewExplorer(w)

	maxWindow := appState.Settings.ScrollBuffer
	if maxWindow < 200 {
		maxWindow = 500
	}

	var ops op.Ops

	for {
		e := w.Event()
		fileExplorer.ListenEvents(e)

		switch e := e.(type) {
		case app.DestroyEvent:
			appState.Shutdown()
			return e.Err

		case app.FrameEvent:
			gtx := app.NewContext(&ops, e)

			handleKeys(gtx, w, appState, fileExplorer, logView, settingsPanel)

			layoutAll(gtx, th, appState, tabBar, toolbar, logView, settingsPanel, fileExplorer, w, maxWindow)

			// After layout, check scroll thresholds for preloading
			checkScrollThresholds(appState, logView, maxWindow)

			e.Frame(gtx.Ops)
		}
	}
}

func handleKeys(gtx layout.Context, w *app.Window, appState *ui.App, fileExplorer *explorer.Explorer, logView *ui.LogView, settingsPanel *ui.SettingsPanel) {
	for {
		ev, ok := gtx.Event(
			key.Filter{Name: "O", Required: key.ModCtrl},
			key.Filter{Name: "W", Required: key.ModCtrl},
			key.Filter{Name: "Q", Required: key.ModCtrl},
			key.Filter{Name: key.NameTab, Required: key.ModCtrl},
			key.Filter{Name: key.NameEnd, Required: key.ModCtrl},
			key.Filter{Name: key.NameHome, Required: key.ModCtrl},
			key.Filter{Name: ",", Required: key.ModCtrl},
			key.Filter{Name: key.NameEscape},
		)
		if !ok {
			break
		}
		ke, ok := ev.(key.Event)
		if !ok || ke.State != key.Press {
			continue
		}

		switch {
		case ke.Name == "O" && ke.Modifiers.Contain(key.ModCtrl):
			go openFileDialog(appState, fileExplorer)

		case ke.Name == "W" && ke.Modifiers.Contain(key.ModCtrl):
			appState.Lock()
			active := appState.Active
			appState.Unlock()
			if active >= 0 {
				appState.CloseTab(active)
			}

		case ke.Name == "Q" && ke.Modifiers.Contain(key.ModCtrl):
			w.Perform(system.ActionClose)

		case ke.Name == key.NameTab && ke.Modifiers.Contain(key.ModCtrl):
			appState.Lock()
			n := len(appState.Tabs)
			active := appState.Active
			appState.Unlock()
			if n > 0 {
				appState.SetActive((active + 1) % n)
			}

		case ke.Name == key.NameEnd && ke.Modifiers.Contain(key.ModCtrl):
			appState.SetAutoScroll(true)
			logView.SetAutoScroll(true)
			logView.ScrollToEnd()

		case ke.Name == key.NameHome && ke.Modifiers.Contain(key.ModCtrl):
			appState.SetAutoScroll(false)
			logView.SetAutoScroll(false)
			logView.ScrollBy(-999999)

		case ke.Name == "," && ke.Modifiers.Contain(key.ModCtrl):
			settingsPanel.Visible = !settingsPanel.Visible
			if settingsPanel.Visible {
				settingsPanel.Init(appState.Config, appState.Settings)
			}

		case ke.Name == key.NameEscape:
			if settingsPanel.Visible {
				settingsPanel.Visible = false
			}
		}
	}
}

func openFileDialog(appState *ui.App, fileExplorer *explorer.Explorer) {
	rc, err := fileExplorer.ChooseFile()
	if err != nil {
		return
	}
	if f, ok := rc.(*os.File); ok {
		path := f.Name()
		rc.Close()
		appState.OpenFile(path)
	} else {
		rc.Close()
	}
}

func checkScrollThresholds(appState *ui.App, logView *ui.LogView, maxWindow int) {
	first, count, _ := logView.Position()
	if count <= 0 {
		return
	}

	appState.Lock()
	tab := appState.ActiveTab()
	if tab == nil || tab.Loading || len(tab.Lines) == 0 {
		appState.Unlock()
		return
	}
	bufSize := len(tab.Lines)
	canBack := tab.Lines[0].Number > 1
	canFwd := tab.Lines[len(tab.Lines)-1].Number < tab.TotalLines
	appState.Unlock()

	triggerTop := bufSize / 4
	triggerBottom := bufSize * 3 / 4
	lastVisible := first + count

	// Scrolling near top — load earlier lines
	if first < triggerTop && canBack {
		go func() {
			prepended := appState.FetchEarlierLines(maxWindow)
			if prepended > 0 {
				// Adjust scroll position to keep viewport stable
				logView.ScrollBy(prepended)
				if appState.Invalidate != nil {
					appState.Invalidate()
				}
			}
		}()
	}

	// Scrolling near bottom — load later lines
	if lastVisible > triggerBottom && canFwd {
		go func() {
			trimmed := appState.FetchLaterLines(maxWindow)
			if trimmed > 0 {
				// Adjust scroll position for trimmed lines
				logView.ScrollBy(-trimmed)
				if appState.Invalidate != nil {
					appState.Invalidate()
				}
			}
		}()
	}
}

func layoutAll(gtx layout.Context, th *material.Theme, appState *ui.App,
	tabBar *ui.TabBar, toolbar *ui.Toolbar, logView *ui.LogView,
	settingsPanel *ui.SettingsPanel, fileExplorer *explorer.Explorer,
	w *app.Window, maxWindow int) layout.Dimensions {

	appState.Lock()
	colors := appState.Colors
	tabs := appState.Tabs
	active := appState.Active
	activeTab := appState.ActiveTab()

	var lines []ui.LinesCopy
	autoScroll := false
	if activeTab != nil {
		lines = make([]ui.LinesCopy, len(activeTab.Lines))
		for i, l := range activeTab.Lines {
			lines[i] = ui.LinesCopy{Number: l.Number, Text: l.Text}
		}
		autoScroll = activeTab.AutoScroll
	}
	showLineNumbers := appState.Settings.ShowLineNumbers
	fontSize := appState.Settings.FontSize
	engine := appState.Rules
	settings := appState.Settings
	appState.Unlock()

	if fontSize < 8 {
		fontSize = 14
	}

	// Sync auto-scroll state to logview widget
	logView.SetAutoScroll(autoScroll)

	// Main layout using Stack so dropdown overlay can paint on top
	return layout.Stack{}.Layout(gtx,
		// Base layer: all content
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
				// Tab bar
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					clicked, closed, dims := tabBar.Layout(gtx, th, colors, tabs, active)
					if clicked >= 0 {
						appState.SetActive(clicked)
					}
					if closed >= 0 {
						appState.CloseTab(closed)
					}
					return dims
				}),
				// Menu bar + toolbar row
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					action, dims := toolbar.Layout(gtx, th, colors, autoScroll)
					handleToolbarAction(action, appState, toolbar, logView, settingsPanel, fileExplorer, w)
					return dims
				}),
				// Separator line
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					size := image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(1)))
					return ui.FillRect(gtx, colors.Border, size)
				}),
				// Log view + optional settings panel side by side
				layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
					if settingsPanel.Visible {
						return layout.Flex{Axis: layout.Horizontal}.Layout(gtx,
							// Log view takes remaining space
							layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
								return layout.Stack{}.Layout(gtx,
									layout.Expanded(func(gtx layout.Context) layout.Dimensions {
										return ui.FillRect(gtx, colors.BgPrimary, gtx.Constraints.Max)
									}),
									layout.Stacked(func(gtx layout.Context) layout.Dimensions {
										return logView.LayoutFromCopy(gtx, th, colors, lines, engine, showLineNumbers, fontSize)
									}),
								)
							}),
							// Settings panel on right
							layout.Rigid(func(gtx layout.Context) layout.Dimensions {
								action, dims := settingsPanel.Layout(gtx, th, colors, &settings)
								if action == ui.SettingsChanged {
									appState.Lock()
									appState.Settings = settings
									appState.Unlock()
									appState.ApplySettings()
								}
								return dims
							}),
						)
					}
					return layout.Stack{}.Layout(gtx,
						layout.Expanded(func(gtx layout.Context) layout.Dimensions {
							return ui.FillRect(gtx, colors.BgPrimary, gtx.Constraints.Max)
						}),
						layout.Stacked(func(gtx layout.Context) layout.Dimensions {
							return logView.LayoutFromCopy(gtx, th, colors, lines, engine, showLineNumbers, fontSize)
						}),
					)
				}),
				// Status bar
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					return layoutStatusBar(gtx, th, colors, activeTab, lines)
				}),
			)
		}),
		// Overlay layer: dropdown menu (paints on top of everything)
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return toolbar.LayoutDropdown(gtx, th, colors)
		}),
	)
}

func handleToolbarAction(action ui.ToolbarAction, appState *ui.App, toolbar *ui.Toolbar,
	logView *ui.LogView, settingsPanel *ui.SettingsPanel,
	fileExplorer *explorer.Explorer, w *app.Window) {

	switch action {
	case ui.ToolbarOpen:
		go openFileDialog(appState, fileExplorer)
	case ui.ToolbarCloseTab:
		appState.Lock()
		idx := appState.Active
		appState.Unlock()
		if idx >= 0 {
			appState.CloseTab(idx)
		}
	case ui.ToolbarQuit:
		w.Perform(system.ActionClose)
	case ui.ToolbarSettings:
		settingsPanel.Visible = !settingsPanel.Visible
		if settingsPanel.Visible {
			settingsPanel.Init(appState.Config, appState.Settings)
		}
	case ui.ToolbarToggleTheme:
		appState.ToggleTheme()
	case ui.ToolbarFollowChanged:
		newVal := toolbar.FollowChk.Value
		appState.SetAutoScroll(newVal)
		logView.SetAutoScroll(newVal)
		if newVal {
			logView.ScrollToEnd()
		}
	}
}

func layoutStatusBar(gtx layout.Context, th *material.Theme, colors ui.Colors, tab *ui.Tab, lines []ui.LinesCopy) layout.Dimensions {
	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(24)))
			return ui.FillRect(gtx, colors.BgSecondary, size)
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{Left: unit.Dp(8), Right: unit.Dp(8), Top: unit.Dp(4), Bottom: unit.Dp(4)}.Layout(gtx,
				func(gtx layout.Context) layout.Dimensions {
					var status string
					if tab != nil {
						status = fmt.Sprintf("%s — %d lines (total: %d)", tab.FilePath, len(lines), tab.TotalLines)
						if tab.AutoScroll {
							status += " [following]"
						}
					} else {
						status = "No file open — Ctrl+O to open"
					}
					lbl := material.Label(th, unit.Sp(12), status)
					lbl.Color = colors.TextMuted
					lbl.MaxLines = 1
					return lbl.Layout(gtx)
				},
			)
		}),
	)
}
