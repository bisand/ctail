package main

import (
	"ctail/internal/config"
	"embed"
	"flag"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/menu"
	"github.com/wailsapp/wails/v2/pkg/menu/keys"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
	"github.com/wailsapp/wails/v2/pkg/options/linux"
	"github.com/wailsapp/wails/v2/pkg/options/mac"
	wailsRuntime "github.com/wailsapp/wails/v2/pkg/runtime"
)

//go:embed all:frontend/dist
var assets embed.FS

//go:embed build/appicon.png
var appIcon []byte

// Set via -ldflags at build time
var buildNumber = "dev"
var version = "0.0.0-dev"

func main() {
	useX11 := flag.Bool("x11", false, "Force X11 backend (fixes multi-monitor maximize on Wayland)")
	useWayland := flag.Bool("wayland", false, "Force native Wayland backend")
	disableDmabuf := flag.Bool("disable-dmabuf", false, "Set WEBKIT_DISABLE_DMABUF_RENDERER=1 (fixes blank/corrupt window on some GPUs)")
	softwareRender := flag.Bool("software-render", false, "Disable GPU compositing entirely (fixes rendering corruption on multi-monitor)")
	flag.Parse()

	app := NewApp(version, buildNumber)

	// Collect file paths from positional CLI arguments (e.g. ctail file1.log file2.log)
	// These are opened after the frontend has restored saved tabs.
	// Only resolve to absolute paths here — don't stat/validate because the file
	// might be on a slow network mount and we must not block before wails.Run().
	if args := flag.Args(); len(args) > 0 {
		var filePaths []string
		for _, arg := range args {
			if strings.HasPrefix(arg, "-") {
				continue
			}
			abs, err := filepath.Abs(arg)
			if err != nil {
				continue
			}
			filePaths = append(filePaths, abs)
		}
		app.pendingFiles = filePaths
	}

	// Pre-load config to populate recent files menu and window state
	cfg, _ := config.NewManager()
	app.preloadedConfig = cfg

	if runtime.GOOS == "linux" {
		setDisplayBackend(*useX11, *useWayland, cfg)
		setWebKitEnv(*disableDmabuf, *softwareRender, cfg)
	}

	gpuPolicy := resolveGpuPolicy(*softwareRender, cfg)

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
	fileMenu.AddText("Reopen Closed Tab", keys.Combo("t", keys.CmdOrCtrlKey, keys.ShiftKey), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:reopen-tab")
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

	// Tools menu
	toolsMenu := appMenu.AddSubmenu("Tools")
	toolsMenu.AddText("AI Assistant...", keys.Combo("a", keys.CmdOrCtrlKey, keys.ShiftKey), func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:ai-assistant")
	})

	// Help menu
	helpMenu := appMenu.AddSubmenu("Help")
	helpMenu.AddText("Check for Updates", nil, func(_ *menu.CallbackData) {
		wailsRuntime.EventsEmit(app.ctx, "menu:check-updates")
	})
	helpMenu.AddSeparator()
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
		SingleInstanceLock: &options.SingleInstanceLock{
			UniqueId:               "ctail-e7a1b2c3-4d5e-6f7a-8b9c-0d1e2f3a4b5c",
			OnSecondInstanceLaunch: app.onSecondInstance,
		},
		Linux: &linux.Options{
			Icon:             appIcon,
			ProgramName:      "ctail",
			WebviewGpuPolicy: gpuPolicy,
		},
		Mac: &mac.Options{
			About: &mac.AboutInfo{
				Title:   "ctail",
				Message: "Log file viewer with real-time tailing and regex highlighting",
				Icon:    appIcon,
			},
			OnFileOpen: app.handleFileOpen,
		},
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
		// Auto: let GTK choose the native display backend.
		// Don't force anything — GTK auto-detects Wayland or X11 correctly.
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

// setWebKitEnv configures WebKit2GTK environment variables.  CLI flags take
// priority, then the persisted gpuPolicy setting, then the legacy disableDmabuf
// bool.
//
// Policies:
//   - "auto"           — no env vars (default, GPU-accelerated rendering)
//   - "disable-dmabuf" — WEBKIT_DISABLE_DMABUF_RENDERER=1
//   - "software"       — WEBKIT_DISABLE_DMABUF_RENDERER=1 +
//                         WEBKIT_DISABLE_COMPOSITING_MODE=1
//
// The "software" policy forces WebKit2GTK into a pure-software rendering path,
// which avoids the GPU compositor corruption seen on multi-monitor setups and
// when display topology changes (monitor hotplug, VPN reconnect).
// See https://github.com/wailsapp/wails/issues/4985
//     https://bugs.archlinux.org/task/79783
func setWebKitEnv(cliDisableDmabuf, cliSoftwareRender bool, cfg *config.Manager) {
	// CLI flags override everything
	if cliSoftwareRender {
		os.Setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
		os.Setenv("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
		return
	}
	if cliDisableDmabuf {
		os.Setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
		return
	}

	if cfg == nil {
		return
	}

	s := cfg.GetSettings()

	// Use gpuPolicy if set, otherwise fall back to legacy disableDmabuf bool
	policy := s.GpuPolicy
	if policy == "" && s.DisableDmabuf {
		policy = "disable-dmabuf"
	}

	switch policy {
	case "software":
		os.Setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
		os.Setenv("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
	case "disable-dmabuf":
		os.Setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
	}
}

// resolveGpuPolicy maps CLI flags and config settings to a Wails
// WebviewGpuPolicy value.  Default is "on-demand" (GPU accelerated).
func resolveGpuPolicy(cliSoftwareRender bool, cfg *config.Manager) linux.WebviewGpuPolicy {
	if runtime.GOOS != "linux" {
		return linux.WebviewGpuPolicyOnDemand
	}

	policy := ""
	if cfg != nil {
		s := cfg.GetSettings()
		policy = s.GpuPolicy
		if policy == "" && s.DisableDmabuf {
			policy = "software"
		}
	}
	if cliSoftwareRender {
		policy = "software"
	}

	switch policy {
	case "software", "disable-dmabuf":
		return linux.WebviewGpuPolicyNever
	default:
		// "auto" or unset → GPU accelerated
		return linux.WebviewGpuPolicyOnDemand
	}
}
