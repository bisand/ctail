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

	// Open files passed as CLI arguments
	initialFiles := flag.Args()

	go func() {
		w := new(app.Window)
		w.Option(
			app.Title("ctail"),
			app.Size(unit.Dp(1200), unit.Dp(800)),
			app.MinSize(unit.Dp(400), unit.Dp(300)),
		)

		appState.Invalidate = w.Invalidate

		// Open initial files
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
	logView := ui.NewLogView()

	fileExplorer := explorer.NewExplorer(w)

	var ops op.Ops

	for {
		e := w.Event()

		// Forward all events to explorer (needed for X11 view events)
		fileExplorer.ListenEvents(e)

		switch e := e.(type) {
		case app.DestroyEvent:
			appState.Shutdown()
			return e.Err

		case app.FrameEvent:
			gtx := app.NewContext(&ops, e)

			// Handle keyboard shortcuts
			handleKeys(gtx, w, appState, fileExplorer)

			// Layout the UI
			layoutUI(gtx, th, appState, tabBar, logView)

			e.Frame(gtx.Ops)
		}
	}
}

func handleKeys(gtx layout.Context, w *app.Window, appState *ui.App, fileExplorer *explorer.Explorer) {
	for {
		ev, ok := gtx.Event(
			key.Filter{Name: "O", Required: key.ModCtrl},
			key.Filter{Name: "W", Required: key.ModCtrl},
			key.Filter{Name: "Q", Required: key.ModCtrl},
			key.Filter{Name: key.NameTab, Required: key.ModCtrl},
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
		}
	}
}

func openFileDialog(appState *ui.App, fileExplorer *explorer.Explorer) {
	rc, err := fileExplorer.ChooseFile()
	if err != nil {
		return
	}

	// Get the file path from the *os.File
	if f, ok := rc.(*os.File); ok {
		path := f.Name()
		rc.Close()
		appState.OpenFile(path)
	} else {
		rc.Close()
	}
}

func layoutUI(gtx layout.Context, th *material.Theme, appState *ui.App, tabBar *ui.TabBar, logView *ui.LogView) layout.Dimensions {
	appState.Lock()
	colors := appState.Colors
	tabs := appState.Tabs
	active := appState.Active
	activeTab := appState.ActiveTab()

	var lines []ui.LinesCopy
	if activeTab != nil {
		lines = make([]ui.LinesCopy, len(activeTab.Lines))
		for i, l := range activeTab.Lines {
			lines[i] = ui.LinesCopy{Number: l.Number, Text: l.Text}
		}
	}
	showLineNumbers := appState.Settings.ShowLineNumbers
	fontSize := appState.Settings.FontSize
	engine := appState.Rules
	appState.Unlock()

	if fontSize < 8 {
		fontSize = 14
	}

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
		// Separator line
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(1)))
			return ui.FillRect(gtx, colors.Border, size)
		}),
		// Log view
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
		// Status bar
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			return layoutStatusBar(gtx, th, colors, activeTab, lines)
		}),
	)
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
						status = fmt.Sprintf("%s — %d lines", tab.FilePath, len(lines))
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
