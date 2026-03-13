package main

import (
	"ctail/internal/config"
	"embed"
	"flag"
	"os"
	"path/filepath"
	"runtime"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/menu"
	"github.com/wailsapp/wails/v2/pkg/menu/keys"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
	wailsRuntime "github.com/wailsapp/wails/v2/pkg/runtime"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	useX11 := flag.Bool("x11", false, "Force X11 backend (fixes multi-monitor maximize on Wayland)")
	useWayland := flag.Bool("wayland", false, "Force native Wayland backend")
	flag.Parse()

	if runtime.GOOS == "linux" {
		if *useX11 {
			os.Setenv("GDK_BACKEND", "x11")
		} else if *useWayland {
			os.Setenv("GDK_BACKEND", "wayland")
		} else {
			// Default to X11 for better multi-monitor support, but allow Wayland if available
			os.Setenv("GDK_BACKEND", "x11")
		}
	}

	app := NewApp()

	// Pre-load config to populate recent files menu
	cfg, _ := config.NewManager()
	app.preloadedConfig = cfg

	appMenu := menu.NewMenu()

	// File menu
	fileMenu := appMenu.AddSubmenu("File")
	fileMenu.AddText("Open File...", keys.CmdOrCtrl("o"), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:open-file")
	})

	// Recent Files submenu — pre-populated from saved config
	recentMenu := fileMenu.AddSubmenu("Open Recent")
	app.recentMenu = recentMenu
	if cfg != nil {
		recentFiles := cfg.GetSettings().RecentFiles
		if len(recentFiles) > 0 {
			for _, fp := range recentFiles {
				filePath := fp
				label := filepath.Base(filePath)
				recentMenu.AddText(label, nil, func(_ *menu.CallbackData) {
					wailsRuntime.EventsEmit(app.ctx, "menu:open-recent", filePath)
				})
			}
			recentMenu.AddSeparator()
			recentMenu.AddText("Clear Recent Files", nil, func(_ *menu.CallbackData) {
				app.ClearRecentFiles()
			})
		} else {
			recentMenu.AddText("(empty)", nil, nil)
		}
	} else {
		recentMenu.AddText("(empty)", nil, nil)
	}

	fileMenu.AddSeparator()
	fileMenu.AddText("Close Tab", keys.CmdOrCtrl("w"), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:close-tab")
	})
	fileMenu.AddSeparator()
	fileMenu.AddText("Quit", keys.CmdOrCtrl("q"), func(_ *menu.CallbackData) {
		wailsRuntime.Quit(app.ctx)
	})

	// Edit menu
	editMenu := appMenu.AddSubmenu("Edit")
	editMenu.AddText("Copy", keys.CmdOrCtrl("c"), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:copy")
	})
	editMenu.AddText("Select All", keys.CmdOrCtrl("a"), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:select-all")
	})
	editMenu.AddSeparator()
	editMenu.AddText("Find...", keys.CmdOrCtrl("f"), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:find")
	})

	// View menu
	viewMenu := appMenu.AddSubmenu("View")
	viewMenu.AddText("Settings", keys.CmdOrCtrl(","), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:toggle-settings")
	})
	viewMenu.AddSeparator()
	viewMenu.AddText("Toggle Theme", nil, func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:toggle-theme")
	})

	// Help menu
	helpMenu := appMenu.AddSubmenu("Help")
	helpMenu.AddText("About ctail", nil, func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:about")
	})

	err := wails.Run(&options.App{
		Title:     "ctail",
		Width:     1200,
		Height:    800,
		MinWidth:  800,
		MinHeight: 500,
		Menu:      appMenu,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		BackgroundColour: &options.RGBA{R: 30, G: 30, B: 46, A: 1},
		OnStartup:        app.startup,
		OnShutdown:       app.shutdown,
		Bind: []interface{}{
			app,
		},
	})

	if err != nil {
		println("Error:", err.Error())
	}
}
