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

// Set via -ldflags at build time
var buildNumber = "dev"
var version = "0.0.0-dev"

func main() {
	useX11 := flag.Bool("x11", false, "Force X11 backend (fixes multi-monitor maximize on Wayland)")
	useWayland := flag.Bool("wayland", false, "Force native Wayland backend")
	flag.Parse()

	app := NewApp(version, buildNumber)

	// Pre-load config to populate recent files menu and window state
	cfg, _ := config.NewManager()
	app.preloadedConfig = cfg

	if runtime.GOOS == "linux" {
		setDisplayBackend(*useX11, *useWayland, cfg)
	}

	// Read saved window geometry for initial size
	savedWindow := cfg.GetSettings().Window
	initialWidth := 1200
	initialHeight := 800
	if savedWindow.Width > 0 && savedWindow.Height > 0 {
		initialWidth = savedWindow.Width
		initialHeight = savedWindow.Height
	}

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
		Width:     initialWidth,
		Height:    initialHeight,
		MinWidth:  800,
		MinHeight: 500,
		Menu:      appMenu,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		BackgroundColour: &options.RGBA{R: 30, G: 30, B: 46, A: 1},
		OnStartup:        app.startup,
		OnDomReady:       app.restoreWindowState,
		OnShutdown:       app.shutdown,
		Bind: []interface{}{
			app,
		},
	})

	if err != nil {
		println("Error:", err.Error())
	}
}

// setDisplayBackend configures GDK_BACKEND based on CLI flags, config setting,
// and available display servers. CLI flags take priority, then config, then auto-detect.
func setDisplayBackend(forceX11, forceWayland bool, cfg *config.Manager) {
	if forceX11 {
		os.Setenv("GDK_BACKEND", "x11")
		return
	}
	if forceWayland {
		os.Setenv("GDK_BACKEND", "wayland")
		return
	}

	// Read preference from config
	backend := "auto"
	if cfg != nil {
		backend = cfg.GetSettings().DisplayBackend
	}

	switch backend {
	case "x11":
		os.Setenv("GDK_BACKEND", "x11")
	case "wayland":
		os.Setenv("GDK_BACKEND", "wayland")
	default:
		// Auto: prefer X11 if available, fall back to wayland
		if isX11Available() {
			os.Setenv("GDK_BACKEND", "x11")
		}
		// If X11 not available, don't set GDK_BACKEND — GTK picks wayland automatically
	}
}

// isX11Available checks whether an X11 display server is reachable
func isX11Available() bool {
	// Check DISPLAY env var (set when X11 or XWayland is available)
	if display := os.Getenv("DISPLAY"); display != "" {
		// Verify the X11 socket actually exists
		if _, err := os.Stat("/tmp/.X11-unix/X" + extractDisplayNum(display)); err == nil {
			return true
		}
		// DISPLAY is set but socket check failed — still trust DISPLAY
		return true
	}
	return false
}

// extractDisplayNum gets the display number from DISPLAY (e.g., ":0" → "0", ":1.0" → "1")
func extractDisplayNum(display string) string {
	d := display
	// Strip hostname if present (e.g., "localhost:0")
	if idx := len(d) - 1; idx >= 0 {
		for i := 0; i < len(d); i++ {
			if d[i] == ':' {
				d = d[i+1:]
				break
			}
		}
	}
	// Strip screen number (e.g., "0.0" → "0")
	for i := 0; i < len(d); i++ {
		if d[i] == '.' {
			return d[:i]
		}
	}
	return d
}
