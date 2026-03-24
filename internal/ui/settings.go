package ui

import (
	"fmt"
	"image"
	"strconv"

	"gioui.org/layout"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"

	"ctail/internal/config"
)

// SettingsPanel provides a UI for editing application settings.
type SettingsPanel struct {
	Visible bool

	// Widgets
	CloseBtn       widget.Clickable
	LineNumbersChk widget.Bool
	WordWrapChk    widget.Bool

	// Theme selection
	ThemeList     []string
	ThemeSelected int
	ThemePrev     widget.Clickable
	ThemeNext     widget.Clickable

	// Theme mode
	DarkModeBtn  widget.Clickable
	LightModeBtn widget.Clickable

	// Font size
	FontSizeInc widget.Clickable
	FontSizeDec widget.Clickable

	// Profile selection
	ProfileList     []string
	ProfileSelected int
	ProfilePrev     widget.Clickable
	ProfileNext     widget.Clickable

	initialized bool
}

// SettingsAction indicates a settings change.
type SettingsAction int

const (
	SettingsNone SettingsAction = iota
	SettingsClose
	SettingsChanged
)

// Init loads current settings into the panel widgets.
func (sp *SettingsPanel) Init(cfg *config.Manager, s config.AppSettings) {
	sp.LineNumbersChk.Value = s.ShowLineNumbers
	sp.WordWrapChk.Value = s.WordWrap

	// Load themes
	themes := cfg.ListThemes()
	sp.ThemeList = make([]string, len(themes))
	sp.ThemeSelected = 0
	for i, t := range themes {
		sp.ThemeList[i] = t.Name
		if t.Name == s.Theme {
			sp.ThemeSelected = i
		}
	}

	// Load profiles
	sp.ProfileList = cfg.ListProfiles()
	sp.ProfileSelected = 0
	for i, p := range sp.ProfileList {
		if p == s.ActiveProfile {
			sp.ProfileSelected = i
		}
	}

	sp.initialized = true
}

// Layout renders the settings panel and returns what changed.
func (sp *SettingsPanel) Layout(gtx layout.Context, th *material.Theme, colors Colors,
	settings *config.AppSettings) (SettingsAction, layout.Dimensions) {

	if !sp.Visible {
		return SettingsNone, layout.Dimensions{}
	}

	action := SettingsNone
	changed := false

	// Handle close
	if sp.CloseBtn.Clicked(gtx) {
		sp.Visible = false
		return SettingsClose, layout.Dimensions{}
	}

	// Handle checkbox changes
	if sp.LineNumbersChk.Update(gtx) {
		settings.ShowLineNumbers = sp.LineNumbersChk.Value
		changed = true
	}
	if sp.WordWrapChk.Update(gtx) {
		settings.WordWrap = sp.WordWrapChk.Value
		changed = true
	}

	// Theme navigation
	if sp.ThemePrev.Clicked(gtx) && len(sp.ThemeList) > 0 {
		sp.ThemeSelected = (sp.ThemeSelected - 1 + len(sp.ThemeList)) % len(sp.ThemeList)
		settings.Theme = sp.ThemeList[sp.ThemeSelected]
		changed = true
	}
	if sp.ThemeNext.Clicked(gtx) && len(sp.ThemeList) > 0 {
		sp.ThemeSelected = (sp.ThemeSelected + 1) % len(sp.ThemeList)
		settings.Theme = sp.ThemeList[sp.ThemeSelected]
		changed = true
	}

	// Theme mode
	if sp.DarkModeBtn.Clicked(gtx) {
		settings.ThemeMode = "dark"
		changed = true
	}
	if sp.LightModeBtn.Clicked(gtx) {
		settings.ThemeMode = "light"
		changed = true
	}

	// Font size
	if sp.FontSizeInc.Clicked(gtx) {
		settings.FontSize++
		if settings.FontSize > 32 {
			settings.FontSize = 32
		}
		changed = true
	}
	if sp.FontSizeDec.Clicked(gtx) {
		settings.FontSize--
		if settings.FontSize < 8 {
			settings.FontSize = 8
		}
		changed = true
	}

	// Profile navigation
	if sp.ProfilePrev.Clicked(gtx) && len(sp.ProfileList) > 0 {
		sp.ProfileSelected = (sp.ProfileSelected - 1 + len(sp.ProfileList)) % len(sp.ProfileList)
		settings.ActiveProfile = sp.ProfileList[sp.ProfileSelected]
		changed = true
	}
	if sp.ProfileNext.Clicked(gtx) && len(sp.ProfileList) > 0 {
		sp.ProfileSelected = (sp.ProfileSelected + 1) % len(sp.ProfileList)
		settings.ActiveProfile = sp.ProfileList[sp.ProfileSelected]
		changed = true
	}

	if changed {
		action = SettingsChanged
	}

	// Panel width: right side, 320dp wide
	panelWidth := gtx.Dp(unit.Dp(320))
	if panelWidth > gtx.Constraints.Max.X {
		panelWidth = gtx.Constraints.Max.X
	}

	dims := layout.Stack{}.Layout(gtx,
		// Background
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(panelWidth, gtx.Constraints.Max.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.BgSurface}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		// Left border
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Dp(unit.Dp(1)), gtx.Constraints.Max.Y)
			return FillRect(gtx, colors.Border, size)
		}),
		// Content
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			gtx.Constraints.Max.X = panelWidth
			gtx.Constraints.Min.X = panelWidth
			return layout.Inset{Top: unit.Dp(8), Left: unit.Dp(12), Right: unit.Dp(12), Bottom: unit.Dp(8)}.Layout(gtx,
				func(gtx layout.Context) layout.Dimensions {
					return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
						// Header with close button
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return layout.Flex{Axis: layout.Horizontal, Spacing: layout.SpaceBetween, Alignment: layout.Middle}.Layout(gtx,
								layout.Rigid(func(gtx layout.Context) layout.Dimensions {
									lbl := material.Label(th, unit.Sp(16), "Settings")
									lbl.Color = colors.TextPrimary
									return lbl.Layout(gtx)
								}),
								layout.Flexed(1, layout.Spacer{}.Layout),
								layout.Rigid(func(gtx layout.Context) layout.Dimensions {
									btn := material.Button(th, &sp.CloseBtn, "✕")
									btn.Background = colors.BgSurface
									btn.Color = colors.TextMuted
									btn.TextSize = unit.Sp(14)
									btn.Inset = layout.Inset{
										Top: unit.Dp(2), Bottom: unit.Dp(2),
										Left: unit.Dp(6), Right: unit.Dp(6),
									}
									return btn.Layout(gtx)
								}),
							)
						}),
						// Separator
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return layout.Inset{Top: unit.Dp(8), Bottom: unit.Dp(8)}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
								size := image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(1)))
								return FillRect(gtx, colors.Border, size)
							})
						}),
						// Theme selector
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							themeName := "default"
							if sp.ThemeSelected >= 0 && sp.ThemeSelected < len(sp.ThemeList) {
								themeName = sp.ThemeList[sp.ThemeSelected]
							}
							return sp.layoutSelector(gtx, th, colors, "Theme", themeName, &sp.ThemePrev, &sp.ThemeNext)
						}),
						// Spacer
						layout.Rigid(layout.Spacer{Height: unit.Dp(8)}.Layout),
						// Theme mode (dark/light buttons)
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return sp.layoutModeToggle(gtx, th, colors, settings.ThemeMode)
						}),
						// Spacer
						layout.Rigid(layout.Spacer{Height: unit.Dp(8)}.Layout),
						// Profile selector
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							profName := "(none)"
							if sp.ProfileSelected >= 0 && sp.ProfileSelected < len(sp.ProfileList) {
								profName = sp.ProfileList[sp.ProfileSelected]
							}
							return sp.layoutSelector(gtx, th, colors, "Profile", profName, &sp.ProfilePrev, &sp.ProfileNext)
						}),
						// Spacer
						layout.Rigid(layout.Spacer{Height: unit.Dp(8)}.Layout),
						// Font size
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return sp.layoutSelector(gtx, th, colors, "Font Size", strconv.Itoa(settings.FontSize), &sp.FontSizeDec, &sp.FontSizeInc)
						}),
						// Spacer
						layout.Rigid(layout.Spacer{Height: unit.Dp(12)}.Layout),
						// Checkboxes
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							chk := material.CheckBox(th, &sp.LineNumbersChk, "Show line numbers")
							chk.Size = unit.Dp(18)
							chk.IconColor = colors.Accent
							chk.TextSize = unit.Sp(13)
							chk.Color = colors.TextPrimary
							return chk.Layout(gtx)
						}),
						layout.Rigid(layout.Spacer{Height: unit.Dp(4)}.Layout),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							chk := material.CheckBox(th, &sp.WordWrapChk, "Word wrap")
							chk.Size = unit.Dp(18)
							chk.IconColor = colors.Accent
							chk.TextSize = unit.Sp(13)
							chk.Color = colors.TextPrimary
							return chk.Layout(gtx)
						}),
					)
				},
			)
		}),
	)

	return action, dims
}

func (sp *SettingsPanel) layoutSelector(gtx layout.Context, th *material.Theme, colors Colors,
	label, value string, prev, next *widget.Clickable) layout.Dimensions {

	return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			lbl := material.Label(th, unit.Sp(13), label)
			lbl.Color = colors.TextSecondary
			gtx.Constraints.Min.X = gtx.Dp(unit.Dp(80))
			return lbl.Layout(gtx)
		}),
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			btn := material.Button(th, prev, "◀")
			btn.Background = colors.BgSecondary
			btn.Color = colors.TextPrimary
			btn.TextSize = unit.Sp(11)
			btn.Inset = layout.UniformInset(unit.Dp(4))
			return btn.Layout(gtx)
		}),
		layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
			return layout.Center.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
				lbl := material.Label(th, unit.Sp(13), value)
				lbl.Color = colors.TextPrimary
				lbl.MaxLines = 1
				return lbl.Layout(gtx)
			})
		}),
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			btn := material.Button(th, next, "▶")
			btn.Background = colors.BgSecondary
			btn.Color = colors.TextPrimary
			btn.TextSize = unit.Sp(11)
			btn.Inset = layout.UniformInset(unit.Dp(4))
			return btn.Layout(gtx)
		}),
	)
}

func (sp *SettingsPanel) layoutModeToggle(gtx layout.Context, th *material.Theme, colors Colors, mode string) layout.Dimensions {
	return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			lbl := material.Label(th, unit.Sp(13), "Mode")
			lbl.Color = colors.TextSecondary
			gtx.Constraints.Min.X = gtx.Dp(unit.Dp(80))
			return lbl.Layout(gtx)
		}),
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			btn := material.Button(th, &sp.DarkModeBtn, "Dark")
			if mode == "dark" {
				btn.Background = colors.Accent
			} else {
				btn.Background = colors.BgSecondary
				btn.Color = colors.TextPrimary
			}
			btn.TextSize = unit.Sp(12)
			btn.Inset = layout.Inset{
				Top: unit.Dp(4), Bottom: unit.Dp(4),
				Left: unit.Dp(12), Right: unit.Dp(12),
			}
			return btn.Layout(gtx)
		}),
		layout.Rigid(layout.Spacer{Width: unit.Dp(4)}.Layout),
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			btn := material.Button(th, &sp.LightModeBtn, "Light")
			if mode == "light" {
				btn.Background = colors.Accent
			} else {
				btn.Background = colors.BgSecondary
				btn.Color = colors.TextPrimary
			}
			btn.TextSize = unit.Sp(12)
			btn.Inset = layout.Inset{
				Top: unit.Dp(4), Bottom: unit.Dp(4),
				Left: unit.Dp(12), Right: unit.Dp(12),
			}
			return btn.Layout(gtx)
		}),
	)
}

// FormatSettingsInfo returns a debug string of current settings.
func FormatSettingsInfo(s config.AppSettings) string {
	return fmt.Sprintf("Theme: %s (%s) | Font: %d | Lines: %v | Profile: %s",
		s.Theme, s.ThemeMode, s.FontSize, s.ShowLineNumbers, s.ActiveProfile)
}
