package main

import (
	"embed"

	"ctail/internal/config"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	// Create an instance of the app structure
	app := NewApp()

	// Load saved window geometry before creating the window so Wails
	// uses the correct initial size (avoids a visible resize flash).
	width, height := 1200, 800
	cfg, err := config.NewManager()
	if err == nil {
		s := cfg.GetSettings()
		if s.WindowWidth > 0 && s.WindowHeight > 0 {
			width = s.WindowWidth
			height = s.WindowHeight
		}
		app.preloadedConfig = cfg
	}

	// Create application with options
	err = wails.Run(&options.App{
		Title:            "ctail",
		Width:            width,
		Height:           height,
		MinWidth:         800,
		MinHeight:        500,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		BackgroundColour: &options.RGBA{R: 30, G: 30, B: 46, A: 1},
		OnStartup:        app.startup,
		OnDomReady:       app.domReady,
		OnBeforeClose:    app.beforeClose,
		OnShutdown:       app.shutdown,
		Bind: []interface{}{
			app,
		},
	})

	if err != nil {
		println("Error:", err.Error())
	}
}
