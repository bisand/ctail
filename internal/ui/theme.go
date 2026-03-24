package ui

import (
	"image/color"
	"strconv"
	"strings"

	"ctail/internal/config"
)

// Colors holds the resolved NRGBA colors for the current theme.
type Colors struct {
	BgPrimary     color.NRGBA
	BgSecondary   color.NRGBA
	BgSurface     color.NRGBA
	BgHover       color.NRGBA
	TextPrimary   color.NRGBA
	TextSecondary color.NRGBA
	TextMuted     color.NRGBA
	Accent        color.NRGBA
	AccentHover   color.NRGBA
	Border        color.NRGBA
	Danger        color.NRGBA
	Success       color.NRGBA
	Warning       color.NRGBA
	TabActive     color.NRGBA
	TabInactive   color.NRGBA
	BadgeColor    color.NRGBA
	ScrollTrack   color.NRGBA
	ScrollThumb   color.NRGBA
}

// DefaultColors returns Catppuccin Mocha dark colors.
func DefaultColors() Colors {
	return Colors{
		BgPrimary:     hexColor("#1e1e2e"),
		BgSecondary:   hexColor("#181825"),
		BgSurface:     hexColor("#313244"),
		BgHover:       hexColor("#45475a"),
		TextPrimary:   hexColor("#cdd6f4"),
		TextSecondary: hexColor("#a6adc8"),
		TextMuted:     hexColor("#6c7086"),
		Accent:        hexColor("#89b4fa"),
		AccentHover:   hexColor("#74c7ec"),
		Border:        hexColor("#45475a"),
		Danger:        hexColor("#f38ba8"),
		Success:       hexColor("#a6e3a1"),
		Warning:       hexColor("#f9e2af"),
		TabActive:     hexColor("#1e1e2e"),
		TabInactive:   hexColor("#181825"),
		BadgeColor:    hexColor("#f9e2af"),
		ScrollTrack:   hexColor("#181825"),
		ScrollThumb:   hexColor("#45475a"),
	}
}

// ColorsFromTheme converts a config.ThemeColors to Colors.
func ColorsFromTheme(tc config.ThemeColors) Colors {
	return Colors{
		BgPrimary:     hexColor(tc.BgPrimary),
		BgSecondary:   hexColor(tc.BgSecondary),
		BgSurface:     hexColor(tc.BgSurface),
		BgHover:       hexColor(tc.BgHover),
		TextPrimary:   hexColor(tc.TextPrimary),
		TextSecondary: hexColor(tc.TextSecondary),
		TextMuted:     hexColor(tc.TextMuted),
		Accent:        hexColor(tc.Accent),
		AccentHover:   hexColor(tc.AccentHover),
		Border:        hexColor(tc.Border),
		Danger:        hexColor(tc.Danger),
		Success:       hexColor(tc.Success),
		Warning:       hexColor(tc.Warning),
		TabActive:     hexColor(tc.TabActive),
		TabInactive:   hexColor(tc.TabInactive),
		BadgeColor:    hexColor(tc.BadgeColor),
		ScrollTrack:   hexColor(tc.ScrollTrack),
		ScrollThumb:   hexColor(tc.ScrollThumb),
	}
}

// hexColor parses a hex color string like "#1e1e2e" to NRGBA.
func hexColor(hex string) color.NRGBA {
	hex = strings.TrimPrefix(hex, "#")
	if len(hex) == 3 {
		hex = string([]byte{hex[0], hex[0], hex[1], hex[1], hex[2], hex[2]})
	}
	if len(hex) != 6 {
		return color.NRGBA{A: 255}
	}
	r, _ := strconv.ParseUint(hex[0:2], 16, 8)
	g, _ := strconv.ParseUint(hex[2:4], 16, 8)
	b, _ := strconv.ParseUint(hex[4:6], 16, 8)
	return color.NRGBA{R: uint8(r), G: uint8(g), B: uint8(b), A: 255}
}

// HexToNRGBA is exported for use in highlight rendering.
func HexToNRGBA(hex string) color.NRGBA {
	return hexColor(hex)
}
