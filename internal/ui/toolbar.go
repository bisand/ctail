package ui

import (
	"image"
	"image/color"

	"gioui.org/layout"
	"gioui.org/op"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"
)

// Toolbar renders the menu bar and toolbar area.
type Toolbar struct {
	// Menu bar buttons
	FileBtn     widget.Clickable
	EditBtn     widget.Clickable
	ViewBtn     widget.Clickable
	HelpBtn     widget.Clickable

	// Dropdown state
	OpenMenu    string // which menu is open: "file", "edit", "view", "help", ""
	menuItems   map[string][]menuItem

	// Measured heights for dropdown positioning
	menuBarHeight    int
	toolbarRowHeight int

	// Menu item clickables (persistent across frames)
	miOpen       widget.Clickable
	miCloseTab   widget.Clickable
	miQuit       widget.Clickable
	miFind       widget.Clickable
	miSettings   widget.Clickable
	miToggleTheme widget.Clickable
	miAbout      widget.Clickable

	// Toolbar widgets
	FollowChk widget.Bool
}

type menuItem struct {
	label     string
	shortcut  string
	clickable *widget.Clickable
	separator bool
	action    ToolbarAction
}

// ToolbarAction indicates which toolbar action was triggered.
type ToolbarAction int

const (
	ToolbarNone ToolbarAction = iota
	ToolbarOpen
	ToolbarCloseTab
	ToolbarQuit
	ToolbarFind
	ToolbarSettings
	ToolbarToggleTheme
	ToolbarAbout
	ToolbarFollowChanged
)

func (tb *Toolbar) initMenuItems() {
	if tb.menuItems != nil {
		return
	}
	tb.menuItems = map[string][]menuItem{
		"file": {
			{label: "Open File...", shortcut: "Ctrl+O", clickable: &tb.miOpen, action: ToolbarOpen},
			{separator: true},
			{label: "Close Tab", shortcut: "Ctrl+W", clickable: &tb.miCloseTab, action: ToolbarCloseTab},
			{separator: true},
			{label: "Quit", shortcut: "Ctrl+Q", clickable: &tb.miQuit, action: ToolbarQuit},
		},
		"edit": {
			{label: "Find...", shortcut: "Ctrl+F", clickable: &tb.miFind, action: ToolbarFind},
		},
		"view": {
			{label: "Settings", shortcut: "Ctrl+,", clickable: &tb.miSettings, action: ToolbarSettings},
			{separator: true},
			{label: "Toggle Theme", clickable: &tb.miToggleTheme, action: ToolbarToggleTheme},
		},
		"help": {
			{label: "About ctail", clickable: &tb.miAbout, action: ToolbarAbout},
		},
	}
}

// Layout draws the menu bar and toolbar row, returns the triggered action.
// Call LayoutDropdown separately as an overlay after the main layout.
func (tb *Toolbar) Layout(gtx layout.Context, th *material.Theme, colors Colors, autoScroll bool) (ToolbarAction, layout.Dimensions) {
	tb.initMenuItems()
	action := ToolbarNone

	// Check menu bar button clicks (toggle dropdown)
	if tb.FileBtn.Clicked(gtx) {
		tb.toggleMenu("file")
	}
	if tb.EditBtn.Clicked(gtx) {
		tb.toggleMenu("edit")
	}
	if tb.ViewBtn.Clicked(gtx) {
		tb.toggleMenu("view")
	}
	if tb.HelpBtn.Clicked(gtx) {
		tb.toggleMenu("help")
	}

	// Check dropdown item clicks
	for _, items := range tb.menuItems {
		for _, mi := range items {
			if mi.clickable != nil && mi.clickable.Clicked(gtx) {
				action = mi.action
				tb.OpenMenu = ""
			}
		}
	}

	// Sync checkbox state with tab's auto-scroll
	tb.FollowChk.Value = autoScroll

	// Detect user click on checkbox (Update toggles Value and returns true)
	if tb.FollowChk.Update(gtx) {
		action = ToolbarFollowChanged
	}

	dims := layout.Flex{Axis: layout.Vertical}.Layout(gtx,
		// Menu bar row
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			d := tb.layoutMenuBar(gtx, th, colors)
			tb.menuBarHeight = d.Size.Y
			return d
		}),
		// Toolbar row (follow checkbox etc.)
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			d := tb.layoutToolbarRow(gtx, th, colors, autoScroll)
			tb.toolbarRowHeight = d.Size.Y
			return d
		}),
	)

	return action, dims
}

// LayoutDropdown renders the dropdown menu as an overlay. Must be called
// after the main layout so it paints on top of other content.
func (tb *Toolbar) LayoutDropdown(gtx layout.Context, th *material.Theme, colors Colors) layout.Dimensions {
	if tb.OpenMenu == "" {
		return layout.Dimensions{}
	}

	items, ok := tb.menuItems[tb.OpenMenu]
	if !ok || len(items) == 0 {
		return layout.Dimensions{}
	}

	// Calculate X offset based on which menu is open
	xOff := tb.menuXOffset(tb.OpenMenu)
	yOff := tb.menuBarHeight // position just below menu bar

	defer op.Offset(image.Pt(xOff, yOff)).Push(gtx.Ops).Pop()

	// Constrain dropdown width
	ddGtx := gtx
	ddGtx.Constraints.Min.X = gtx.Dp(unit.Dp(220))
	ddGtx.Constraints.Max.X = gtx.Dp(unit.Dp(280))

	// Background + border + items
	return layout.Stack{}.Layout(ddGtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Min.X, gtx.Constraints.Min.Y)
			// Shadow/border
			borderRect := image.Rect(-1, -1, size.X+1, size.Y+1)
			defer clip.Rect(borderRect).Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.Border}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Min.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.BgSurface}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			children := make([]layout.FlexChild, 0, len(items))
			for _, mi := range items {
				mi := mi
				if mi.separator {
					children = append(children, layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						size := image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(1)))
						return FillRect(gtx, colors.Border, size)
					}))
					continue
				}
				children = append(children, layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					return tb.layoutMenuItem(gtx, th, colors, mi)
				}))
			}
			return layout.Flex{Axis: layout.Vertical}.Layout(gtx, children...)
		}),
	)
}

// CloseMenu closes any open dropdown (call on click outside).
func (tb *Toolbar) CloseMenu() {
	tb.OpenMenu = ""
}

func (tb *Toolbar) menuXOffset(name string) int {
	// Approximate pixel offsets for each menu button.
	// Each button is ~padding(8)+text+padding(8). Approximate widths:
	switch name {
	case "file":
		return 4
	case "edit":
		return 50
	case "view":
		return 95
	case "help":
		return 145
	}
	return 0
}

func (tb *Toolbar) toggleMenu(name string) {
	if tb.OpenMenu == name {
		tb.OpenMenu = ""
	} else {
		tb.OpenMenu = name
	}
}

func (tb *Toolbar) layoutMenuBar(gtx layout.Context, th *material.Theme, colors Colors) layout.Dimensions {
	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.BgSecondary}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{Left: unit.Dp(4), Right: unit.Dp(4)}.Layout(gtx,
				func(gtx layout.Context) layout.Dimensions {
					return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return tb.layoutMenuButton(gtx, th, colors, &tb.FileBtn, "File", tb.OpenMenu == "file")
						}),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return tb.layoutMenuButton(gtx, th, colors, &tb.EditBtn, "Edit", tb.OpenMenu == "edit")
						}),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return tb.layoutMenuButton(gtx, th, colors, &tb.ViewBtn, "View", tb.OpenMenu == "view")
						}),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return tb.layoutMenuButton(gtx, th, colors, &tb.HelpBtn, "Help", tb.OpenMenu == "help")
						}),
					)
				},
			)
		}),
	)
}

func (tb *Toolbar) layoutMenuButton(gtx layout.Context, th *material.Theme, colors Colors,
	btn *widget.Clickable, label string, active bool) layout.Dimensions {

	bg := colors.BgSecondary
	if active {
		bg = colors.BgHover
	}
	if btn.Hovered() && !active {
		bg = blendColor(colors.BgSecondary, colors.BgHover, 0.5)
	}

	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Min.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: bg}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return material.Clickable(gtx, btn, func(gtx layout.Context) layout.Dimensions {
				return layout.Inset{
					Top: unit.Dp(4), Bottom: unit.Dp(4),
					Left: unit.Dp(8), Right: unit.Dp(8),
				}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
					lbl := material.Label(th, unit.Sp(13), label)
					lbl.Color = colors.TextPrimary
					return lbl.Layout(gtx)
				})
			})
		}),
	)
}

func (tb *Toolbar) layoutToolbarRow(gtx layout.Context, th *material.Theme, colors Colors, autoScroll bool) layout.Dimensions {
	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.BgSurface}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{
				Top: unit.Dp(3), Bottom: unit.Dp(3),
				Left: unit.Dp(8), Right: unit.Dp(8),
			}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
				return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
					// Follow/tail checkbox
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						chk := material.CheckBox(th, &tb.FollowChk, "Follow tail")
						chk.Size = unit.Dp(18)
						chk.IconColor = colors.Accent
						chk.TextSize = unit.Sp(13)
						chk.Color = colors.TextSecondary
						return chk.Layout(gtx)
					}),
				)
			})
		}),
	)
}

func (tb *Toolbar) layoutMenuItem(gtx layout.Context, th *material.Theme, colors Colors, mi menuItem) layout.Dimensions {
	bg := colors.BgSurface
	if mi.clickable.Hovered() {
		bg = colors.BgHover
	}

	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: bg}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return material.Clickable(gtx, mi.clickable, func(gtx layout.Context) layout.Dimensions {
				return layout.Inset{
					Top: unit.Dp(6), Bottom: unit.Dp(6),
					Left: unit.Dp(16), Right: unit.Dp(16),
				}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
					return layout.Flex{Axis: layout.Horizontal, Spacing: layout.SpaceBetween, Alignment: layout.Middle}.Layout(gtx,
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							lbl := material.Label(th, unit.Sp(13), mi.label)
							lbl.Color = colors.TextPrimary
							return lbl.Layout(gtx)
						}),
						layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
							if mi.shortcut == "" {
								return layout.Dimensions{}
							}
							return layout.Inset{Left: unit.Dp(32)}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
								lbl := material.Label(th, unit.Sp(11), mi.shortcut)
								lbl.Color = colors.TextMuted
								return lbl.Layout(gtx)
							})
						}),
					)
				})
			})
		}),
	)
}

func blendColor(a, b color.NRGBA, t float32) color.NRGBA {
	return color.NRGBA{
		R: uint8(float32(a.R)*(1-t) + float32(b.R)*t),
		G: uint8(float32(a.G)*(1-t) + float32(b.G)*t),
		B: uint8(float32(a.B)*(1-t) + float32(b.B)*t),
		A: uint8(float32(a.A)*(1-t) + float32(b.A)*t),
	}
}
